//! Single-connection parameter grid sweep — stays connected, reads status between probes.
//!
//!   cargo run --bin ble_sweep -- --device UUID [--handshake]

use anyhow::{Context, Result};
use ble_hack_skill::crc::{frame_with_aa, frame_with_crc};
use ble_hack_skill::session::{
    adapter, connect, send_and_wait, send_burst, send_handshake, spaced_hex, ChannelPair,
};
use btleplug::api::{bleuuid::uuid_from_u16, Peripheral};
use futures::StreamExt;
use std::fs;
use std::time::Duration;

const CHANNEL: ChannelPair = ChannelPair {
    label: "FFE1/FFE2",
    rx: uuid_from_u16(0xFFE1),
    tx: uuid_from_u16(0xFFE2),
};

const SVAKOM_HANDSHAKE: [&[u8]; 3] = [
    &[0x55, 0x04, 0x00, 0x00, 0x01, 0xFF, 0xAA],
    &[0x55, 0x04, 0x00, 0x00, 0x00, 0x00, 0xAA],
    &[0x55, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00],
];

const STATUS: [u8; 7] = [0x55, 0x02, 0x00, 0x00, 0x00, 0x00, 0xFC];

struct Row {
    label: String,
    sent: String,
    response: String,
    status_before: String,
    status_after: String,
    status_delta: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let device = arg_value(&args, "--device").context("--device required")?;
    let with_handshake = args.iter().any(|a| a == "--handshake");
    let output = arg_value(&args, "--output").unwrap_or_else(|| "sweep_results.md".into());

    let adpt = adapter().await?;
    let session = connect(&adpt, &device, &CHANNEL).await?;
    let mut notifications = session.peripheral.notifications().await?;

    if with_handshake {
        send_handshake(&session, &mut notifications, &SVAKOM_HANDSHAKE).await?;
        println!("Handshake sent.\n");
    }

    let mut rows = Vec::new();

    // Phase 1: battery query baseline
    let resp = send_and_wait(&session, &mut notifications, &STATUS).await?;
    println!(
        "battery: {} -> {}",
        spaced_hex(&STATUS),
        resp.as_ref().map(|r| spaced_hex(r)).unwrap_or_default()
    );

    // Phase 2: Boost scale sweep (0x04 + AA)
    for scale in [0x00u8, 0x20, 0x40, 0x80, 0xCC, 0xFF] {
        let frame = frame_with_aa([0x55, 0x04, 0x00, 0x00, 0x00, scale]).to_vec();
        let status_before = read_status(&session, &mut notifications).await?;
        let response = send_and_wait(&session, &mut notifications, &frame).await?;
        let status_after = read_status(&session, &mut notifications).await?;
        let resp_str = response
            .as_ref()
            .map(|r| spaced_hex(r))
            .unwrap_or_else(|| "(silent)".into());
        rows.push(Row {
            label: format!("boost_{scale:02X}"),
            sent: spaced_hex(&frame),
            response: resp_str,
            status_before: status_before.clone(),
            status_after: status_after.clone(),
            status_delta: status_before != status_after,
        });
    }

    // Phase 3: 0x08 CRC families — Direct Stretch (p1=0x00) and M-mode (p1=0x03)
    for p1 in [0x00u8, 0x03] {
        for mode in 1u8..=5 {
            for travel in 1u8..=5 {
                let frame = frame_with_crc([0x55, 0x08, 0x00, p1, mode, travel]).to_vec();
                let status_before = read_status(&session, &mut notifications).await?;
                let response = send_and_wait(&session, &mut notifications, &frame).await?;
                let status_after = read_status(&session, &mut notifications).await?;
                let resp_str = response
                    .as_ref()
                    .map(|r| spaced_hex(r))
                    .unwrap_or_else(|| "(silent)".into());
                let is_nack = resp_str.contains("55 FF");
                if !is_nack && resp_str != "(silent)" {
                    println!("HIT p1={p1:02X} m={mode} t={travel}: {resp_str}");
                }
                rows.push(Row {
                    label: format!("crc_p1_{p1:02X}_m{mode}_t{travel}"),
                    sent: spaced_hex(&frame),
                    response: resp_str,
                    status_before: status_before.clone(),
                    status_after: status_after.clone(),
                    status_delta: status_before != status_after,
                });
            }
        }
    }

