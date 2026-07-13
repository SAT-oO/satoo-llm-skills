//! Interactive human verification — mandatory gate before FINDINGS.md.
//!
//!   cargo run -p ble-hack-skill --bin ble_verify
//!   cargo run -p ble-hack-skill --bin ble_verify -- --workdir .
//!   cargo run -p ble-hack-skill --bin ble_verify -- --plan verify_plan_m_modes.json
//!   cargo run -p ble-hack-skill --bin ble_verify -- --from suction_lvl4
//!
//! Device UUID is read from `ble_session.json` or `scan_results.md` in the workdir
//! unless `--device` is passed.
//!   y = success (correct movement) → next
//!   n = error (wrong/no movement)  → next, marked failed
//!   r = replay this checkpoint
//!   q = quit early, save results so far

use anyhow::{Context, Result};
use ble_hack_skill::discover;
use ble_hack_skill::session::{
    ChannelPair, adapter, connect, send_and_wait, send_burst, spaced_hex,
};
use ble_hack_skill::verify;
use ble_hack_skill::workdir;
use btleplug::api::{CharPropFlags, Peripheral, WriteType, bleuuid::uuid_from_u16};
use serde::Deserialize;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;
use tokio::time;

fn default_channel() -> ChannelPair {
    ChannelPair {
        label: "FFE1/FFE2".into(),
        rx: uuid_from_u16(0xFFE1),
        tx: uuid_from_u16(0xFFE2),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Success,
    Error,
    Skipped,
}

#[derive(Debug, Deserialize)]
struct Plan {
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
    prime_hex: Option<String>,
    #[serde(default)]
    prime_seconds: Option<u64>,
    #[serde(default)]
    stop_hex: Option<String>,
    /// Semicolon-separated frames; alternated for `stop_burst_seconds` (FINDINGS stop sequence).
    #[serde(default)]
    stop_burst_hex: Option<String>,
    #[serde(default)]
    stop_burst_seconds: Option<u64>,
    /// Query/read commands: single send, no sustain burst.
    #[serde(default)]
    one_shot: bool,
    /// Override plan-level channel for this checkpoint (`ae01`, `ae3b`, …).
    #[serde(default)]
    channel: Option<String>,
    /// Optional explicit write characteristic UUID (overrides channel Rx).
    #[serde(default)]
    write_uuid: Option<String>,
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
    let workdir = workdir::workdir_from_args(&args);
    let (brand, product) = workdir::brand_product_from_args(&workdir, &args);
    let findings_auto = !args.iter().any(|a| a == "--no-findings");
    let device = workdir::resolve_device(&workdir, arg_value(&args, "--device").as_deref())?;
    let plan_path = workdir::resolve_plan_path(&workdir, arg_value(&args, "--plan").as_deref());
    let output = workdir::resolve_output_path(&workdir, arg_value(&args, "--output").as_deref());
    let from_id = arg_value(&args, "--from");

    let plan: Plan = serde_json::from_str(
        &fs::read_to_string(&plan_path)
            .with_context(|| format!("read plan: {}", plan_path.display()))?,
    )
    .with_context(|| format!("parse plan: {}", plan_path.display()))?;

    if plan.checkpoints.is_empty() {
        anyhow::bail!("plan has no checkpoints");
    }

    let channel = resolve_channel(plan.channel.as_deref());
    let adpt = adapter().await?;
    let mut session = connect(&adpt, &device, &channel).await?;
    let mut notifications = session.peripheral.notifications().await?;
    let mut active_channel = channel.label.clone();

    println!("Connected. Human verification — watch the device.\n");

    let sustain = Duration::from_millis(plan.sustain_ms);
    let _ = sustain;
    let mut results = Vec::new();
    let mut idx = 0usize;
    if let Some(id) = &from_id {
        idx = plan
            .checkpoints
            .iter()
            .position(|c| c.id == *id)
            .with_context(|| format!("checkpoint id not in plan: {id}"))?;
        println!(
            "Resuming from checkpoint {id} ({}/{})",
            idx + 1,
            plan.checkpoints.len()
        );
    }

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
            println!("(read-only checkpoint — no BLE send)\n");
            match prompt_user()? {
                PromptChoice::Success => {
                    println!("  → marked SUCCESS\n");
                    results.push(ResultRow {
                        id: cp.id.clone(),
                        label: cp.label.clone(),
                        sent: String::new(),
                        expect: cp.expect.clone(),
                        verdict: Verdict::Success,
                    });
                    idx += 1;
                }
                PromptChoice::Error => {
                    println!("  → marked ERROR\n");
                    results.push(ResultRow {
                        id: cp.id.clone(),
                        label: cp.label.clone(),
                        sent: String::new(),
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
                        sent: String::new(),
                        expect: cp.expect.clone(),
                        verdict: Verdict::Skipped,
                    });
                    break;
                }
            }
            continue;
        }

        let frame = parse_hex(&cp.burst_hex)?;
        let mut sent_label = String::new();

        if let Some(prime) = &cp.prime_hex {
            let prime_frame = parse_hex(prime)?;
            let prime_secs = cp.prime_seconds.unwrap_or(4);
            println!("Prime: `{}` ({}s)", spaced_hex(&prime_frame), prime_secs);
            send_burst_on_session(
                &session,
                &mut notifications,
                &[prime_frame.clone()],
                Duration::from_secs(prime_secs),
                plan.sustain_ms,
            )
            .await?;
            time::sleep(Duration::from_millis(300)).await;
            sent_label = format!(
                "{} ({}s prime) → ",
                spaced_hex(&prime_frame),
                prime_secs
            );
        }

        println!("Send: `{}`", spaced_hex(&frame));

        let cp_channel = cp.channel.as_deref().or_else(|| {
            cp.write_uuid.as_deref().and_then(|w| {
                if w.to_ascii_lowercase().contains("ae3b") {
                    Some("ae3b")
                } else if w.to_ascii_lowercase().contains("ae01") {
                    Some("ae01")
                } else {
                    None
                }
            })
        });
        if let Some(ch) = cp_channel {
            let pair = resolve_channel(Some(ch));
            if pair.label != active_channel {
                session.peripheral.disconnect().await?;
                session = connect(&adpt, &device, &pair).await?;
                notifications = session.peripheral.notifications().await?;
                active_channel = pair.label.clone();
            }
        }

        let burst_label = if cp.one_shot {
            send_on_session(&session, &mut notifications, &frame).await?;
            time::sleep(Duration::from_millis(300)).await;
            spaced_hex(&frame)
        } else {
            send_burst_on_session(
                &session,
                &mut notifications,
                &[frame.clone()],
                Duration::from_secs(cp.burst_seconds),
                plan.sustain_ms,
            )
            .await?;
            format!("{} ({}s burst)", spaced_hex(&frame), cp.burst_seconds)
        };
        sent_label.push_str(&burst_label);

        if let Some(stop_burst) = &cp.stop_burst_hex {
            time::sleep(Duration::from_millis(plan.sustain_ms)).await;
            let frames: Vec<Vec<u8>> = stop_burst
                .split(';')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(parse_hex)
                .collect::<Result<_>>()?;
            let secs = cp.stop_burst_seconds.unwrap_or(2);
            println!(
                "Stop burst: {} frame(s), {}s",
                frames.len(),
                secs
            );
            for f in &frames {
                println!("  `{}`", spaced_hex(f));
            }
            send_app_stop_burst(
                &session,
                &mut notifications,
                &frames,
                Duration::from_secs(secs),
                plan.sustain_ms,
            )
            .await?;
            time::sleep(Duration::from_millis(500)).await;
        } else if let Some(stop) = &cp.stop_hex {
            time::sleep(Duration::from_millis(plan.sustain_ms)).await;
            let stop_frame = parse_hex(stop)?;
            send_on_session(&session, &mut notifications, &stop_frame).await?;
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

    let md = format_results(&device, plan_path.to_str().unwrap(), &results);
    fs::write(&output, &md)?;
    print_summary(&results);
    println!("\nWrote {}", output.display());

    let ok = results
        .iter()
        .filter(|r| r.verdict == Verdict::Success)
        .count();
    if ok > 0 && findings_auto {
        let summary = verify::VerifySummary::from_markdown(&md);
        let sweep_md = fs::read_to_string(workdir.join("sweep_results.md")).ok();
        let findings_path = workdir.join("FINDINGS.md");
        let body = discover::render_findings_for_workdir(
            &brand,
            &product,
            &[summary],
            sweep_md.as_deref(),
            &workdir,
        );
        fs::write(&findings_path, body)?;
        println!("Wrote {} ({} success rows)", findings_path.display(), ok);
    } else if ok > 0 {
        println!("Only SUCCESS rows may be copied into FINDINGS.md.");
        println!(
            "Regenerate: cargo run -p ble-hack-skill --bin ble_check -- --workdir {} --brand \"{}\" --product \"{}\"",
            workdir.display(),
            brand,
            product
        );
    } else {
        println!("No SUCCESS rows — FINDINGS.md not updated.");
    }

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
            label: "AE01/AE02".into(),
            rx: uuid_from_u16(0xAE01),
            tx: uuid_from_u16(0xAE02),
        },
        Some("ae3b") | Some("AE3B") => ChannelPair {
            label: "AE3B/AE3C".into(),
            rx: uuid_from_u16(0xAE3B),
            tx: uuid_from_u16(0xAE3C),
        },
        Some("ae10") | Some("AE10") => ChannelPair {
            label: "AE10/AE02".into(),
            rx: uuid_from_u16(0xAE10),
            tx: uuid_from_u16(0xAE02),
        },
        Some("ae03") | Some("AE03") => ChannelPair {
            label: "AE03/AE05".into(),
            rx: uuid_from_u16(0xAE03),
            tx: uuid_from_u16(0xAE05),
        },
        _ => default_channel(),
    }
}

