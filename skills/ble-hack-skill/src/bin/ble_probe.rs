//! Automated BLE protocol discovery: header probes, opcode sweeps, burst tests.
//!
//! Usage:
//!   cargo run --bin ble_probe -- --device UUID --auto --output test_results.md
//!   cargo run --bin ble_probe -- --device UUID --channel ffe1 --header-sweep
//!   cargo run --bin ble_probe -- --device UUID --channel ffe1 --opcode-sweep --header 55
//!   cargo run --bin ble_probe -- --device UUID --channel ffe1 --burst "55 03 00 00 01 01 00" --seconds 3

use anyhow::{Context, Result};
use ble_hack_skill::crc::{frame_with_aa, frame_with_crc};
use ble_hack_skill::session::{
    adapter, classify_response, connect, hex, send_and_wait, send_burst, send_handshake,
    spaced_hex, ChannelPair, Session,
};
use btleplug::api::{bleuuid::uuid_from_u16, Peripheral};
use futures::StreamExt;
use std::fs;
use std::time::Duration;
use uuid::Uuid;

const CHANNELS: [ChannelPair; 3] = [
    ChannelPair {
        label: "FFE1/FFE2",
        rx: uuid_from_u16(0xFFE1),
        tx: uuid_from_u16(0xFFE2),
    },
    ChannelPair {
        label: "AE01/AE02",
        rx: uuid_from_u16(0xAE01),
        tx: uuid_from_u16(0xAE02),
    },
    ChannelPair {
        label: "FFA1/FFA2",
        rx: uuid_from_u16(0xFFA1),
        tx: uuid_from_u16(0xFFA2),
    },
];

const SVAKOM_HANDSHAKE: [&[u8]; 3] = [
    &[0x55, 0x04, 0x00, 0x00, 0x01, 0xFF, 0xAA],
    &[0x55, 0x04, 0x00, 0x00, 0x00, 0x00, 0xAA],
    &[0x55, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00],
];

struct ProbeRow {
    label: String,
    channel: String,
    sent: String,
    response: String,
    class: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let device = arg_value(&args, "--device").context("--device UUID required")?;
    let output = arg_value(&args, "--output").unwrap_or_else(|| "test_results.md".into());
    let auto = args.iter().any(|a| a == "--auto");
    let header_sweep = args.iter().any(|a| a == "--header-sweep");
    let opcode_sweep = args.iter().any(|a| a == "--opcode-sweep");
    let with_handshake = args.iter().any(|a| a == "--handshake");
    let burst_hex = arg_value(&args, "--burst");
    let burst_secs: u64 = arg_value(&args, "--seconds")
        .and_then(|s| s.parse().ok())
        .unwrap_or(3);
    let header_byte: u8 = arg_value(&args, "--header")
        .and_then(|s| u8::from_str_radix(s.trim_start_matches("0x"), 16).ok())
        .unwrap_or(0x55);

    let channel_filter = arg_value(&args, "--channel").map(|c| parse_channel(&c));

    let mut rows = Vec::new();

    if auto {
        rows.extend(run_auto(&device).await?);
    } else if let Some(hex_str) = burst_hex {
        let channel = channel_filter.context("--channel required for --burst")?;
        let payload = parse_hex(&hex_str)?;
        let adpt = adapter().await?;
        let session = connect(&adpt, &device, &channel).await?;
        let mut notifications = session.peripheral.notifications().await?;
        if with_handshake {
            send_handshake(&session, &mut notifications, &SVAKOM_HANDSHAKE).await?;
        }
        let frames = vec![payload.clone()];
        let response = send_burst(
            &session,
            &mut notifications,
            &frames,
            Duration::from_secs(burst_secs),
        )
        .await?;
        rows.push(ProbeRow {
            label: "burst".into(),
            channel: channel.label.into(),
            sent: spaced_hex(&payload),
            response: response
                .as_ref()
                .map(|r| spaced_hex(r))
                .unwrap_or_else(|| "(no response)".into()),
            class: classify_response(&payload, &response).into(),
        });
        session.peripheral.disconnect().await?;
    } else if header_sweep || opcode_sweep {
        let channels: Vec<ChannelPair> = if let Some(ch) = channel_filter {
            vec![ch]
        } else {
            CHANNELS.to_vec()
        };
        for channel in channels {
            if header_sweep {
                rows.extend(run_header_sweep(&device, &channel, with_handshake).await?);
            }
            if opcode_sweep {
                rows.extend(
                    run_opcode_sweep(&device, &channel, header_byte, with_handshake).await?,
                );
            }
        }
    } else {
        anyhow::bail!(
            "Specify --auto, --header-sweep, --opcode-sweep, or --burst. See --help in source."
        );
    }

