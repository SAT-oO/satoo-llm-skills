//! Pipeline completeness check — can artifacts reach current FINDINGS level?
//!
//!   cargo run -p ble-hack-skill --bin ble_check -- --workdir .
//!
//! Exits 0 when `Ready for FINDINGS: true`, 1 otherwise.

use anyhow::{Context, Result};
use ble_hack_skill::discover::{
    analyze_probe, draft_verify_plan_from_sweep, evaluate_pipeline, parse_probe_md, parse_sweep_md,
    render_findings_strict,
};
use ble_hack_skill::verify::{VerifySummary, write_verify_plan};
use ble_hack_skill::workdir;
use std::fs;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(ready) => {
            if ready {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(e) => {
            eprintln!("{e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<bool> {
    let args: Vec<String> = std::env::args().collect();
    let workdir = workdir::workdir_from_args(&args);
    let (brand, product) = workdir::brand_product_from_args(&workdir, &args);

    let probe_path = workdir.join("test_results.md");
    let sweep_path = workdir.join("sweep_results.md");
    let verify_path = workdir.join(workdir::DEFAULT_VERIFY_OUTPUT);
    let plan_path = workdir.join(workdir::DEFAULT_PLAN);

    let probe_md = fs::read_to_string(&probe_path)
        .with_context(|| format!("missing {}", probe_path.display()))?;
    let sweep_md = fs::read_to_string(&sweep_path)
        .with_context(|| format!("missing {}", sweep_path.display()))?;

    let verify_md = verify_path
        .exists()
        .then(|| fs::read_to_string(&verify_path))
        .transpose()?;
    let eval = evaluate_pipeline(&probe_md, &sweep_md, verify_md.as_deref());

    println!("=== Pipeline check ===\n");
    println!("Probe header: 0x{:02X}", eval.header);
    println!("Probe hot opcodes: {:?}", eval.hot_opcodes);
    println!("Probe-expanded frame count: {}", eval.expansion_frames);
    println!("Sweep hits: {}", eval.sweep_hits);
    println!("Plan checkpoints: {}", eval.plan_checkpoints);

    if !eval.missing_sweep_in_expansion.is_empty() {
        println!(
            "\nSweep hits not in probe expansion ({}):",
            eval.missing_sweep_in_expansion.len()
        );
        for m in eval.missing_sweep_in_expansion.iter().take(8) {
            println!("  - {m}");
        }
    }

    if let Some(report) = &eval.completeness {
        println!("\nVerified commands: {}", report.verified_commands);
        println!("Ready for FINDINGS: {}", report.ready_for_findings);
        if !report.missing_from_plan.is_empty() {
            println!(
                "\nSweep hits missing from plan ({}):",
                report.missing_from_plan.len()
            );
            for m in report.missing_from_plan.iter().take(10) {
                println!("  - {m}");
            }
        }
        if !report.missing_from_verify.is_empty() {
            println!(
                "\nVerified rows not in sweep ({}):",
                report.missing_from_verify.len()
            );
            for m in report.missing_from_verify.iter().take(10) {
                println!("  - {m}");
            }
        }

        let summary = VerifySummary::from_markdown(verify_md.as_ref().unwrap());
        let findings = render_findings_strict(&brand, &product, &[summary], Some(&sweep_md));
        let out = workdir.join("FINDINGS.md");
        fs::write(&out, findings)?;
        println!("\nWrote {} from verify-only renderer", out.display());
    } else {
        let analysis = analyze_probe(&parse_probe_md(&probe_md));
        let plan = draft_verify_plan_from_sweep(&parse_sweep_md(&sweep_md), &analysis);
        write_verify_plan(&plan_path, &plan)?;
        println!(
            "\nNo verify_results.md — run ble_verify after reviewing {}",
            plan_path.display()
        );
    }

    Ok(eval.ready_for_findings)
}
