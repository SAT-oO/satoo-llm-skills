//! One-go BLE hack orchestrator — scan → probe → discover sweep → plan → verify → check.
//!
//! Run from the project root that vendors `ble-hack-skill/`:
//!   cargo run -p ble-hack-skill --bin ble_run -- --brand BRAND --product PRODUCT --workdir .

use anyhow::{bail, Context, Result};
use ble_hack_skill::pipeline::{self, motor_families_in_probe, pick_target, probe_passes_automation_gate};
use ble_hack_skill::workdir as wd;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let brand = arg_value(&args, "--brand").context("--brand required")?;
    let product = arg_value(&args, "--product").context("--product required")?;
    let workdir = arg_value(&args, "--workdir")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let max_iters: u32 = arg_value(&args, "--max-iter")
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);
    let scan_seconds: u64 = arg_value(&args, "--seconds")
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);
    let skip_verify = args.iter().any(|a| a == "--skip-verify");
    let mut device_id: Option<String> = None;
    let mut motor_discovered = false;

    for attempt in 1..=max_iters {
        println!("\n╔══════════════════════════════════════════════════╗");
        println!("║  BLE hack iteration {attempt}/{max_iters}");
        println!("╚══════════════════════════════════════════════════╝\n");

        let adpt = pipeline::adapter().await?;
        let devices = pipeline::scan(
            &adpt,
            scan_seconds,
            Some(brand.as_str()),
            Some(product.as_str()),
        )
        .await?;

        let scan_path = workdir.join("scan_results.md");
        fs::write(
            &scan_path,
            format_scan_md(&devices, Some(brand.as_str()), Some(product.as_str())),
        )?;
        println!("Wrote {}", scan_path.display());

        let target = pick_target(&devices, true).with_context(|| {
            "no device with matching local name — power on target, disconnect official app"
        })?;

        println!(
            "Target: {} ({}) tier={} score={}",
            target.id,
            target.local_name.as_deref().unwrap_or("—"),
            target.tier,
            target.score
        );
        device_id = Some(target.id.clone());
        wd::save_session(&workdir, &target.id, target.local_name.as_deref())?;

        if args.iter().any(|a| a == "--discover") {
            let seconds_s = scan_seconds.to_string();
            let mut scan_args: Vec<&str> = vec![
                "--discover",
                "--seconds",
                &seconds_s,
                "--output",
                scan_path.to_str().unwrap(),
            ];
            let brand_s = brand.clone();
            let product_s = product.clone();
            scan_args.push("--brand");
            scan_args.push(&brand_s);
            scan_args.push("--product");
            scan_args.push(&product_s);
            run_subcommand(&workdir, "ble_scan", &scan_args).await?;
        }

        let probe_path = workdir.join("test_results.md");
        run_subcommand(
            &workdir,
            "ble_probe",
            &[
                "--device",
                &target.id,
                "--auto",
                "--output",
                probe_path.to_str().unwrap(),
            ],
        )
        .await?;

        let probe_md = fs::read_to_string(&probe_path)?;
        motor_discovered = motor_families_in_probe(&probe_md);

        if probe_passes_automation_gate(&probe_md) {
            println!("\n✓ Automation gate passed (FFE1 motor-channel responses).");
            break;
        }

        println!("\n✗ Automation gate failed — no FFE1 boost/stretch echo. Retrying in 5s…");
        if attempt == max_iters {
            bail!(
                "probe did not surface motor candidates after {max_iters} iterations; see {}",
                probe_path.display()
            );
        }
        time::sleep(Duration::from_secs(5)).await;
    }

    let device_id = device_id.context("no device selected")?;
    let workdir_s = workdir.to_str().unwrap();
    let offline_sweep = args.iter().any(|a| a == "--offline-sweep");

    println!("\n=== STEP 3: Discover sweep (probe-expanded grid) ===\n");
    run_discover_sweep(&workdir, &device_id, offline_sweep).await?;

    println!("\n=== STEP 4: Verify plan from sweep hits ===\n");
    run_subcommand(&workdir, "ble_plan", &["--workdir", workdir_s]).await?;

    if skip_verify {
        print_verify_instructions(&workdir, motor_discovered, &brand, &product);
        return Ok(());
    }

    println!("\n═══ STEP 5: Human verification (watch the device) ═══\n");
    let status = run_subcommand_interactive(&workdir, "ble_verify", &["--workdir", workdir_s]).await?;

    if !status.success() {
        bail!("ble_verify exited with {}", status);
    }

    println!("\n═══ STEP 6: Pipeline check + FINDINGS ═══\n");
    let check_args = [
        "--workdir",
        workdir_s,
        "--brand",
        &brand,
        "--product",
        &product,
    ];
    let check_status = Command::new("cargo")
        .args([
            "run",
            "-p",
            "ble-hack-skill",
            "--bin",
            "ble_check",
            "--",
        ])
        .args(&check_args)
        .current_dir(&workdir)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await
        .context("ble_check")?;

    if check_status.success() {
        println!("\n✓ Pipeline complete. Review FINDINGS.md.");
    } else {
        println!("\n✗ ble_check failed — revise artifacts and re-run ble_check.");
        print_verify_instructions(&workdir, motor_discovered, &brand, &product);
    }

    Ok(())
}

