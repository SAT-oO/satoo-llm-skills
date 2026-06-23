//! Interactive human verification — mandatory gate before FINDINGS.md.
//!
//!   cargo run --bin ble_verify -- --device UUID --plan verify_plan.json
//!
//! At each checkpoint the user watches the device and chooses:
//!   y = success (correct movement) → next
//!   n = error (wrong/no movement)  → next, marked failed
//!   r = replay this checkpoint
//!   q = quit early, save results so far

use anyhow::{Context, Result};
use ble_hack_skill::session::{
    adapter, connect, send_and_wait, send_burst, send_handshake, spaced_hex, ChannelPair,
};
use btleplug::api::{bleuuid::uuid_from_u16, Peripheral};
use serde::Deserialize;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;
use tokio::time;

const DEFAULT_CHANNEL: ChannelPair = ChannelPair {
    label: "FFE1/FFE2",
    rx: uuid_from_u16(0xFFE1),
    tx: uuid_from_u16(0xFFE2),
};

const SVAKOM_HANDSHAKE: [&[u8]; 3] = [
    &[0x55, 0x04, 0x00, 0x00, 0x01, 0xFF, 0xAA],
    &[0x55, 0x04, 0x00, 0x00, 0x00, 0x00, 0xAA],
    &[0x55, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00],
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Success,
    Error,
    Skipped,
}

#[derive(Debug, Deserialize)]
struct Plan {
    #[serde(default)]
    handshake: bool,
    #[serde(default = "default_sustain_ms")]
    sustain_ms: u64,
    #[serde(default)]
    channel: Option<String>,
    checkpoints: Vec<Checkpoint>,
}

#[derive(Debug, Deserialize)]
struct Checkpoint {
    id: String,
    label: String,
    expect: String,
    #[serde(default)]
    burst_hex: String,
    #[serde(default = "default_burst_secs")]
    burst_seconds: u64,
    #[serde(default)]
    stop_hex: Option<String>,
    /// Query/read commands: single send, no sustain burst.
    #[serde(default)]
    one_shot: bool,
}

fn default_sustain_ms() -> u64 {
    50
}

fn default_burst_secs() -> u64 {
    3
}

struct ResultRow {
    id: String,
    label: String,
    sent: String,
    expect: String,
    verdict: Verdict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PromptChoice {
    Success,
    Error,
    Replay,
    Quit,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let device = arg_value(&args, "--device").context("--device UUID required")?;
    let plan_path = arg_value(&args, "--plan").context("--plan verify_plan.json required")?;
    let output = arg_value(&args, "--output").unwrap_or_else(|| "verify_results.md".into());

    let plan: Plan = serde_json::from_str(&fs::read_to_string(&plan_path)?)
        .with_context(|| format!("parse plan: {plan_path}"))?;

    if plan.checkpoints.is_empty() {
        anyhow::bail!("plan has no checkpoints");
    }

    let channel = resolve_channel(plan.channel.as_deref());
    let adpt = adapter().await?;
    let session = connect(&adpt, &device, &channel).await?;
    let mut notifications = session.peripheral.notifications().await?;

    println!("Connected. Human verification — watch the device.\n");
    if plan.handshake {
        send_handshake(&session, &mut notifications, &SVAKOM_HANDSHAKE).await?;
        println!("Handshake sent.\n");
    }

    let sustain = Duration::from_millis(plan.sustain_ms);
    let mut results = Vec::new();
    let mut idx = 0usize;

    while idx < plan.checkpoints.len() {
        let cp = &plan.checkpoints[idx];
        println!("══════════════════════════════════════════════════");
        println!(
            "Checkpoint {}/{}: {}",
            idx + 1,
            plan.checkpoints.len(),
            cp.label
        );
        println!("ID: {}", cp.id);
        println!("Expect: {}", cp.expect);

        if cp.burst_hex.is_empty() {
            println!("(no burst_hex — skipping send)");
            results.push(ResultRow {
                id: cp.id.clone(),
                label: cp.label.clone(),
                sent: String::new(),
                expect: cp.expect.clone(),
                verdict: Verdict::Skipped,
            });
            idx += 1;
            continue;
        }

        let frame = parse_hex(&cp.burst_hex)?;
        println!("Send: `{}`", spaced_hex(&frame));

        let sent_label = if cp.one_shot {
            send_and_wait(&session, &mut notifications, &frame).await?;
            time::sleep(Duration::from_millis(300)).await;
            spaced_hex(&frame)
        } else {
            send_burst(
                &session,
                &mut notifications,
                &[frame.clone()],
                Duration::from_secs(cp.burst_seconds),
            )
            .await?;
            format!("{} ({}s burst)", spaced_hex(&frame), cp.burst_seconds)
        };

        if let Some(stop) = &cp.stop_hex {
            time::sleep(sustain).await;
            let stop_frame = parse_hex(stop)?;
            send_and_wait(&session, &mut notifications, &stop_frame).await?;
            println!("Stop: `{}`", spaced_hex(&stop_frame));
            time::sleep(Duration::from_millis(500)).await;
        }

        println!();
        match prompt_user()? {
            PromptChoice::Success => {
                println!("  → marked SUCCESS\n");
                results.push(ResultRow {
                    id: cp.id.clone(),
                    label: cp.label.clone(),
                    sent: sent_label,
                    expect: cp.expect.clone(),
                    verdict: Verdict::Success,
                });
                idx += 1;
            }
            PromptChoice::Error => {
                println!("  → marked ERROR (wrong protocol)\n");
                results.push(ResultRow {
                    id: cp.id.clone(),
                    label: cp.label.clone(),
                    sent: sent_label,
                    expect: cp.expect.clone(),
                    verdict: Verdict::Error,
                });
                idx += 1;
            }
            PromptChoice::Replay => {
                println!("  → replaying checkpoint\n");
            }
            PromptChoice::Quit => {
                println!("  → quit early\n");
                results.push(ResultRow {
                    id: cp.id.clone(),
                    label: cp.label.clone(),
                    sent: sent_label,
                    expect: cp.expect.clone(),
                    verdict: Verdict::Skipped,
                });
                break;
            }
        }
    }

    session.peripheral.disconnect().await?;

    let md = format_results(&device, &plan_path, &results);
    fs::write(&output, &md)?;
    print_summary(&results);
    println!("\nWrote {output}");
    println!("Only SUCCESS rows may be copied into FINDINGS.md.");

    Ok(())
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

fn resolve_channel(name: Option<&str>) -> ChannelPair {
    match name {
        Some("ae01") | Some("AE01") => ChannelPair {
            label: "AE01/AE02",
            rx: uuid_from_u16(0xAE01),
            tx: uuid_from_u16(0xAE02),
        },
        _ => DEFAULT_CHANNEL,
    }
}

fn parse_hex(s: &str) -> Result<Vec<u8>> {
    s.split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).context(format!("bad hex: {b}")))
        .collect()
}

