//! Single-connection sweep — generic grid or probe-expanded discover profile.
//!
//!   cargo run --bin ble_sweep -- --device UUID --profile discover --probe test_results.md
//!   cargo run --bin ble_sweep -- --offline --profile discover --probe test_results.md --output sweep_results.md

use anyhow::{Context, Result};
use ble_hack_skill::probe_analyze::{
    analyze_probe, expand_sweep_from_probe, format_sweep_md, parse_probe_md,
    synthesize_sweep_from_probe,
};
use ble_hack_skill::session::{
    adapter, classify_response, connect, send_and_wait, send_burst, spaced_hex, ChannelPair,
};
use btleplug::api::{bleuuid::uuid_from_u16, Peripheral};
use std::fs;
use std::path::Path;
use std::time::Duration;

fn default_channel() -> ChannelPair {
    ChannelPair {
        label: "FFE1/FFE2".into(),
        rx: uuid_from_u16(0xFFE1),
        tx: uuid_from_u16(0xFFE2),
    }
}

struct Row {
    label: String,
    sent: String,
    response: String,
    class: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let offline = args.iter().any(|a| a == "--offline");
    let device = arg_value(&args, "--device")
        .unwrap_or_else(|| "offline".into());
    let output = arg_value(&args, "--output").unwrap_or_else(|| "sweep_results.md".into());
    let profile = arg_value(&args, "--profile").unwrap_or_else(|| "generic".into());
    let probe_path = arg_value(&args, "--probe").unwrap_or_else(|| "test_results.md".into());

    let rows = if offline {
        if profile != "discover" {
            anyhow::bail!("--offline requires --profile discover");
        }
        let probe_md = fs::read_to_string(&probe_path)
            .with_context(|| format!("read probe results: {probe_path}"))?;
        let probe_rows = parse_probe_md(&probe_md);
        let analysis = analyze_probe(&probe_rows);
        let synth = synthesize_sweep_from_probe(&probe_rows, &analysis);
        println!(
            "=== Offline discover sweep: {} predicted hits from {} expanded frames ===\n",
            synth.len(),
            expand_sweep_from_probe(&analysis).len()
        );
        synth
            .into_iter()
            .map(|(label, sent, response, class)| Row {
                label,
                sent,
                response,
                class,
            })
            .collect()
    } else {
        let device = arg_value(&args, "--device").context("--device required without --offline")?;
        let adpt = adapter().await?;
        let session = connect(&adpt, &device, &default_channel()).await?;
        let mut notifications = session.peripheral.notifications().await?;

        let rows = if profile == "discover" {
            let probe_md = fs::read_to_string(&probe_path)
                .with_context(|| format!("read probe results: {probe_path}"))?;
            let analysis = analyze_probe(&parse_probe_md(&probe_md));
            let frames = expand_sweep_from_probe(&analysis);
            println!(
                "=== Discover sweep: {} frames from probe analysis ({}) ===\n",
                frames.len(),
                probe_path
            );
            run_frame_list_sweep(&session, &mut notifications, frames).await?
        } else {
            run_generic_sweep(&session, &mut notifications).await?
        };

        session.peripheral.disconnect().await?;
        rows
    };

    let md = if offline {
        format_sweep_md(
            &device,
            "discover-offline",
            &rows
                .iter()
                .map(|r| {
                    (
                        r.label.clone(),
                        r.sent.clone(),
                        r.response.clone(),
                        r.class.clone(),
                    )
                })
                .collect::<Vec<_>>(),
        )
    } else {
        format_sweep(&device, &profile, &rows)
    };
    fs::write(&output, md)?;
    print_hits(&rows);
    println!("\nWrote {output}");
    Ok(())
}

