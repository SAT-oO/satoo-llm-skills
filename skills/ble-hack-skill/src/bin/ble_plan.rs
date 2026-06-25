//! Write verify plans from probe + sweep artifacts (no BLE required).
//!
//!   cargo run -p ble-hack-skill --bin ble_plan -- --workdir .

use anyhow::{Context, Result};
use ble_hack_skill::discover::{draft_verify_plan_from_sweep, parse_sweep_md};
use ble_hack_skill::verify::write_verify_plan;
use ble_hack_skill::probe_analyze::{analyze_probe, parse_probe_md};
use ble_hack_skill::workdir;
use std::fs;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let workdir = workdir::workdir_from_args(&args);
    let probe_path = workdir.join("test_results.md");
    let sweep_path = workdir.join("sweep_results.md");

    let probe_md = fs::read_to_string(&probe_path)
        .with_context(|| format!("read probe: {}", probe_path.display()))?;
    let sweep_md = fs::read_to_string(&sweep_path)
        .with_context(|| format!("read sweep: {}", sweep_path.display()))?;

    let analysis = analyze_probe(&parse_probe_md(&probe_md));
    let rows = parse_sweep_md(&sweep_md);
    let plan = draft_verify_plan_from_sweep(&rows, &analysis);

    if plan.checkpoints.is_empty() {
        anyhow::bail!("no sweep hits — run ble_probe then ble_sweep");
    }

    let plan_path = workdir.join(workdir::DEFAULT_PLAN);
    write_verify_plan(&plan_path, &plan)?;
    println!(
        "Wrote {} ({} checkpoints from {} sweep hits)",
        plan_path.display(),
        plan.checkpoints.len(),
        rows.iter().filter(|r| r.class == "echo" || r.class == "non-standard").count()
    );
    Ok(())
}