    // Phase 4: legacy 7-byte zero-tail grid (Klitty-style)
    let opcodes = [0x03u8, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10];
    for opcode in opcodes {
        for mode in 1u8..=5 {
            for intensity in 1u8..=5 {
                let frame = vec![0x55, opcode, 0x00, 0x00, mode, intensity, 0x00];
                let status_before = read_status(&session, &mut notifications).await?;
                let response = send_and_wait(&session, &mut notifications, &frame).await?;
                let status_after = read_status(&session, &mut notifications).await?;
                let delta = status_before != status_after;
                let resp_str = response
                    .as_ref()
                    .map(|r| spaced_hex(r))
                    .unwrap_or_else(|| "(silent)".into());
                let is_echo = response.as_ref().is_some_and(|r| r.as_slice() == frame.as_slice());
                let is_nack = resp_str.contains("55 FF 01");
                if delta || (!is_nack && !is_echo && resp_str != "(silent)") {
                    println!(
                        "HIT op={opcode:02X} m={mode} i={intensity}: {resp_str} | status {} -> {}{}",
                        status_before,
                        status_after,
                        if delta { " DELTA" } else { "" }
                    );
                }
                rows.push(Row {
                    label: format!("op_{opcode:02X}_m{mode}_i{intensity}"),
                    sent: spaced_hex(&frame),
                    response: resp_str,
                    status_before,
                    status_after,
                    status_delta: delta,
                });
            }
        }
    }

    // Phase 3: 8-byte AA-suffix frames (Svakom init style) as motor attempts
    for opcode in 0x03u8..=0x10 {
        for p4 in [0x01u8, 0x05] {
            for p5 in [0x01u8, 0x05, 0xFF] {
                let frame = vec![0x55, opcode, 0x00, 0x00, p4, p5, 0xAA];
                let status_before = read_status(&session, &mut notifications).await?;
                let response = send_and_wait(&session, &mut notifications, &frame).await?;
                let status_after = read_status(&session, &mut notifications).await?;
                let resp_str = response
                    .as_ref()
                    .map(|r| spaced_hex(r))
                    .unwrap_or_else(|| "(silent)".into());
                if status_before != status_after || !resp_str.contains("55 FF 01") {
                    println!(
                        "AA-frame op={opcode:02X} p4={p4:02X} p5={p5:02X}: {resp_str}"
                    );
                }
                let delta = status_before != status_after;
                rows.push(Row {
                    label: format!("aa_op_{opcode:02X}_{p4:02X}_{p5:02X}"),
                    sent: spaced_hex(&frame),
                    response: resp_str,
                    status_before: status_before.clone(),
                    status_after: status_after.clone(),
                    status_delta: delta,
                });
            }
        }
    }

    // Phase 4: burst best candidates (non-NACK, non-echo, or status delta)
    let candidates: Vec<_> = rows
        .iter()
        .filter(|r| {
            r.status_delta
                || (!r.response.contains("55 FF 01")
                    && !r.response.contains("(silent)")
                    && r.response != r.sent)
        })
        .take(10)
        .collect();

    println!("\n=== Burst candidates ({}) ===", candidates.len());
    for c in candidates {
        let bytes: Vec<u8> = c
            .sent
            .split_whitespace()
            .map(|b| u8::from_str_radix(b, 16).unwrap())
            .collect();
        println!("Bursting {} for 3s...", c.label);
        let _ = send_burst(
            &session,
            &mut notifications,
            &[bytes],
            Duration::from_secs(3),
        )
        .await?;
        let status = read_status(&session, &mut notifications).await?;
        println!("  status after burst: {status}");
    }

    session.peripheral.disconnect().await?;

    let md = format_sweep(&device, &rows);
    fs::write(&output, md)?;
    println!("\nWrote {output}");
    Ok(())
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

async fn read_status(
    session: &ble_hack_skill::session::Session,
    notifications: &mut (impl StreamExt<Item = btleplug::api::ValueNotification> + Unpin),
) -> Result<String> {
    let resp = send_and_wait(session, notifications, &STATUS).await?;
    Ok(resp
        .as_ref()
        .map(|r| spaced_hex(r))
        .unwrap_or_else(|| "(none)".into()))
}

fn format_sweep(device: &str, rows: &[Row]) -> String {
    let mut out = format!("# BLE Sweep Results\n\n- Device: `{device}`\n\n");
    out.push_str("| label | sent | response | status_before | status_after | delta |\n");
    out.push_str("| ----- | ---- | -------- | ------------- | ------------ | ----- |\n");
    for r in rows {
        out.push_str(&format!(
            "| {} | `{}` | `{}` | `{}` | `{}` | {} |\n",
            r.label,
            r.sent,
            r.response,
            r.status_before,
            r.status_after,
            r.status_delta
        ));
    }
    out
}