    let md = format_results(&device, &rows);
    fs::write(&output, &md)?;
    print_results(&rows);
    println!("\nWrote {output}");
    Ok(())
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

fn parse_channel(s: &str) -> ChannelPair {
    let s = s.to_lowercase();
    CHANNELS
        .iter()
        .find(|c| {
            c.label.to_lowercase().contains(&s)
                || c.rx.to_string().contains(&s)
                || format!("{:04x}", c.rx.as_u128() as u16).contains(&s)
        })
        .cloned()
        .unwrap_or_else(|| {
            let uuid = Uuid::parse_str(&s).ok();
            if let Some(rx) = uuid {
                ChannelPair {
                    label: "custom",
                    rx,
                    tx: uuid_from_u16(0xFFE2),
                }
            } else {
                CHANNELS[0].clone()
            }
        })
}

fn parse_hex(s: &str) -> Result<Vec<u8>> {
    s.split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).context(format!("bad hex byte: {b}")))
        .collect()
}

async fn run_auto(device: &str) -> Result<Vec<ProbeRow>> {
    let mut rows = Vec::new();
    let adpt = adapter().await?;

    println!("=== Phase 1: header probes on all channels ===\n");
    for channel in &CHANNELS {
        match run_header_sweep(device, channel, false).await {
            Ok(mut r) => rows.append(&mut r),
            Err(e) => eprintln!("  {} header sweep failed: {e}", channel.label),
        }
    }

    let best = rows
        .iter()
        .filter(|r| r.channel.contains("FFE1") && r.class == "non-standard")
        .max_by_key(|r| score_row(r))
        .or_else(|| {
            rows.iter()
                .filter(|r| r.class == "non-standard" || r.class == "standard ack")
                .max_by_key(|r| score_row(r))
        })
        .map(|r| r.channel.clone());

    let target_channel = best.as_deref().and_then(|label| {
        CHANNELS
            .iter()
            .find(|c| label.contains(c.label))
            .cloned()
    });

    let channel = target_channel.unwrap_or(CHANNELS[0].clone());
    println!("\n=== Phase 2: opcode sweep on {} (header 0x55) ===\n", channel.label);

    rows.extend(run_opcode_sweep(device, &channel, 0x55, false).await?);

    println!("\n=== Phase 3: opcode sweep with Svakom-style handshake ===\n");
    rows.extend(run_opcode_sweep(device, &channel, 0x55, true).await?);

    println!("\n=== Phase 4: KooSync-style motor candidates (burst 2s) ===\n");

    // Boost family: 0x04 + fixed AA tail
    for scale in [0x00u8, 0x20, 0x40, 0x80, 0xFF] {
        let frame = frame_with_aa([0x55, 0x04, 0x00, 0x00, 0x00, scale]).to_vec();
        match run_burst_probe(device, &channel, &frame, 2, false).await {
            Ok(row) => {
                println!("  boost scale={scale:02X}: {} -> {}", row.sent, row.response);
                rows.push(row);
            }
            Err(e) => eprintln!("  boost scale={scale:02X} failed: {e}"),
        }
    }

    // Stretch / M-mode: 0x08 + CRC-8 C2
    let crc_candidates: [([u8; 6], &str); 6] = [
        ([0x55, 0x08, 0x00, 0x00, 0x01, 0x01], "stretch_l1"),
        ([0x55, 0x08, 0x00, 0x01, 0x00, 0x00], "stretch_stop"),
        ([0x55, 0x08, 0x00, 0x03, 0x01, 0x01], "m1_t1"),
        ([0x55, 0x08, 0x00, 0x03, 0x01, 0x05], "m1_t5"),
        ([0x55, 0x03, 0x00, 0x00, 0x01, 0x01], "legacy_vibe"),
        ([0x55, 0x02, 0x00, 0x00, 0x00, 0x00], "battery_query"),
    ];
    for (bytes, label) in crc_candidates {
        let frame = frame_with_crc(bytes).to_vec();
        match run_burst_probe(device, &channel, &frame, 2, false).await {
            Ok(row) => {
                println!("  {label}: {} -> {}", row.sent, row.response);
                rows.push(row);
            }
            Err(e) => eprintln!("  {label} failed: {e}"),
        }
    }

    // Fredorch-style AE channel: checksum frames
    println!("\n=== Phase 5: AE01 Fredorch-style pattern sweep ===\n");
    rows.extend(run_fredorch_sweep(device).await.unwrap_or_default());

    let _ = adpt;
    Ok(rows)
}