async fn run_discover_sweep(workdir: &Path, device_id: &str, force_offline: bool) -> Result<()> {
    let output = "sweep_results.md";
    if force_offline {
        return run_subcommand(
            workdir,
            "ble_sweep",
            &[
                "--offline",
                "--probe",
                "test_results.md",
                "--output",
                output,
            ],
        )
        .await;
    }

    let live = run_subcommand(
        workdir,
        "ble_sweep",
        &[
            "--device",
            device_id,
            "--probe",
            "test_results.md",
            "--output",
            output,
        ],
    )
    .await;

    if live.is_ok() {
        return Ok(());
    }

    eprintln!("\n⚠ Live sweep failed — falling back to --offline synthesis from probe evidence.");
    run_subcommand(
        workdir,
        "ble_sweep",
        &[
            "--offline",
            "--probe",
            "test_results.md",
            "--output",
            output,
        ],
    )
    .await
}

fn print_verify_instructions(workdir: &Path, motor_discovered: bool, brand: &str, product: &str) {
    let wd = workdir.display();
    println!("\n═══ Human verification (run in a real terminal with device powered on) ═══\n");
    println!("  cargo run -p ble-hack-skill --bin ble_verify -- --workdir {wd}");
    println!("\nPipeline completeness check (no BLE):");
    println!(
        "  cargo run -p ble-hack-skill --bin ble_check -- --workdir {wd} --brand \"{brand}\" --product \"{product}\""
    );
    if motor_discovered {
        println!("\nMotor families detected (boost/stretch echo on FFE1).");
    }
    println!("\nAt each checkpoint: y = success, n = wrong, r = replay, q = quit.");
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

async fn run_subcommand(workdir: &Path, bin: &str, extra_args: &[&str]) -> Result<()> {
    let mut args: Vec<&str> = vec!["run", "-p", "ble-hack-skill", "--bin", bin, "--"];
    args.extend(extra_args);
    let status = Command::new("cargo")
        .args(&args)
        .current_dir(workdir)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await
        .with_context(|| format!("cargo run --bin {bin}"))?;
    if !status.success() {
        bail!("cargo run --bin {bin} failed");
    }
    Ok(())
}

async fn run_subcommand_interactive(
    workdir: &Path,
    bin: &str,
    extra_args: &[&str],
) -> Result<std::process::ExitStatus> {
    let mut args: Vec<&str> = vec!["run", "-p", "ble-hack-skill", "--bin", bin, "--"];
    args.extend(extra_args);
    Command::new("cargo")
        .args(&args)
        .current_dir(workdir)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await
        .context("interactive subcommand")
}

fn format_scan_md(
    devices: &[pipeline::ScannedDevice],
    brand: Option<&str>,
    product: Option<&str>,
) -> String {
    let mut out = String::from("# BLE Scan Results\n\n");
    if let Some(b) = brand {
        out.push_str(&format!("- Brand filter: `{b}`\n"));
    }
    if let Some(p) = product {
        out.push_str(&format!("- Product filter: `{p}`\n"));
    }
    out.push_str(&format!("- Devices found: {}\n\n", devices.len()));
    out.push_str("| tier | device_id | name | brand_match | rssi | score |\n");
    out.push_str("| ---- | --------- | ---- | ----------- | ---- | ----- |\n");
    for d in devices {
        out.push_str(&format!(
            "| {} | `{}` | {} | {} | {} | {} |\n",
            d.tier,
            d.id,
            d.local_name.as_deref().unwrap_or("—"),
            d.brand_match,
            d.rssi.map(|r| r.to_string()).unwrap_or_else(|| "—".into()),
            d.score
        ));
    }
    out
}
