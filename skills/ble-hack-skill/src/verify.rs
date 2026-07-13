//! Shared types for verify plans and parsing `verify_results.md`.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyPlan {
    #[serde(default = "default_sustain_ms")]
    pub sustain_ms: u64,
    #[serde(default = "default_channel")]
    pub channel: String,
    pub checkpoints: Vec<VerifyCheckpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyCheckpoint {
    pub id: String,
    pub label: String,
    pub expect: String,
    pub burst_hex: String,
    #[serde(default = "default_burst_secs")]
    pub burst_seconds: u64,
    /// Optional actuation burst sent before `burst_hex` (e.g. prime suction before stop test).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prime_hex: Option<String>,
    #[serde(default)]
    pub prime_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_hex: Option<String>,
    /// Semicolon-separated stop frames (alternated ~2s).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_burst_hex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_burst_seconds: Option<u64>,
    #[serde(default)]
    pub one_shot: bool,
}

#[derive(Debug, Default)]
pub struct VerifySummary {
    pub success_ids: HashSet<String>,
    pub success_rows: Vec<VerifiedRow>,
    pub error_rows: Vec<VerifiedRow>,
}

#[derive(Debug, Clone)]
pub struct VerifiedRow {
    pub id: String,
    pub sent: String,
    pub expect: String,
}

fn default_sustain_ms() -> u64 {
    50
}

fn default_burst_secs() -> u64 {
    4
}

fn default_channel() -> String {
    "ffe1".into()
}

impl VerifySummary {
    pub fn from_markdown(md: &str) -> Self {
        let mut success_ids = HashSet::new();
        let mut success_rows = Vec::new();
        let mut error_rows = Vec::new();
        for line in md.lines() {
            if !line.starts_with('|') || line.contains("verdict") || line.contains("---") {
                continue;
            }
            let parts: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
            if parts.len() < 6 {
                continue;
            }
            let row = VerifiedRow {
                id: parts[1].to_string(),
                sent: parts[3].trim_matches('`').to_string(),
                expect: parts[4].to_string(),
            };
            if parts[5].contains("success") {
                success_ids.insert(parts[1].to_string());
                success_rows.push(row);
            } else if parts[5].contains("error") {
                error_rows.push(VerifiedRow {
                    id: parts[1].to_string(),
                    sent: parts[3].trim_matches('`').to_string(),
                    expect: parts[4].to_string(),
                });
            }
        }
        Self {
            success_ids,
            success_rows,
            error_rows,
        }
    }
}

pub fn write_verify_plan(path: &std::path::Path, plan: &VerifyPlan) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(plan)? + "\n";
    std::fs::write(path, json)?;
    Ok(())
}