fn score_row(r: &ProbeRow) -> i32 {
    match r.class.as_str() {
        "non-standard" => 100,
        "standard ack" => 50,
        "echo" => 20,
        _ => 0,
    }
}

async fn run_header_sweep(
    device: &str,
    channel: &ChannelPair,
    handshake: bool,
) -> Result<Vec<ProbeRow>> {
    let adpt = adapter().await?;
    let session = connect(&adpt, device, channel).await?;
    let mut notifications = session.peripheral.notifications().await?;
    if handshake {
        send_handshake(&session, &mut notifications, &SVAKOM_HANDSHAKE).await?;
    }

    let headers = [0x00, 0x55, 0xAA, 0xA5, 0x5A, 0xFF];
    let mut rows = Vec::new();

    for h in headers {
        // Probe A: [H] 00 00 00 00 00 00
        let a = vec![h, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        rows.push(probe_frame(
            &session,
            &mut notifications,
            channel,
            &format!("probeA_H={h:02X}"),
            &a,
        )
        .await?);

        // Probe B: [H] 01 00 00 00 00 00
        let b = vec![h, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00];
        rows.push(probe_frame(
            &session,
            &mut notifications,
            channel,
            &format!("probeB_H={h:02X}"),
            &b,
        )
        .await?);

        // Probe C: 03 [H] 00 00
        let c = vec![0x03, h, 0x00, 0x00];
        rows.push(probe_frame(
            &session,
            &mut notifications,
            channel,
            &format!("probeC_H={h:02X}"),
            &c,
        )
        .await?);
    }

    session.peripheral.disconnect().await?;
    Ok(rows)
}

async fn run_opcode_sweep(
    device: &str,
    channel: &ChannelPair,
    header: u8,
    handshake: bool,
) -> Result<Vec<ProbeRow>> {
    let adpt = adapter().await?;
    let session = connect(&adpt, device, channel).await?;
    let mut notifications = session.peripheral.notifications().await?;
    if handshake {
        send_handshake(&session, &mut notifications, &SVAKOM_HANDSHAKE).await?;
    }

    let mut rows = Vec::new();
    for opcode in 0x00..=0x20u8 {
        let frame = vec![header, opcode, 0x00, 0x00, 0x01, 0x01, 0x00];
        let row = probe_frame(
            &session,
            &mut notifications,
            channel,
            &format!("opcode_{opcode:02X}"),
            &frame,
        )
        .await?;
        if row.class != "silent" {
            println!(
                "  {} {:02X}: {} -> {} [{}]",
                channel.label, opcode, row.sent, row.response, row.class
            );
        }
        rows.push(row);
    }

    session.peripheral.disconnect().await?;
    Ok(rows)
}

async fn run_burst_probe(
    device: &str,
    channel: &ChannelPair,
    frame: &[u8],
    seconds: u64,
    handshake: bool,
) -> Result<ProbeRow> {
    let adpt = adapter().await?;
    let session = connect(&adpt, device, channel).await?;
    let mut notifications = session.peripheral.notifications().await?;
    if handshake {
        send_handshake(&session, &mut notifications, &SVAKOM_HANDSHAKE).await?;
    }
    let response = send_burst(
        &session,
        &mut notifications,
        &[frame.to_vec()],
        Duration::from_secs(seconds),
    )
    .await?;
    let row = ProbeRow {
        label: format!("burst_{}", hex(frame)),
        channel: channel.label.into(),
        sent: spaced_hex(frame),
        response: response
            .as_ref()
            .map(|r| spaced_hex(r))
            .unwrap_or_else(|| "(no response)".into()),
        class: classify_response(frame, &response).into(),
    };
    session.peripheral.disconnect().await?;
    Ok(row)
}

async fn run_fredorch_sweep(device: &str) -> Result<Vec<ProbeRow>> {
    let channel = &CHANNELS[1]; // AE01/AE02
    let adpt = adapter().await?;
    let session = connect(&adpt, device, channel).await?;
    let mut notifications = session.peripheral.notifications().await?;
    let mut rows = Vec::new();

    // Fredorch login start
    let login = vec![0x55, 0x03, 0x99, 0x9C, 0xAA];
    rows.push(
        probe_frame(
            &session,
            &mut notifications,
            channel,
            "fredorch_login",
            &login,
        )
        .await?,
    );

    // Pattern commands 0x00-0x13
    for pat in 0x00..=0x05u8 {
        let inner = vec![0x16, pat];
        let frame = fredorch_frame(&inner);
        rows.push(
            probe_frame(
                &session,
                &mut notifications,
                channel,
                &format!("fredorch_pat_{pat:02X}"),
                &frame,
            )
            .await?,
        );
    }

    session.peripheral.disconnect().await?;
    Ok(rows)
}

fn fredorch_frame(data: &[u8]) -> Vec<u8> {
    let len = data.len() as u8 + 2;
    let mut frame = vec![0x55, len];
    frame.extend_from_slice(data);
    let sum: u16 = frame[1..].iter().map(|&b| b as u16).sum();
    frame.push((sum & 0xFF) as u8);
    frame.push(0xAA);
    frame
}

async fn probe_frame(
    session: &Session,
    notifications: &mut (impl StreamExt<Item = btleplug::api::ValueNotification> + Unpin),
    channel: &ChannelPair,
    label: &str,
    frame: &[u8],
) -> Result<ProbeRow> {
    let response = send_and_wait(session, notifications, frame).await?;
    Ok(ProbeRow {
        label: label.into(),
        channel: channel.label.into(),
        sent: spaced_hex(frame),
        response: response
            .as_ref()
            .map(|r| spaced_hex(r))
            .unwrap_or_else(|| "(no response)".into()),
        class: classify_response(frame, &response).into(),
    })
}

fn format_results(device: &str, rows: &[ProbeRow]) -> String {
    let mut out = format!("# BLE Probe Results\n\n- Device: `{device}`\n\n");
    out.push_str("| label | channel | sent | response | class |\n");
    out.push_str("| ----- | ------- | ---- | -------- | ----- |\n");
    for r in rows {
        out.push_str(&format!(
            "| {} | {} | `{}` | `{}` | {} |\n",
            r.label, r.channel, r.sent, r.response, r.class
        ));
    }
    out
}

fn print_results(rows: &[ProbeRow]) {
    let interesting: Vec<_> = rows
        .iter()
        .filter(|r| r.class == "non-standard" || r.class == "standard ack")
        .collect();
    println!("\n=== Interesting responses ({}) ===", interesting.len());
    for r in interesting {
        println!(
            "  [{}] {} | {} -> {} ({})",
            r.channel, r.label, r.sent, r.response, r.class
        );
    }
}