fn prompt_user() -> Result<PromptChoice> {
    print!(
        "Did the device match \"Expect\"?\n\
         \n\
           [y] yes — success, next checkpoint\n\
           [n] no  — wrong protocol, next checkpoint\n\
           [r] replay this checkpoint\n\
           [q] quit and save\n\
         \n\
         Choice: "
    );
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    match line.trim().to_lowercase().as_str() {
        "y" | "yes" | "s" | "success" => Ok(PromptChoice::Success),
        "n" | "no" | "e" | "error" | "fail" => Ok(PromptChoice::Error),
        "r" | "replay" => Ok(PromptChoice::Replay),
        "q" | "quit" | "exit" => Ok(PromptChoice::Quit),
        other => {
            eprintln!("Unknown choice '{other}', treating as replay.");
            Ok(PromptChoice::Replay)
        }
    }
}

fn format_results(device: &str, plan: &str, rows: &[ResultRow]) -> String {
    let mut out = format!("# Human Verification Results\n\n");
    out.push_str(&format!("- Device: `{device}`\n"));
    out.push_str(&format!("- Plan: `{}`\n\n", Path::new(plan).file_name().unwrap().to_string_lossy()));
    out.push_str("| id | label | sent | expect | verdict |\n");
    out.push_str("| --- | --- | --- | --- | --- |\n");
    for r in rows {
        let v = match r.verdict {
            Verdict::Success => "**success**",
            Verdict::Error => "error",
            Verdict::Skipped => "skipped",
        };
        out.push_str(&format!(
            "| {} | {} | `{}` | {} | {} |\n",
            r.id, r.label, r.sent, r.expect, v
        ));
    }
    out.push_str("\n## FINDINGS.md gate\n\n");
    out.push_str("Copy only **success** rows into FINDINGS.md → Verified commands.\n");
    out
}

fn print_summary(rows: &[ResultRow]) {
    let ok = rows.iter().filter(|r| r.verdict == Verdict::Success).count();
    let bad = rows.iter().filter(|r| r.verdict == Verdict::Error).count();
    let skip = rows.iter().filter(|r| r.verdict == Verdict::Skipped).count();
    println!("\n=== Summary ===");
    println!("  success: {ok}");
    println!("  error:   {bad}");
    println!("  skipped: {skip}");
}
