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
    adapter, classify_response, connect, discover_channels_on_device, listen_notifications,
    read_readable_chars, send_and_wait, send_burst, send_handshake, spaced_hex, ChannelPair, Session,
};
use btleplug::api::{bleuuid::uuid_from_u16, Peripheral};
use futures::StreamExt;
use std::fs;
use std::time::Duration;
use uuid::Uuid;

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
    let preflight_only = args.iter().any(|a| a == "--preflight");
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
        match run_header_sweep(device, channel, false).await {
            Ok(mut r) => rows.append(&mut r),
            Err(e) => eprintln!("  {} header sweep failed: {e}", channel.label),
        }
    }

    let best = rows
        .iter()
        .filter(|r| r.channel.contains("FFE1") && (r.class == "echo" || r.class == "non-standard"))
        .max_by_key(|r| score_row(r))
        .or_else(|| {
            rows.iter()
                .filter(|r| r.class == "non-standard" || r.class == "standard ack" || r.class == "echo")
                .max_by_key(|r| score_row(r))
        })
        .map(|r| r.channel.clone());

    let target_channel = channels
        .iter()
        .find(|c| c.label.contains("FFE1"))
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
    println!("\n=== Phase 2–4: single-session motor work on {} ===\n", channel.label);

    let session = connect(&adpt, device, &channel).await?;
    let mut notifications = session.peripheral.notifications().await?;

    rows.extend(run_opcode_sweep_session(&session, &mut notifications, &channel, 0x55, false).await?);
    rows.extend(run_opcode_sweep_session(&session, &mut notifications, &channel, 0x55, true).await?);

    println!("\n=== Phase 4: tail-family probes on hot opcodes ===\n");
    rows.extend(run_tail_family_probe(&session, &mut notifications, &channel).await?);

    session.peripheral.disconnect().await?;

    println!("\n=== Phase 5: AE01 Fredorch-style pattern sweep ===\n");
    rows.extend(run_fredorch_sweep(device).await.unwrap_or_default());

    Ok(rows)
}

async fn run_tail_family_probe(
    session: &Session,
    notifications: &mut (impl StreamExt<Item = btleplug::api::ValueNotification> + Unpin),
    channel: &ChannelPair,
) -> Result<Vec<ProbeRow>> {
    let mut rows = Vec::new();
    let samples: [(&str, [u8; 7]); 6] = [
        (
            "tail_crc_query_02",
            frame_with_crc([0x55, 0x02, 0x00, 0x00, 0x00, 0x00]),
        ),
        (
            "tail_crc_query_A0",
            frame_with_crc([0x55, 0xA0, 0x00, 0x00, 0x00, 0x00]),
        ),
        (
            "tail_aa_boost_40",
            frame_with_aa([0x55, 0x04, 0x00, 0x00, 0x00, 0x40]),
        ),
        (
            "tail_crc_stretch",
            frame_with_crc([0x55, 0x08, 0x00, 0x00, 0x01, 0x01]),
        ),
        (
            "tail_crc_mmode",
            frame_with_crc([0x55, 0x08, 0x00, 0x03, 0x01, 0x05]),
        ),
        (
            "tail_crc_stop",
            frame_with_crc([0x55, 0x08, 0x00, 0x01, 0x00, 0x00]),
        ),
    ];

    for (label, frame) in samples {
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
    handshake: bool,
) -> Result<Vec<ProbeRow>> {
    if handshake {
        send_handshake(session, notifications, &SVAKOM_HANDSHAKE).await?;
    }
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

        for (_, data) in listen_notifications(&session, &mut notifications, Duration::from_secs(2)).await
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
            println!(
                "  {} read {}: {}",
                channel.label,
                uuid,
                spaced_hex(&data)
            );
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

async fn run_fredorch_sweep(device: &str) -> Result<Vec<ProbeRow>> {
    let channel = ChannelPair {
        label: "AE01/AE02".into(),
        rx: uuid_from_u16(0xAE01),
        tx: uuid_from_u16(0xAE02),
    };
    let adpt = adapter().await?;
    let session = connect(&adpt, device, &channel).await?;
    let mut notifications = session.peripheral.notifications().await?;
    let mut rows = Vec::new();

    // Fredorch login start
    let login = vec![0x55, 0x03, 0x99, 0x9C, 0xAA];
    rows.push(
        probe_frame(
            &session,
            &mut notifications,
            &channel,
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
                &channel,
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
