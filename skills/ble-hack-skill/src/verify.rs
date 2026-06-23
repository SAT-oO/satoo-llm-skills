//! Shared types for verify plans and parsing `verify_results.md`.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyPlan {
    #[serde(default)]
    pub handshake: bool,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_hex: Option<String>,
    #[serde(default)]
    pub one_shot: bool,
}

#[derive(Debug, Default)]
pub struct VerifySummary {
    pub success_ids: HashSet<String>,
    pub success_rows: Vec<VerifiedRow>,
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
        for line in md.lines() {
            if !line.starts_with('|') || line.contains("verdict") || line.contains("---") {
                continue;
            }
            let parts: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
            if parts.len() < 6 {
                continue;
            }
            if parts[5].contains("success") {
                success_ids.insert(parts[1].to_string());
                let sent = parts[3].trim_matches('`').to_string();
                let sent_hex = sent.split(" (").next().unwrap_or(&sent).to_string();
                success_rows.push(VerifiedRow {
                    id: parts[1].to_string(),
                    sent: sent_hex,
                    expect: parts[4].to_string(),
                });
            }
        }
        Self {
            success_ids,
            success_rows,
        }
    }
}

pub fn write_verify_plan(path: &std::path::Path, plan: &VerifyPlan) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(plan)? + "\n";
    std::fs::write(path, json)?;
    Ok(())
}
