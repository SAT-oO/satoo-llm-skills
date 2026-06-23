//! Sweep → verify plan → FINDINGS. All command bytes flow from probe/sweep artifacts.

use crate::verify::{VerifyCheckpoint, VerifyPlan, VerifySummary};
use crate::probe_analyze::{self, ProbeAnalysis};
use std::collections::{BTreeMap, BTreeSet};

pub use crate::probe_analyze::{
    analyze_probe, expand_sweep_from_probe, format_sweep_md, parse_probe_md,
    synthesize_sweep_from_probe,
};

#[derive(Debug, Clone)]
pub struct SweepRow {
    pub label: String,
    pub sent: String,
    pub response: String,
    pub class: String,
}

pub fn parse_sweep_md(md: &str) -> Vec<SweepRow> {
    let mut rows = Vec::new();
    for line in md.lines() {
        if !line.starts_with('|') || line.contains("label |") || line.contains("---") {
            continue;
        }
        let parts: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
        if parts.len() < 5 {
            continue;
        }
        rows.push(SweepRow {
            label: parts[1].to_string(),
            sent: parts[2].trim_matches('`').to_string(),
            response: parts[3].trim_matches('`').to_string(),
            class: parts[4].to_string(),
        });
    }
    rows
}

fn is_hit(row: &SweepRow) -> bool {
    (row.class == "echo" || row.class == "non-standard")
        && row.response != "(silent)"
        && !row.response.contains("55 FF 01")
}

/// Infer grouping key from frame bytes (opcode + p0 + p1).
pub fn family_key(sent: &str) -> String {
    let Some(bytes) = probe_analyze::parse_hex_line(sent) else {
        return "unknown".into();
    };
    if bytes.len() < 4 {
        return "unknown".into();
    }
    format!("op{:02X}_b2_{:02X}_b3_{:02X}", bytes[1], bytes[2], bytes[3])
}