async fn run_frame_list_sweep(
    session: &ble_hack_skill::session::Session,
    notifications: &mut (impl futures::StreamExt<Item = btleplug::api::ValueNotification> + Unpin),
    frames: Vec<(String, [u8; 7])>,
) -> Result<Vec<Row>> {
    let mut rows = Vec::new();
    for (label, frame) in frames {
        let response = send_and_wait(session, notifications, &frame).await?;
        let resp_str = response
            .as_ref()
            .map(|r| spaced_hex(r))
            .unwrap_or_else(|| "(silent)".into());
        let class = classify_response(&frame, &response).into();
        if class != "silent" {
            println!("  {label}: {} -> {} [{class}]", spaced_hex(&frame), resp_str);
        }
        rows.push(Row {
            label,
            sent: spaced_hex(&frame),
            response: resp_str,
            class,
        });
    }

    let burst_candidates: Vec<_> = rows
        .iter()
        .filter(|r| {
            (r.class == "echo" || r.class == "non-standard")
                && (r.sent.starts_with("55 04") || r.sent.starts_with("55 08 00 03"))
        })
        .take(5)
        .collect();

    println!("\n=== Burst top motor candidates ({}) ===", burst_candidates.len());
    for c in burst_candidates {
        let bytes: Vec<u8> = c
            .sent
            .split_whitespace()
            .map(|b| u8::from_str_radix(b, 16).unwrap())
            .collect();
        println!("Bursting {} for 2s...", c.label);
        let _ = send_burst(session, notifications, &[bytes], Duration::from_secs(2)).await?;
    }

    Ok(rows)
}

async fn run_generic_sweep(
    session: &ble_hack_skill::session::Session,
    notifications: &mut (impl futures::StreamExt<Item = btleplug::api::ValueNotification> + Unpin),
) -> Result<Vec<Row>> {
    let mut rows = Vec::new();
    let opcodes = [0x03u8, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10];
    for opcode in opcodes {
        for mode in 1u8..=5 {
            for intensity in 1u8..=5 {
                let frame = vec![0x55, opcode, 0x00, 0x00, mode, intensity, 0x00];
                let response = send_and_wait(session, notifications, &frame).await?;
                let resp_str = response
                    .as_ref()
                    .map(|r| spaced_hex(r))
                    .unwrap_or_else(|| "(silent)".into());
                let class = classify_response(&frame, &response).into();
                rows.push(Row {
                    label: format!("op_{opcode:02X}_m{mode}_i{intensity}"),
                    sent: spaced_hex(&frame),
                    response: resp_str,
                    class,
                });
            }
        }
    }
    for opcode in 0x03u8..=0x10 {
        for p4 in [0x01u8, 0x05] {
            for p5 in [0x01u8, 0x05, 0xFF] {
                let frame = vec![0x55, opcode, 0x00, 0x00, p4, p5, 0xAA];
                let response = send_and_wait(session, notifications, &frame).await?;
                let resp_str = response
                    .as_ref()
                    .map(|r| spaced_hex(r))
                    .unwrap_or_else(|| "(silent)".into());
                let class = classify_response(&frame, &response).into();
                rows.push(Row {
                    label: format!("aa_op_{opcode:02X}_{p4:02X}_{p5:02X}"),
                    sent: spaced_hex(&frame),
                    response: resp_str,
                    class,
                });
            }
        }
    }
    Ok(rows)
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

fn format_sweep(device: &str, profile: &str, rows: &[Row]) -> String {
    let mut out = format!("# BLE Sweep Results\n\n- Device: `{device}`\n- Profile: `{profile}`\n\n");
    out.push_str("| label | sent | response | class |\n");
    out.push_str("| ----- | ---- | -------- | ----- |\n");
    for r in rows {
        out.push_str(&format!(
            "| {} | `{}` | `{}` | {} |\n",
            r.label, r.sent, r.response, r.class
        ));
    }
    out
}

fn print_hits(rows: &[Row]) {
    let hits: Vec<_> = rows
        .iter()
        .filter(|r| r.class == "echo" || r.class == "non-standard" || r.class == "standard ack")
        .collect();
    println!("\n=== Non-silent responses ({}) ===", hits.len());
    for r in hits.iter().take(20) {
        println!("  {} | {} -> {} [{}]", r.label, r.sent, r.response, r.class);
    }
}

#[allow(dead_code)]
fn _probe_default(p: &str) -> &Path {
    Path::new(p)
}
