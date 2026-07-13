//! Render FINDINGS.md from verify_results*.md success rows.

use anyhow::{Context, Result};
use ble_hack_skill::discover;
use ble_hack_skill::verify::VerifySummary;
use ble_hack_skill::workdir;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let workdir = workdir::workdir_from_args(&args);
    let (brand, product) = workdir::brand_product_from_args(&workdir, &args);
    let output = arg_value(&args, "--output")
        .map(PathBuf::from)
        .unwrap_or_else(|| workdir.join("FINDINGS.md"));

    let mut summaries = Vec::new();
    let mut found = false;
    for entry in fs::read_dir(&workdir)? {
        let entry = entry?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with("verify_results") || !name.ends_with(".md") {
            continue;
        }
        let md = fs::read_to_string(&path)?;
        if md.contains("**success**") {
            found = true;
        }
        summaries.push(VerifySummary::from_markdown(&md));
        println!("Loaded {}", path.display());
    }

    if !found {
        anyhow::bail!("no success rows in verify_results*.md — run ble_verify first");
    }

    let sweep_md = fs::read_to_string(workdir.join("sweep_results.md")).ok();
    let body = discover::render_findings_for_workdir(
        &brand,
        &product,
        &summaries,
        sweep_md.as_deref(),
        &workdir,
    );
    fs::write(&output, body).with_context(|| format!("write {}", output.display()))?;
    println!("Wrote {}", output.display());
    Ok(())
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}
