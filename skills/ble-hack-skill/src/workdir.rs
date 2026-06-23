//! Resolve device UUID and artifact paths from a project workdir (no per-run UUID typing).

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const SESSION_FILE: &str = "ble_session.json";
pub const DEFAULT_PLAN: &str = "verify_plan.json";
pub const EXTENDED_PLAN: &str = "verify_plan_extended.json";
pub const DEFAULT_VERIFY_OUTPUT: &str = "verify_results.md";
pub const EXTENDED_VERIFY_OUTPUT: &str = "verify_results_extended.md";
pub const SCAN_RESULTS: &str = "scan_results.md";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionFile {
    pub device_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_name: Option<String>,
}

pub fn workdir_from_args(args: &[String]) -> PathBuf {
    arg_value(args, "--workdir")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn resolve_device(workdir: &Path, cli_device: Option<&str>) -> Result<String> {
    if let Some(id) = cli_device {
        return Ok(id.to_string());
    }

    let session_path = workdir.join(SESSION_FILE);
    if session_path.exists() {
        let session: SessionFile = serde_json::from_str(
            &fs::read_to_string(&session_path)
                .with_context(|| format!("read {}", session_path.display()))?,
        )
        .with_context(|| format!("parse {}", session_path.display()))?;
        println!(
            "Using device from {}: {} ({})",
            SESSION_FILE,
            session.device_id,
            session.local_name.as_deref().unwrap_or("—")
        );
        return Ok(session.device_id);
    }

    if let Some(id) = device_from_scan_results(&workdir.join(SCAN_RESULTS))? {
        println!("Using device from {SCAN_RESULTS}: {id}");
        return Ok(id);
    }

    for name in [DEFAULT_VERIFY_OUTPUT, "test_results.md"] {
        let path = workdir.join(name);
        if let Some(id) = device_from_markdown_device_line(&path)? {
            println!("Using device from {}: {id}", path.display());
            return Ok(id);
        }
    }

    bail!(
        "no device — run ble_scan/ble_run first, or pass --device UUID (writes {SESSION_FILE})"
    )
}

pub fn resolve_plan_path(workdir: &Path, cli_plan: Option<&str>) -> PathBuf {
    cli_plan
        .map(PathBuf::from)
        .unwrap_or_else(|| workdir.join(DEFAULT_PLAN))
}

pub fn resolve_output_path(workdir: &Path, cli_output: Option<&str>) -> PathBuf {
    cli_output
        .map(PathBuf::from)
        .unwrap_or_else(|| workdir.join(DEFAULT_VERIFY_OUTPUT))
}

/// Brand/product for FINDINGS rendering — CLI flags, else session local name, else generic.
pub fn brand_product_from_args(workdir: &Path, args: &[String]) -> (String, String) {
    let brand = arg_value(args, "--brand").unwrap_or_else(|| "Unknown".into());
    let product = arg_value(args, "--product").unwrap_or_else(|| {
        session_local_name(workdir).unwrap_or_else(|| "BLE Device".into())
    });
    (brand, product)
}

pub fn session_local_name(workdir: &Path) -> Option<String> {
    let path = workdir.join(SESSION_FILE);
    let session: SessionFile = serde_json::from_str(&fs::read_to_string(path).ok()?).ok()?;
    session.local_name
}

pub fn save_session(workdir: &Path, device_id: &str, local_name: Option<&str>) -> Result<()> {
    let session = SessionFile {
        device_id: device_id.to_string(),
        local_name: local_name.map(str::to_string),
    };
    let path = workdir.join(SESSION_FILE);
    fs::write(&path, serde_json::to_string_pretty(&session)? + "\n")?;
    Ok(())
}

fn device_from_scan_results(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path)?;
    let mut best: Option<(i32, String)> = None;

    for line in text.lines() {
        if !line.starts_with('|') || line.contains("device_id") || line.contains("---") {
            continue;
        }
        let parts: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
        if parts.len() < 4 {
            continue;
        }
        let tier = parts[1];
        if tier == "SKIP" {
            continue;
        }
        let id = parts[2].trim_matches('`').to_string();
        if id.is_empty() || id == "—" {
            continue;
        }
        let brand_match = parts.get(4).is_some_and(|s| *s == "true");
        let mut score = match tier {
            "PRIMARY" => 100,
            "CANDIDATE" => 50,
            _ => 10,
        };
        if brand_match {
            score += 200;
        }
        if best.as_ref().is_none_or(|(s, _)| score > *s) {
            best = Some((score, id));
        }
    }

    Ok(best.map(|(_, id)| id))
}

fn device_from_markdown_device_line(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    for line in fs::read_to_string(path)?.lines() {
        if let Some(rest) = line.strip_prefix("- Device: `") {
            if let Some(id) = rest.strip_suffix('`') {
                return Ok(Some(id.to_string()));
            }
        }
    }
    Ok(None)
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scan_primary_row() {
        let md = r#"
| tier | device_id | name | brand_match | rssi | score |
| PRIMARY | `abc-123` | Example Device | true | -44 | 120 |
"#;
        let dir = std::env::temp_dir().join("ble_workdir_test_scan");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(SCAN_RESULTS), md).unwrap();
        assert_eq!(
            device_from_scan_results(&dir.join(SCAN_RESULTS))
                .unwrap()
                .as_deref(),
            Some("abc-123")
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
