//! Automated BLE protocol discovery: header probes, opcode sweeps, burst tests.
//!
//! Usage:
//!   cargo run --bin ble_probe -- --device UUID --auto --output test_results.md
//!   cargo run --bin ble_probe -- --device UUID --channel ffe1 --header-sweep
//!   cargo run --bin ble_probe -- --device UUID --channel ffe1 --opcode-sweep --header 55
//!   cargo run --bin ble_probe -- --device UUID --channel ffe1 --burst "55 03 00 00 01 01 00" --seconds 3

use anyhow::{Context, Result};
use ble_hack_skill::crc::{frame_with_aa, frame_with_crc};
use ble_hack_skill::gatt;
use ble_hack_skill::session::{
    ChannelPair, Session, adapter, classify_response, connect, discover_channels_on_device,
    listen_notifications, read_readable_chars, send_and_wait, send_burst, spaced_hex,
};
use btleplug::api::{Peripheral, bleuuid::uuid_from_u16};
use futures::StreamExt;
use std::fs;
use std::time::Duration;
use uuid::Uuid;

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
    let preflight_only = args.iter().any(|a| a == "--preflight");
    let header_sweep = args.iter().any(|a| a == "--header-sweep");
    let opcode_sweep = args.iter().any(|a| a == "--opcode-sweep");
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
    } else if preflight_only {
        let adpt = adapter().await?;
        let channels = discover_channels_on_device(&adpt, &device).await?;
        rows.extend(run_gatt_preflight(&device, &channels).await?);
    } else if let Some(hex_str) = burst_hex {
        let channel = channel_filter.context("--channel required for --burst")?;
        let payload = parse_hex(&hex_str)?;
        let adpt = adapter().await?;
        let session = connect(&adpt, &device, &channel).await?;
        let mut notifications = session.peripheral.notifications().await?;
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
            channel: channel.label.clone(),
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
            let adpt = adapter().await?;
            discover_channels_on_device(&adpt, &device).await?
        };
        for channel in channels {
            if header_sweep {
                rows.extend(run_header_sweep(&device, &channel).await?);
            }
            if opcode_sweep {
                rows.extend(run_opcode_sweep(&device, &channel, header_byte).await?);
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
    gatt::static_channels()
        .into_iter()
        .find(|c| {
            c.label.to_lowercase().contains(&s)
                || c.rx.to_string().contains(&s)
                || format!("{:04x}", c.rx.as_u128() as u16).contains(&s)
        })
        .unwrap_or_else(|| {
            let uuid = Uuid::parse_str(&s).ok();
            if let Some(rx) = uuid {
                ChannelPair {
                    label: "custom".into(),
                    rx,
                    tx: uuid_from_u16(0xFFE2),
                }
            } else {
                gatt::static_channels()[0].clone()
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
    let channels = discover_channels_on_device(&adpt, device).await?;
    if channels.is_empty() {
        anyhow::bail!("No write/notify channels found on device");
    }
    println!(
        "Discovered {} channel(s): {}\n",
        channels.len(),
        channels
            .iter()
            .map(|c| c.label.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );

    println!("=== Phase 0: GATT preflight (idle + reads) ===\n");
    rows.extend(run_gatt_preflight(device, &channels).await?);

    println!("=== Phase 1: header probes on all channels ===\n");
    for channel in &channels {
        match run_header_sweep(device, channel).await {
            Ok(mut r) => rows.append(&mut r),
            Err(e) => eprintln!("  {} header sweep failed: {e}", channel.label),
        }
    }

    let best = rows
        .iter()
        .filter(|r| {
            (r.channel.contains("FFE1") || r.channel.contains("AE01"))
                && (r.class == "echo" || r.class == "non-standard")
        })
        .max_by_key(|r| score_row(r))
        .or_else(|| {
            rows.iter()
                .filter(|r| {
                    r.class == "non-standard" || r.class == "standard ack" || r.class == "echo"
                })
                .max_by_key(|r| score_row(r))
        })
        .map(|r| r.channel.clone());

    let target_channel = channels
        .iter()
        .find(|c| c.label.contains("FFE1") || c.label.contains("AE01"))
        .cloned()
        .or_else(|| {
            best.as_deref().and_then(|label| {
                channels
                    .iter()
                    .find(|c| label.contains(c.label.as_str()) || label.contains(&c.label))
                    .cloned()
            })
        });

    let channel = target_channel.unwrap_or_else(|| channels[0].clone());
    let header = best_header_from_rows(&rows);
    println!("\n=== Phase 2–4: motor probes on {} (header 0x{:02X}) ===\n", channel.label, header);

    let session = connect(&adpt, device, &channel).await?;
    let mut notifications = session.peripheral.notifications().await?;

    rows.extend(
        run_opcode_sweep_session(&session, &mut notifications, &channel, header).await?,
    );

    println!("\n=== Phase 4: tail-family probes ===\n");
    rows.extend(run_tail_family_probe(&session, &mut notifications, &channel, header).await?);

    session.peripheral.disconnect().await?;

    Ok(rows)
}

async fn run_tail_family_probe(
    session: &Session,
    notifications: &mut (impl StreamExt<Item = btleplug::api::ValueNotification> + Unpin),
    channel: &ChannelPair,
    header: u8,
) -> Result<Vec<ProbeRow>> {
    let mut rows = Vec::new();
    let samples: [(&str, [u8; 6]); 6] = [
        ("tail_crc_query_02", [header, 0x02, 0x00, 0x00, 0x00, 0x00]),
        ("tail_crc_query_A0", [header, 0xA0, 0x00, 0x00, 0x00, 0x00]),
        ("tail_aa_boost_40", [header, 0x04, 0x00, 0x00, 0x00, 0x40]),
        (
            "tail_crc_stretch",
            [header, 0x08, 0x00, 0x00, 0x01, 0x01],
        ),
        (
            "tail_crc_mmode",
            [header, 0x08, 0x00, 0x03, 0x01, 0x05],
        ),
        (
            "tail_crc_stop",
            [header, 0x08, 0x00, 0x01, 0x00, 0x00],
        ),
    ];

    for (label, body) in samples {
        let frame = if label.contains("aa") {
            frame_with_aa(body)
        } else {
            frame_with_crc(body)
        };
        let row = probe_frame(session, notifications, channel, label, &frame).await?;
        if row.class != "silent" {
            println!(
                "  {}: {} -> {} [{}]",
                label, row.sent, row.response, row.class
            );
        }
        rows.push(row);
    }
    Ok(rows)
}

async fn run_opcode_sweep_session(
    session: &Session,
    notifications: &mut (impl StreamExt<Item = btleplug::api::ValueNotification> + Unpin),
    channel: &ChannelPair,
    header: u8,
) -> Result<Vec<ProbeRow>> {
    let mut rows = Vec::new();
    for opcode in 0x00..=0x20u8 {
        let frame = vec![header, opcode, 0x00, 0x00, 0x01, 0x01, 0x00];
        let row = probe_frame(
            session,
            notifications,
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
    Ok(rows)
}

async fn run_gatt_preflight(device: &str, channels: &[ChannelPair]) -> Result<Vec<ProbeRow>> {
    let mut rows = Vec::new();
    let adpt = adapter().await?;

    for channel in channels {
        let session = match connect(&adpt, device, channel).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("  {} preflight connect failed: {e}", channel.label);
                continue;
            }
        };
        let mut notifications = session.peripheral.notifications().await?;

        for (_, data) in
            listen_notifications(&session, &mut notifications, Duration::from_secs(2)).await
        {
            rows.push(ProbeRow {
                label: "idle_notify".into(),
                channel: channel.label.clone(),
                sent: "(listen)".into(),
                response: spaced_hex(&data),
                class: "non-standard".into(),
            });
        }

        for (uuid, data) in read_readable_chars(&session).await? {
            println!("  {} read {}: {}", channel.label, uuid, spaced_hex(&data));
            rows.push(ProbeRow {
                label: format!("read_{}", uuid.to_string().split('-').next().unwrap_or("?")),
                channel: channel.label.clone(),
                sent: "(read)".into(),
                response: spaced_hex(&data),
                class: "non-standard".into(),
            });
        }

        session.peripheral.disconnect().await?;
    }

    Ok(rows)
}

fn score_row(r: &ProbeRow) -> i32 {
    let mut score = match r.class.as_str() {
        "echo" => 80,
        "non-standard" => 100,
        "standard ack" => 50,
        _ => 0,
    };
    if r.channel.contains("FFE1") {
        score += 50;
    }
    if r.class != "silent" && !r.sent.starts_with('(') {
        score += 20;
    }
    score
}

fn best_header_from_rows(rows: &[ProbeRow]) -> u8 {
    rows.iter()
        .filter(|r| r.label.starts_with("probeA_H=") && r.class != "silent")
        .max_by_key(|r| score_row(r))
        .and_then(|r| {
            r.label
                .strip_prefix("probeA_H=")
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        })
        .unwrap_or(0x55)
}

async fn run_header_sweep(device: &str, channel: &ChannelPair) -> Result<Vec<ProbeRow>> {
    let adpt = adapter().await?;
    let session = connect(&adpt, device, channel).await?;
    let mut notifications = session.peripheral.notifications().await?;

    let headers = [0x00, 0x55, 0xAA, 0xA5, 0x5A, 0xFF];
    let mut rows = Vec::new();

    for h in headers {
        // Probe A: [H] 00 00 00 00 00 00
        let a = vec![h, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        rows.push(
            probe_frame(
                &session,
                &mut notifications,
                channel,
                &format!("probeA_H={h:02X}"),
                &a,
            )
            .await?,
        );

        // Probe B: [H] 01 00 00 00 00 00
        let b = vec![h, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00];
        rows.push(
            probe_frame(
                &session,
                &mut notifications,
                channel,
                &format!("probeB_H={h:02X}"),
                &b,
            )
            .await?,
        );

        // Probe C: 03 [H] 00 00
        let c = vec![0x03, h, 0x00, 0x00];
        rows.push(
            probe_frame(
                &session,
                &mut notifications,
                channel,
                &format!("probeC_H={h:02X}"),
                &c,
            )
            .await?,
        );
    }

    session.peripheral.disconnect().await?;
    Ok(rows)
}

async fn run_opcode_sweep(
    device: &str,
    channel: &ChannelPair,
    header: u8,
) -> Result<Vec<ProbeRow>> {
    let adpt = adapter().await?;
    let session = connect(&adpt, device, channel).await?;
    let mut notifications = session.peripheral.notifications().await?;

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
        channel: channel.label.clone(),
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