async fn send_on_session(
    session: &ble_hack_skill::session::Session,
    notifications: &mut (impl futures::StreamExt<Item = btleplug::api::ValueNotification> + Unpin),
    frame: &[u8],
) -> Result<()> {
    send_and_wait(session, notifications, frame).await?;
    Ok(())
}

async fn send_app_stop_burst(
    session: &ble_hack_skill::session::Session,
    notifications: &mut (impl futures::StreamExt<Item = btleplug::api::ValueNotification> + Unpin),
    frames: &[Vec<u8>],
    duration: Duration,
    sustain_ms: u64,
) -> Result<()> {
    let (ffe1, ffe2): (Vec<_>, Vec<_>) = frames
        .iter()
        .cloned()
        .partition(|f| f.len() >= 2 && f[1] == 0xFF);
    if !ffe1.is_empty() {
        send_burst_on_session(session, notifications, &ffe1, duration, sustain_ms).await?;
    }
    for frame in &ffe2 {
        let wt = if session
            .tx_char
            .properties
            .contains(CharPropFlags::WRITE_WITHOUT_RESPONSE)
        {
            WriteType::WithoutResponse
        } else {
            WriteType::WithResponse
        };
        println!("  FFE2 stop: `{}`", spaced_hex(frame));
        match time::timeout(
            Duration::from_secs(3),
            session.peripheral.write(&session.tx_char, frame, wt),
        )
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(e)) => eprintln!("warning: FFE2 stop write failed: {e}"),
            Err(_) => eprintln!("warning: FFE2 stop write timed out (continuing)"),
        }
    }
    Ok(())
}

async fn send_burst_on_session(
    session: &ble_hack_skill::session::Session,
    notifications: &mut (impl futures::StreamExt<Item = btleplug::api::ValueNotification> + Unpin),
    frames: &[Vec<u8>],
    duration: Duration,
    sustain_ms: u64,
) -> Result<()> {
    let _ = sustain_ms;
    send_burst(session, notifications, frames, duration).await?;
    Ok(())
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
    out.push_str(&format!(
        "- Plan: `{}`\n\n",
        Path::new(plan).file_name().unwrap().to_string_lossy()
    ));
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
    let ok = rows
        .iter()
        .filter(|r| r.verdict == Verdict::Success)
        .count();
    let bad = rows.iter().filter(|r| r.verdict == Verdict::Error).count();
    let skip = rows
        .iter()
        .filter(|r| r.verdict == Verdict::Skipped)
        .count();
    println!("\n=== Summary ===");
    println!("  success: {ok}");
    println!("  error:   {bad}");
    println!("  skipped: {skip}");
}
