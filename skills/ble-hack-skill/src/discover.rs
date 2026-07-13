//! Sweep → verify plan → FINDINGS. All command bytes flow from probe/sweep artifacts.

use crate::probe_analyze::{self, ProbeAnalysis};
use crate::verify::{VerifyCheckpoint, VerifyPlan, VerifySummary};
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
                && probe_analyze::parse_hex_line(&r.sent)
                    .is_some_and(|b| b.len() >= 4 && b[1] == 0x08 && b[3] == 0x01)
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
            prime_hex: None,
            prime_seconds: None,
            stop_hex,
            stop_burst_hex: None,
            stop_burst_seconds: None,
            one_shot,
        });
    }

    if hits.iter().any(|r| {
        probe_analyze::parse_hex_line(&r.sent)
            .is_some_and(|b| b.get(1) == Some(&0x04) && b.get(5) != Some(&0x00))
    }) {
        if let Some(row) = hits
            .iter()
            .find(|r| {
                probe_analyze::parse_hex_line(&r.sent).is_some_and(|b| b[1] == 0x04 && b[5] == 0x40)
            })
            .or_else(|| {
                hits.iter().find(|r| {
                    probe_analyze::parse_hex_line(&r.sent)
                        .is_some_and(|b| b[1] == 0x04 && b[5] != 0x00)
                })
            })
        {
            checkpoints.push(VerifyCheckpoint {
                id: "boost_latch".into(),
                label: "Boost latch (single frame)".into(),
                expect: "Single non-zero frame sustains motion without repeat; stop halts.".into(),
                burst_hex: row.sent.clone(),
                burst_seconds: 1,
                prime_hex: None,
                prime_seconds: None,
                stop_hex: boost_stop.clone(),
                stop_burst_hex: None,
                stop_burst_seconds: None,
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
            prime_hex: None,
            prime_seconds: None,
            stop_hex: None,
            stop_burst_hex: None,
            stop_burst_seconds: None,
            one_shot: true,
        });
    }

    VerifyPlan {
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
            let hex = c
                .burst_hex
                .split(" (")
                .next()
                .unwrap_or(&c.burst_hex)
                .to_string();
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
        .filter(|r| r.sent.starts_with("55 ") && !expanded_hex.contains(&r.sent))
        .map(|r| format!("{} `{}`", r.label, r.sent))
        .collect();

    let completeness = verify_md.map(|v| completeness_report(v, sweep_md, &plan));
    let ready_for_findings = missing_sweep_in_expansion.is_empty()
        && completeness.as_ref().is_some_and(|c| c.ready_for_findings);

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

fn merge_summaries(summaries: &[VerifySummary]) -> VerifySummary {
    let mut merged = VerifySummary::default();
    for s in summaries {
        merged.success_ids.extend(s.success_ids.iter().cloned());
        merged.success_rows.extend(s.success_rows.iter().cloned());
        merged.error_rows.extend(s.error_rows.iter().cloned());
    }
    merged
}

/// Minimal byte-oriented FINDINGS (see `FINDINGS.template.md`).
pub fn render_findings_strict(
    brand: &str,
    product: &str,
    summaries: &[VerifySummary],
    _sweep_md: Option<&str>,
    write_target: Option<&str>,
) -> String {
    let merged = merge_summaries(summaries);
    if merged.success_rows.is_empty() {
        return String::from("# FINDINGS\n\nNo verified commands — run `ble_verify` first.\n");
    }

    let write_line = write_target
        .map(str::to_string)
        .unwrap_or_else(|| "Write characteristic (see scan_results.md)".into());

    let mut out = format!("# {brand} {product} — BLE Commands\n\n");
    out.push_str(&format!("Write to **{write_line}**. "));

    let families = group_frame_families(&merged.success_rows);
    if families.iter().any(|f| !f.speed_byte_indices.is_empty()) {
        out.push_str("Each speed byte is `0x00` (off) through `0xFF` (max).\n\n");
    } else {
        out.push('\n');
    }

    for family in &families {
        render_frame_family(&mut out, family);
    }

    render_orgasm_section(&mut out, &merged);
    render_cautions(&mut out, &merged, write_target);

    out
}

fn primary_hex_from_sent(sent: &str) -> Option<Vec<u8>> {
    for token in sent.split(['→', ';']) {
        let t = token.split('(').next().unwrap_or(token).trim();
        if let Some(b) = probe_analyze::parse_hex_line(t) {
            return Some(b);
        }
    }
    probe_analyze::parse_hex_line(sent)
}

fn hex_label_from_sent(sent: &str) -> String {
    for token in sent.split(['→', ';']) {
        let t = token.split('(').next().unwrap_or(token).trim();
        if probe_analyze::parse_hex_line(t).is_some() {
            return t.to_string();
        }
    }
    sent.split('(').next().unwrap_or(sent).trim().to_string()
}

struct FrameFamily {
    prefix_label: String,
    length: usize,
    speed_byte_indices: Vec<usize>,
    byte_labels: Vec<String>,
    example: String,
}

fn group_frame_families(rows: &[crate::verify::VerifiedRow]) -> Vec<FrameFamily> {
    let mut map: BTreeMap<String, Vec<&crate::verify::VerifiedRow>> = BTreeMap::new();
    for row in rows {
        if row.id.contains("orgasm") {
            continue;
        }
        let Some(bytes) = primary_hex_from_sent(&row.sent) else {
            continue;
        };
        let key = if bytes.len() >= 2 {
            format!("{}:{}:{}", bytes.len(), bytes[0], bytes[1])
        } else {
            format!("{}:{}", bytes.len(), bytes.first().copied().unwrap_or(0))
        };
        map.entry(key).or_default().push(row);
    }

    let mut families = Vec::new();
    for (_key, group) in map {
        let Some(first_bytes) = primary_hex_from_sent(&group[0].sent) else {
            continue;
        };
        let speed_indices = infer_speed_byte_indices(&group);
        let labels = infer_byte_labels(&group, first_bytes.len(), &speed_indices);
        let prefix = if first_bytes.len() >= 2 {
            format!("{:02X} {:02X}", first_bytes[0], first_bytes[1])
        } else {
            format!("{:02X}", first_bytes[0])
        };
        families.push(FrameFamily {
            prefix_label: prefix,
            length: first_bytes.len(),
            speed_byte_indices: speed_indices,
            byte_labels: labels,
            example: pick_example_hex(&group),
        });
    }
    families.sort_by_key(|f| f.length);
    families
}

fn infer_speed_byte_indices(group: &[&crate::verify::VerifiedRow]) -> Vec<usize> {
    let parsed: Vec<Vec<u8>> = group
        .iter()
        .filter_map(|r| primary_hex_from_sent(&r.sent))
        .collect();
    if parsed.is_empty() {
        return Vec::new();
    }
    let len = parsed[0].len();
    let mut varying = Vec::new();
    for i in 2..len {
        let values: BTreeSet<u8> = parsed.iter().filter_map(|b| b.get(i).copied()).collect();
        if values.len() > 1 {
            varying.push(i);
        }
    }
    varying
}

fn infer_byte_labels(
    group: &[&crate::verify::VerifiedRow],
    len: usize,
    speed_indices: &[usize],
) -> Vec<String> {
    let mut labels = vec!["—".into(); len];
    for row in group {
        let id = row.id.to_ascii_lowercase();
        let Some(bytes) = primary_hex_from_sent(&row.sent) else {
            continue;
        };
        if id.contains("extension") && bytes.len() > 2 {
            labels[2] = "Extension speed".into();
        }
        if id.contains("vib1") && bytes.len() > 3 {
            labels[3] = "Vibration 1 speed".into();
        }
        if id.contains("vib2") && bytes.len() > 4 {
            labels[4] = "Vibration 2 speed".into();
        }
    }
    for (idx, label) in labels.iter_mut().enumerate() {
        if *label == "—" {
            if speed_indices.contains(&idx) {
                *label = format!("Byte {idx} speed");
            } else if idx == 0 {
                *label = "Header (fixed)".into();
            } else if idx == 1 {
                *label = "Opcode (fixed)".into();
            } else {
                *label = format!("Byte {idx}");
            }
        }
    }
    labels
}

fn pick_example_hex(group: &[&crate::verify::VerifiedRow]) -> String {
    if let Some(row) = group
        .iter()
        .find(|r| r.id.contains("vib1") && !r.id.contains("vib2"))
    {
        return hex_label_from_sent(&row.sent);
    }
    group
        .first()
        .map(|r| hex_label_from_sent(&r.sent))
        .unwrap_or_else(|| "—".into())
}

fn render_frame_family(out: &mut String, family: &FrameFamily) {
    out.push_str(&format!(
        "**Motor frame ({} bytes):**\n\n",
        family.length
    ));
    out.push_str("| Byte | Value | Meaning |\n| --- | --- | --- |\n");
    let prefix_parts: Vec<&str> = family.prefix_label.split_whitespace().collect();
    for i in 0..family.length {
        let value = if family.speed_byte_indices.contains(&i) {
            "`00`–`FF`".to_string()
        } else if i < prefix_parts.len() {
            format!("`{}`", prefix_parts[i])
        } else {
            "`00`–`FF`".to_string()
        };
        let meaning = family
            .byte_labels
            .get(i)
            .cloned()
            .unwrap_or_else(|| format!("Byte {i}"));
        out.push_str(&format!("| {i} | {value} | {meaning} |\n"));
    }
    if !family.example.is_empty() && family.example != "—" {
        out.push_str(&format!("\nExample: `{}`.\n\n", family.example));
    } else {
        out.push('\n');
    }
}

fn render_orgasm_section(out: &mut String, merged: &VerifySummary) {
    let orgasm_rows: Vec<_> = merged
        .success_rows
        .iter()
        .filter(|r| r.id.contains("orgasm"))
        .collect();
    if orgasm_rows.is_empty() {
        return;
    }

    let arm = merged.success_rows.iter().find(|r| {
        r.sent.contains("01")
            || r.id.contains("orgasm_arm")
            || r.id.contains("report14_on")
    });
    let sustain = orgasm_rows.iter().find(|r| {
        r.sent.contains("A0 03")
            || r.id.contains("sustain")
            || r.id.contains("report14_on")
    });
    let stop = orgasm_rows
        .iter()
        .find(|r| r.id.contains("stop") && r.sent.starts_with("A0 06"));

    if arm.is_none() && sustain.is_none() && stop.is_none() {
        return;
    }

    out.push_str("**Orgasm (3 steps):**\n\n");
    out.push_str("| Step | Bytes | Meaning |\n| --- | --- | --- |\n");
    if let Some(a) = arm {
        if a.sent.contains("01") {
            out.push_str("| Arm | `01` | 1-byte prime before sustain |\n");
        }
    }
    if let Some(s) = sustain {
        let hx = hex_label_from_sent(&s.sent);
        if hx.starts_with("A0 03") {
            out.push_str(&format!(
                "| Sustain | `{hx}` | Same motor frame, all max — repeat ~10 Hz |\n",
            ));
        }
    }
    if let Some(s) = stop {
        let hx = hex_label_from_sent(&s.sent);
        out.push_str(&format!("| Stop | `{hx}` | End orgasm mode |\n"));
    }
    out.push('\n');
}

fn render_cautions(
    out: &mut String,
    merged: &VerifySummary,
    write_target: Option<&str>,
) {
    let mut bullets: Vec<String> = Vec::new();

    if let Some(target) = write_target {
        if target.to_ascii_uppercase().contains("AE3B") {
            bullets.push("Use **AE3B** for writes (not AE01). Notify on AE3C.".into());
        }
    }

    let motor_stop = merged.success_rows.iter().any(|r| {
        r.sent.eq_ignore_ascii_case("A0 03 00 00 00") && r.id.contains("stop")
    });
    let orgasm_stop = merged
        .success_rows
        .iter()
        .any(|r| r.sent.starts_with("A0 06") && r.id.contains("orgasm"));
    if motor_stop && orgasm_stop {
        bullets.push(
            "`A0 03 00 00 00` stops motors but **not** orgasm mode — use `A0 06 00 00 00`."
                .into(),
        );
    }

    for row in &merged.error_rows {
        if row.id.contains("orgasm") || row.sent.starts_with("A0 05") {
            bullets.push(format!("`{}` does **not** work on this device.", row.sent));
        }
        if row.id.contains("ae01") {
            bullets.push("AE01 fallback failed — use the primary write channel.".into());
        }
    }

    bullets.push("Orgasm without `01` + ~10 Hz repeat behaves like a normal motor hold.".into());
    bullets.push("Disconnect the official app before sending commands.".into());

    bullets.sort();
    bullets.dedup();

    out.push_str("## Cautions\n\n");
    for b in bullets {
        out.push_str(&format!("- {b}\n"));
    }
}

pub fn write_target_from_channel(channel: &str) -> String {
    match channel.to_ascii_lowercase().as_str() {
        "ae3b" => "AE3B (`0x0045`)".into(),
        "ae01" => "AE01".into(),
        "ffe1" => "FFE1".into(),
        other => other.to_uppercase(),
    }
}

fn load_plan_channel(workdir: &std::path::Path) -> Option<String> {
    for name in ["verify_plan.json", "verify_plan_extended.json"] {
        let path = workdir.join(name);
        if !path.exists() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(ch) = v.get("channel").and_then(|c| c.as_str()) {
                return Some(ch.to_string());
            }
        }
    }
    None
}

pub fn render_findings_for_workdir(
    brand: &str,
    product: &str,
    summaries: &[VerifySummary],
    sweep_md: Option<&str>,
    workdir: &std::path::Path,
) -> String {
    let write_target = load_plan_channel(workdir).map(|c| write_target_from_channel(&c));
    render_findings_strict(
        brand,
        product,
        summaries,
        sweep_md,
        write_target.as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn project_pipeline_covers_findings_level() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap();
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
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap();
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