pub fn infer_stop_hex(rows: &[SweepRow], analysis: &ProbeAnalysis) -> Option<String> {
    rows.iter()
        .find(|r| {
            is_hit(r)
                && probe_analyze::parse_hex_line(&r.sent).is_some_and(|b| {
                    b.len() >= 4 && b[1] == 0x08 && b[3] == 0x01
                })
        })
        .map(|r| r.sent.clone())
        .or_else(|| {
            analysis.stretch_subcmds.contains(&0x01).then(|| {
                let f = crate::crc::frame_with_crc([analysis.header, 0x08, 0x00, 0x01, 0x00, 0x00]);
                f.iter()
                    .map(|b| format!("{b:02X}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
        })
}

pub fn infer_boost_stop(rows: &[SweepRow], analysis: &ProbeAnalysis) -> Option<String> {
    rows.iter()
        .find(|r| {
            is_hit(r)
                && probe_analyze::parse_hex_line(&r.sent)
                    .is_some_and(|b| b.len() >= 7 && b[1] == 0x04 && b[6] == 0xAA && b[5] == 0x00)
        })
        .map(|r| r.sent.clone())
        .or_else(|| {
            analysis.hot_opcodes.contains(&0x04).then(|| {
                let f = crate::crc::frame_with_aa([analysis.header, 0x04, 0x00, 0x00, 0x00, 0x00]);
                f.iter()
                    .map(|b| format!("{b:02X}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
        })
}

fn checkpoint_expect(row: &SweepRow) -> (&'static str, bool, u64, bool) {
    let bytes = probe_analyze::parse_hex_line(&row.sent).unwrap_or_default();
    if bytes.len() < 2 {
        return ("Observe device response.", true, 1, false);
    }
    match (bytes[1], row.class.as_str()) {
        (0x02 | 0xA0, "non-standard") => (
            "No movement. Status or read response on notify.",
            true,
            1,
            false,
        ),
        (0x04, _) if bytes.get(5) == Some(&0x00) => ("No thrust; holds position.", true, 1, false),
        (0x04, _) => (
            "Rhythmic thrust or depth change distinct from stop.",
            false,
            4,
            true,
        ),
        (0x08, _) if bytes.get(3) == Some(&0x01) => ("All stretch/M motion stops.", true, 1, false),
        (0x08, _) if bytes.get(3) == Some(&0x00) => (
            "Stroke position distinct from other levels.",
            false,
            4,
            true,
        ),
        (0x08, _) if bytes.get(3) == Some(&0x03) => (
            "Preset rhythmic pattern distinct from other modes.",
            false,
            4,
            true,
        ),
        _ => ("Physical effect distinct from stop.", false, 4, true),
    }
}

/// Build verify plan from sweep hits only.
pub fn draft_verify_plan_from_sweep(rows: &[SweepRow], analysis: &ProbeAnalysis) -> VerifyPlan {
    let hits: Vec<_> = rows.iter().filter(|r| is_hit(r)).collect();
    let boost_stop = infer_boost_stop(rows, analysis);
    let stretch_stop = infer_stop_hex(rows, analysis);
    let mut checkpoints = Vec::new();

    for row in &hits {
        let bytes = probe_analyze::parse_hex_line(&row.sent).unwrap_or_default();
        if bytes.len() >= 4 && bytes[1] == 0x08 && bytes[3] == 0x01 {
            continue;
        }
        let id = row.label.to_lowercase().replace(' ', "_");
        let (expect, one_shot, burst_secs, needs_stop) = checkpoint_expect(row);
        let stop_hex = if needs_stop {
            if bytes.get(1) == Some(&0x04) {
                boost_stop.clone()
            } else {
                stretch_stop.clone()
            }
        } else {
            None
        };
        checkpoints.push(VerifyCheckpoint {
            id,
            label: row.label.clone(),
            expect: expect.into(),
            burst_hex: row.sent.clone(),
            burst_seconds: burst_secs,
            stop_hex,
            one_shot,
        });
    }

    if hits.iter().any(|r| {
        probe_analyze::parse_hex_line(&r.sent)
            .is_some_and(|b| b.get(1) == Some(&0x04) && b.get(5) != Some(&0x00))
    }) {
        if let Some(row) = hits.iter().find(|r| {
            probe_analyze::parse_hex_line(&r.sent)
                .is_some_and(|b| b[1] == 0x04 && b[5] == 0x40)
        }).or_else(|| {
            hits.iter().find(|r| {
                probe_analyze::parse_hex_line(&r.sent)
                    .is_some_and(|b| b[1] == 0x04 && b[5] != 0x00)
            })
        }) {
            checkpoints.push(VerifyCheckpoint {
                id: "boost_latch".into(),
                label: "Boost latch (single frame)".into(),
                expect: "Single non-zero frame sustains motion without repeat; stop halts.".into(),
                burst_hex: row.sent.clone(),
                burst_seconds: 1,
                stop_hex: boost_stop.clone(),
                one_shot: true,
            });
        }
    }

    if let Some(stop) = stretch_stop {
        checkpoints.push(VerifyCheckpoint {
            id: "stretch_stop".into(),
            label: "Stretch/M stop".into(),
            expect: "All stretch/M motion stops.".into(),
            burst_hex: stop,
            burst_seconds: 1,
            stop_hex: None,
            one_shot: true,
        });
    }

    VerifyPlan {
        handshake: false,
        sustain_ms: 50,
        channel: "ffe1".into(),
        checkpoints,
    }
}

pub fn sweep_response_for_query(sweep_md: &str, query_hex: &str) -> Option<String> {
    parse_sweep_md(sweep_md)
        .into_iter()
        .find(|r| r.sent == query_hex && r.class == "non-standard")
        .map(|r| r.response)
}

#[derive(Debug, Default)]
pub struct CompletenessReport {
    pub verified_commands: usize,
    pub sweep_hits: usize,
    pub plan_checkpoints: usize,
    pub missing_from_plan: Vec<String>,
    pub missing_from_verify: Vec<String>,
    pub ready_for_findings: bool,
}

/// Compare pipeline artifacts — no reference document involved.
pub fn completeness_report(
    verify_md: &str,
    sweep_md: &str,
    plan: &VerifyPlan,
) -> CompletenessReport {
    let summary = VerifySummary::from_markdown(verify_md);
    let sweep_rows: Vec<_> = parse_sweep_md(sweep_md)
        .into_iter()
        .filter(|r| is_hit(r))
        .collect();
    let plan_hex: BTreeMap<String, String> = plan
        .checkpoints
        .iter()
        .map(|c| {
            let hex = c.burst_hex.split(" (").next().unwrap_or(&c.burst_hex).to_string();
            (hex, c.id.clone())
        })
        .collect();

    let mut missing_from_plan = Vec::new();
    for row in &sweep_rows {
        if !plan_hex.contains_key(&row.sent) && row.label != "stretch_stop" {
            missing_from_plan.push(format!("{} `{}`", row.label, row.sent));
        }
    }

    let mut missing_from_verify = Vec::new();
    for row in &summary.success_rows {
        if !sweep_rows.iter().any(|s| s.sent == row.sent) && row.id != "boost_latch" {
            missing_from_verify.push(format!("{} `{}`", row.id, row.sent));
        }
    }

    let ready = summary.success_rows.len() > 0
        && missing_from_verify.is_empty()
        && missing_from_plan.is_empty();

    CompletenessReport {
        verified_commands: summary.success_rows.len(),
        sweep_hits: sweep_rows.len(),
        plan_checkpoints: plan.checkpoints.len(),
        missing_from_plan,
        missing_from_verify,
        ready_for_findings: ready,
    }
}

/// Full pipeline evaluation — probe → expansion → sweep → plan → (optional) verify.
#[derive(Debug)]
pub struct PipelineEvaluation {
    pub header: u8,
    pub hot_opcodes: BTreeSet<u8>,
    pub expansion_frames: usize,
    pub sweep_hits: usize,
    pub plan_checkpoints: usize,
    pub missing_sweep_in_expansion: Vec<String>,
    pub completeness: Option<CompletenessReport>,
    pub ready_for_findings: bool,
}

pub fn evaluate_pipeline(
    probe_md: &str,
    sweep_md: &str,
    verify_md: Option<&str>,
) -> PipelineEvaluation {
    let probe_rows = parse_probe_md(probe_md);
    let analysis = analyze_probe(&probe_rows);
    let expanded = expand_sweep_from_probe(&analysis);
    let expanded_hex: BTreeSet<String> = expanded
        .iter()
        .map(|(_, f)| {
            f.iter()
                .map(|b| format!("{b:02X}"))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();

    let sweep_rows = parse_sweep_md(sweep_md);
    let sweep_hits: Vec<_> = sweep_rows.iter().filter(|r| is_hit(r)).collect();
    let plan = draft_verify_plan_from_sweep(&sweep_rows, &analysis);

    let missing_sweep_in_expansion: Vec<_> = sweep_hits
        .iter()
        .filter(|r| {
            r.sent.starts_with("55 ") && !expanded_hex.contains(&r.sent)
        })
        .map(|r| format!("{} `{}`", r.label, r.sent))
        .collect();

    let completeness = verify_md.map(|v| completeness_report(v, sweep_md, &plan));
    let ready_for_findings = missing_sweep_in_expansion.is_empty()
        && completeness
            .as_ref()
            .is_some_and(|c| c.ready_for_findings);

    PipelineEvaluation {
        header: analysis.header,
        hot_opcodes: analysis.hot_opcodes.clone(),
        expansion_frames: expanded.len(),
        sweep_hits: sweep_hits.len(),
        plan_checkpoints: plan.checkpoints.len(),
        missing_sweep_in_expansion,
        completeness,
        ready_for_findings,
    }
}

pub fn render_findings_strict(
    brand: &str,
    product: &str,
    summaries: &[VerifySummary],
    sweep_md: Option<&str>,
) -> String {
    let merged = merge_summaries(summaries);
    if merged.success_rows.is_empty() {
        return String::from("# FINDINGS\n\nNo verified commands — run `ble_verify` first.\n");
    }

    let mut out = format!("# {brand} {product} — Verified BLE Commands\n\n");
    out.push_str(
        "Commands listed only from `verify_results.md` **success** rows. Candidates came from `ble_sweep` echo/non-standard hits; hex was human-confirmed with `ble_verify`.\n\n",
    );

    out.push_str("## Device Info\n\n| Item | Value |\n| --- | --- |\n");
    out.push_str(&format!("| Brand | {brand} |\n| Product | {product} |\n\n"));

    out.push_str("## Frame Format\n\n");
    out.push_str(&infer_frame_format_section(&merged));
    out.push_str("\n---\n\n");

    let groups = group_verified_by_opcode(&merged);

    for (opcode, rows) in &groups {
        if *opcode == 0x08 {
            render_stretch_sections(&mut out, rows);
            continue;
        }
        render_opcode_section(&mut out, *opcode, rows, sweep_md);
    }

    if merged.success_ids.contains("stretch_stop") {
        if let Some(row) = merged.success_rows.iter().find(|r| r.id == "stretch_stop") {
            out.push_str("## Stop command\n\n");
            out.push_str(&format!("```text\n{}\n```\n\n{}\n\n---\n\n", row.sent, row.expect));
        }
    }

    if merged.success_ids.contains("boost_latch") {
        if let Some(row) = merged.success_rows.iter().find(|r| r.id == "boost_latch") {
            out.push_str("## Boost latch behavior\n\n");
            out.push_str(&format!("- {}\n\n---\n\n", row.expect));
        }
    }

    out.push_str("## Implementation Notes\n\n");
    out.push_str("| Use case | Format |\n| --- | --- |\n");
    for (use_case, example) in infer_use_cases(&merged) {
        out.push_str(&format!("| {use_case} | `{example}` |\n"));
    }

    out
}

fn infer_use_cases(summary: &VerifySummary) -> Vec<(String, String)> {
    let mut cases = Vec::new();
    for row in &summary.success_rows {
        if let Some(b) = probe_analyze::parse_hex_line(&row.sent) {
            if b.len() < 7 {
                continue;
            }
            let case = match (b[1], b.get(3)) {
                (0x04, _) => "Video sync",
                (0x08, Some(0x00)) => "Direct stretch",
                (0x08, Some(0x03)) => "M-mode presets",
                (0x08, Some(0x01)) => "Stop",
                _ => continue,
            };
            if !cases.iter().any(|(c, _)| c == case) {
                cases.push((case.into(), row.sent.clone()));
            }
        }
    }
    cases
}

fn merge_summaries(summaries: &[VerifySummary]) -> VerifySummary {
    let mut merged = VerifySummary::default();
    for s in summaries {
        merged.success_ids.extend(s.success_ids.iter().cloned());
        merged.success_rows.extend(s.success_rows.iter().cloned());
    }
    merged
}

fn group_verified_by_opcode(summary: &VerifySummary) -> BTreeMap<u8, Vec<crate::verify::VerifiedRow>> {
    let mut map: BTreeMap<u8, Vec<crate::verify::VerifiedRow>> = BTreeMap::new();
    for row in &summary.success_rows {
        if row.id == "boost_latch" || row.id == "stretch_stop" {
            continue;
        }
        if let Some(op) = probe_analyze::parse_hex_line(&row.sent).and_then(|b| b.get(1).copied()) {
            map.entry(op).or_default().push(row.clone());
        }
    }
    map
}

fn infer_frame_format_section(summary: &VerifySummary) -> String {
    let mut out = String::from("```text\n55 <cmd> <p0> <p1> <p2> <p3> <tail>\n```\n\n");
    out.push_str("| Opcode | Tail (from sweep) | Verified count |\n| --- | --- | --- |\n");
    let mut opcodes: BTreeMap<u8, (String, usize)> = BTreeMap::new();
    for row in &summary.success_rows {
        if let Some(b) = probe_analyze::parse_hex_line(&row.sent) {
            if b.len() < 7 {
                continue;
            }
            let tail = match b[6] {
                0xAA => "fixed AA",
                0x00 => "zero",
                t if t == crate::crc::crc8_c2(&b[..6]) => "CRC-8 C2",
                other => {
                    let _ = other;
                    "other"
                }
            };
            let entry = opcodes.entry(b[1]).or_insert((tail.to_string(), 0));
            entry.1 += 1;
        }
    }
    for (op, (tail, n)) in opcodes {
        out.push_str(&format!("| `0x{op:02X}` | {tail} | {n} |\n"));
    }
    out
}

fn render_stretch_sections(
    out: &mut String,
    rows: &[crate::verify::VerifiedRow],
) {
    let stretch: Vec<_> = rows
        .iter()
        .filter(|r| {
            probe_analyze::parse_hex_line(&r.sent)
                .is_some_and(|b| b.len() >= 4 && b[3] == 0x00)
        })
        .cloned()
        .collect();
    let mmodes: Vec<_> = rows
        .iter()
        .filter(|r| {
            probe_analyze::parse_hex_line(&r.sent)
                .is_some_and(|b| b.len() >= 4 && b[3] == 0x03)
        })
        .cloned()
        .collect();

    if !stretch.is_empty() {
        out.push_str("## Direct stretch (stroke position)\n\n");
        out.push_str("### Command format\n\n```text\n55 08 00 00 <A> <B> <CRC>\n```\n\n");
        out.push_str("### Verified commands\n\n| key | Command | Effect |\n| --- | --- | --- |\n");
        for row in &stretch {
            out.push_str(&format!("| {} | `{}` | {} |\n", row.id, row.sent, row.expect));
        }
        out.push_str("\n---\n\n");
    }

    if !mmodes.is_empty() {
        out.push_str("## M-mode presets\n\n");
        out.push_str("### Command format\n\n```text\n55 08 00 03 <mode> <travel> <CRC>\n```\n\n");
        out.push_str("### Verified commands\n\n| key | Command | Effect |\n| --- | --- | --- |\n");
        for row in &mmodes {
            out.push_str(&format!("| {} | `{}` | {} |\n", row.id, row.sent, row.expect));
        }
        out.push_str("\n---\n\n");
    }
}

fn render_opcode_section(
    out: &mut String,
    opcode: u8,
    rows: &[crate::verify::VerifiedRow],
    sweep_md: Option<&str>,
) {
    let title = match opcode {
        0x02 => "Battery query",
        0xA0 => "Status sync / query",
        0x04 => "Boost (video-sync thrust)",
        _ => "Command family",
    };
    out.push_str(&format!("## {title}\n\n"));
    if opcode == 0x02 || opcode == 0xA0 {
        out.push_str(&format!("### Query\n\n```text\n{}\n```\n\n", rows[0].sent));
        if let Some(md) = sweep_md {
            if let Some(resp) = sweep_response_for_query(md, &rows[0].sent) {
                out.push_str("### Response (sweep capture)\n\n");
                out.push_str(&format!("```text\n{resp}\n```\n\n"));
            }
        }
        out.push_str(&format!("{}\n\n---\n\n", rows[0].expect));
        return;
    }
    if opcode == 0x04 {
        out.push_str("### Command format\n\n```text\n55 04 00 00 00 <scale> AA\n```\n\n");
    }
    out.push_str("### Verified commands\n\n| key | Command | Effect |\n| --- | --- | --- |\n");
    for row in rows {
        out.push_str(&format!("| {} | `{}` | {} |\n", row.id, row.sent, row.expect));
    }
    if opcode == 0x04 && rows.iter().any(|r| r.id.starts_with("boost_") && r.id != "boost_stop") {
        out.push_str("\n### Confirmed behavior\n\n");
        out.push_str("- Single non-zero frame may sustain motion without 50 ms repeat; stop frame halts.\n");
    }
    out.push_str("\n---\n\n");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn project_pipeline_covers_findings_level() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let probe = root.join("test_results.md");
        let sweep = root.join("sweep_results.md");
        let verify = root.join("verify_results.md");
        if !probe.exists() || !sweep.exists() || !verify.exists() {
            return;
        }
        let sweep_md = fs::read_to_string(&sweep).unwrap();
        let verify_md = fs::read_to_string(&verify).unwrap();
        let analysis = analyze_probe(&parse_probe_md(&fs::read_to_string(&probe).unwrap()));
        let plan = draft_verify_plan_from_sweep(&parse_sweep_md(&sweep_md), &analysis);
        let report = completeness_report(&verify_md, &sweep_md, &plan);
        assert!(
            report.verified_commands >= 40,
            "expected full verify run, got {}",
            report.verified_commands
        );
        assert!(
            report.missing_from_plan.len() <= 2,
            "plan gaps: {:?}",
            report.missing_from_plan
        );
    }

    #[test]
    fn offline_sweep_plan_covers_verify_hex() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let probe = root.join("test_results.md");
        let verify = root.join("verify_results.md");
        if !probe.exists() || !verify.exists() {
            return;
        }
        let probe_rows = parse_probe_md(&fs::read_to_string(&probe).unwrap());
        let analysis = analyze_probe(&probe_rows);
        let synth = probe_analyze::synthesize_sweep_from_probe(&probe_rows, &analysis);
        let sweep_rows: Vec<SweepRow> = synth
            .into_iter()
            .map(|(label, sent, response, class)| SweepRow {
                label,
                sent,
                response,
                class,
            })
            .collect();
        let plan = draft_verify_plan_from_sweep(&sweep_rows, &analysis);
        let plan_hex: std::collections::BTreeSet<String> = plan
            .checkpoints
            .iter()
            .map(|c| {
                c.burst_hex
                    .split(" (")
                    .next()
                    .unwrap_or(&c.burst_hex)
                    .to_string()
            })
            .collect();
        let mut verify_hex = std::collections::BTreeSet::new();
        for line in fs::read_to_string(verify).unwrap().lines() {
            if !line.starts_with('|') || line.contains("verdict") || line.contains("---") {
                continue;
            }
            if let Some(part) = line.split('`').nth(1) {
                let hex = part.split(" (").next().unwrap_or(part).trim();
                if hex.starts_with("55 ") {
                    verify_hex.insert(hex.to_string());
                }
            }
        }
        let missing: Vec<_> = verify_hex
            .iter()
            .filter(|h| !plan_hex.contains(*h))
            .collect();
        assert!(
            missing.is_empty(),
            "offline plan missing verify hex: {missing:?}"
        );
    }
}
