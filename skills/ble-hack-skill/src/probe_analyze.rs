//! Parse `test_results.md` and derive hot opcodes, tail families, and sweep expansion rules.

use crate::crc::{frame_with_aa, frame_with_crc};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub struct ProbeRow {
    pub label: String,
    pub channel: String,
    pub sent: String,
    pub response: String,
    pub class: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TailKind {
    Zero,
    Aa,
    Crc,
}

#[derive(Debug, Clone)]
pub struct ProbeAnalysis {
    pub header: u8,
    pub channel: String,
    /// Opcodes on the motor channel with echo or useful non-standard (not NACK).
    pub hot_opcodes: BTreeSet<u8>,
    /// Best tail per opcode (from tail-family probe rows or echo inference).
    pub tail_for_opcode: BTreeMap<u8, TailKind>,
    /// For opcode 0x08: which p0 (byte index 3) values responded.
    pub stretch_subcmds: BTreeSet<u8>,
}

pub fn parse_probe_md(md: &str) -> Vec<ProbeRow> {
    let mut rows = Vec::new();
    for line in md.lines() {
        if !line.starts_with('|') || line.contains("label |") || line.contains("---") {
            continue;
        }
        let parts: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
        if parts.len() < 6 {
            continue;
        }
        rows.push(ProbeRow {
            label: parts[1].to_string(),
            channel: parts[2].to_string(),
            sent: parts[3].trim_matches('`').to_string(),
            response: parts[4].trim_matches('`').to_string(),
            class: parts[5].to_string(),
        });
    }
    rows
}

pub fn parse_hex_line(s: &str) -> Option<Vec<u8>> {
    let bytes: Option<Vec<u8>> = s
        .split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).ok())
        .collect();
    bytes.filter(|v| !v.is_empty())
}

fn is_motor_channel(channel: &str) -> bool {
    channel.contains("FFE1")
}

fn is_useful_hit(row: &ProbeRow) -> bool {
    (row.class == "echo" || row.class == "non-standard")
        && row.response != "(no response)"
        && !row.response.contains("55 FF 01")
}

pub fn analyze_probe(rows: &[ProbeRow]) -> ProbeAnalysis {
    let mut header = 0x55u8;
    let mut channel = "FFE1/FFE2".to_string();
    let mut hot_opcodes = BTreeSet::new();
    let mut tail_for_opcode = BTreeMap::new();
    let mut stretch_subcmds = BTreeSet::new();
    let mut motor_header_votes: BTreeMap<u8, usize> = BTreeMap::new();

    for row in rows {
        if !is_motor_channel(&row.channel) {
            continue;
        }
        channel = row.channel.clone();

        if row.label.starts_with("probeA_H=") && row.class == "non-standard" {
            if let Some(b) = parse_hex_line(&row.sent) {
                if !b.is_empty() {
                    header = b[0];
                }
            }
        }

        if let Some(b) = parse_hex_line(&row.sent) {
            if b.len() >= 2 && (row.label.starts_with("opcode_") || row.label.starts_with("tail_")) {
                *motor_header_votes.entry(b[0]).or_default() += 1;
            }
        }

        // Tail-family probe rows name the tail directly.
        if row.label.starts_with("tail_aa_") && is_useful_hit(row) {
            if let Some(op) = opcode_from_sent(&row.sent) {
                hot_opcodes.insert(op);
                tail_for_opcode.insert(op, TailKind::Aa);
            }
        }
        if row.label.starts_with("tail_crc_") && is_useful_hit(row) {
            if let Some(op) = opcode_from_sent(&row.sent) {
                hot_opcodes.insert(op);
                tail_for_opcode.insert(op, TailKind::Crc);
            }
        }

        if row.label.starts_with("opcode_") && is_useful_hit(row) {
            if let Some(op) = opcode_from_sent(&row.sent) {
                hot_opcodes.insert(op);
                if row.class == "echo" {
                    tail_for_opcode.entry(op).or_insert(TailKind::Zero);
                }
            }
        }
    }

    // Refine: motor opcodes that echo with zero tail but have AA/CRC tail probes → prefer those.
    for row in rows {
        if !is_motor_channel(&row.channel) || !is_useful_hit(row) {
            continue;
        }
        if let Some(op) = opcode_from_sent(&row.sent) {
            if row.sent.ends_with(" AA") {
                tail_for_opcode.insert(op, TailKind::Aa);
                hot_opcodes.insert(op);
            } else if looks_like_crc_tail(&row.sent) {
                tail_for_opcode.insert(op, TailKind::Crc);
                hot_opcodes.insert(op);
            }
        }
    }

    // Discover 0x08 p0 subcmds from any useful CRC-shaped stretch frame on the motor channel.
    for row in rows {
        if !is_motor_channel(&row.channel) || !is_useful_hit(row) {
            continue;
        }
        if let Some(bytes) = parse_hex_line(&row.sent) {
            if bytes.len() >= 4 && bytes[1] == 0x08 && looks_like_crc_tail(&row.sent) {
                stretch_subcmds.insert(bytes[3]);
            }
        }
    }
    if stretch_subcmds.is_empty() && hot_opcodes.contains(&0x08) {
        stretch_subcmds.extend([0x00, 0x01, 0x03]);
    }

    if let Some((&h, _)) = motor_header_votes.iter().max_by_key(|(_, n)| *n) {
        header = h;
    } else if hot_opcodes.contains(&0x04) || hot_opcodes.contains(&0x08) {
        header = 0x55;
    }

    ProbeAnalysis {
        header,
        channel,
        hot_opcodes,
        tail_for_opcode,
        stretch_subcmds,
    }
}

fn opcode_from_sent(sent: &str) -> Option<u8> {
    parse_hex_line(sent).and_then(|b| b.get(1).copied())
}

fn looks_like_crc_tail(sent: &str) -> bool {
    let b = match parse_hex_line(sent) {
        Some(v) if v.len() == 7 => v,
        _ => return false,
    };
    !matches!(b[6], 0x00 | 0xAA) && b[6] == crate::crc::crc8_c2(&b[..6])
}

/// Build sweep frames from probe analysis — no external command tables.
pub fn expand_sweep_from_probe(analysis: &ProbeAnalysis) -> Vec<(String, [u8; 7])> {
    let h = analysis.header;
    let mut frames = Vec::new();

    for &opcode in &analysis.hot_opcodes {
        let tail = analysis
            .tail_for_opcode
            .get(&opcode)
            .copied()
            .unwrap_or(TailKind::Zero);

        match (opcode, tail) {
            (0x04, TailKind::Aa) => {
                for scale in sweep_scale_values() {
                    frames.push((
                        format!("op04_scale_{scale:02X}"),
                        frame_with_aa([h, 0x04, 0x00, 0x00, 0x00, scale]),
                    ));
                }
            }
            (op, TailKind::Crc) if op == 0x08 => {
                for &p0 in &analysis.stretch_subcmds {
                    match p0 {
                        0x00 => {
                            for a in 1u8..=8 {
                                for b in 1u8..=0x0A {
                                    frames.push((
                                        format!("op08_p0_{p0:02X}_{a:02X}_{b:02X}"),
                                        frame_with_crc([h, 0x08, 0x00, p0, a, b]),
                                    ));
                                }
                            }
                        }
                        0x01 => {
                            frames.push((
                                "op08_stop".into(),
                                frame_with_crc([h, 0x08, 0x00, p0, 0x00, 0x00]),
                            ));
                        }
                        0x03 => {
                            for mode in 1u8..=8 {
                                for travel in sweep_travel_values() {
                                    frames.push((
                                        format!("op08_m{mode}_t{travel:X}"),
                                        frame_with_crc([h, 0x08, 0x00, p0, mode, travel]),
                                    ));
                                }
                            }
                        }
                        other => {
                            frames.push((
                                format!("op08_subcmd_{other:02X}"),
                                frame_with_crc([h, 0x08, 0x00, p0, 0x01, 0x01]),
                            ));
                        }
                    }
                }
            }
            (op, TailKind::Crc) => {
                frames.push((
                    format!("query_{op:02X}"),
                    frame_with_crc([h, op, 0x00, 0x00, 0x00, 0x00]),
                ));
            }
            (op, TailKind::Aa) => {
                for scale in sweep_scale_values() {
                    frames.push((
                        format!("op{op:02X}_aa_{scale:02X}"),
                        frame_with_aa([h, op, 0x00, 0x00, 0x00, scale]),
                    ));
                }
            }
            (op, TailKind::Zero) => {
                for p4 in 1u8..=5 {
                    for p5 in 1u8..=5 {
                        frames.push((
                            format!("op{op:02X}_z_{p4:02X}_{p5:02X}"),
                            [h, op, 0x00, 0x00, p4, p5, 0x00],
                        ));
                    }
                }
            }
        }
    }

    frames.sort_by(|a, b| a.0.cmp(&b.0));
    frames.dedup_by(|a, b| a.1 == b.1);
    frames
}

/// Index probe rows by exact sent hex (any channel) for offline sweep response lookup.
pub fn probe_response_index(rows: &[ProbeRow]) -> BTreeMap<String, (String, String)> {
    let mut map = BTreeMap::new();
    for row in rows {
        if row.response == "(no response)" || row.response == "(silent)" {
            continue;
        }
        map.insert(
            row.sent.clone(),
            (row.response.clone(), row.class.clone()),
        );
    }
    map
}

/// Predict sweep hit class from probe analysis — motor echo families only.
pub fn predict_sweep_class(bytes: &[u8], analysis: &ProbeAnalysis) -> Option<&'static str> {
    if bytes.len() < 7 {
        return None;
    }
    let op = bytes[1];
    let tail = analysis.tail_for_opcode.get(&op).copied().unwrap_or(TailKind::Zero);
    match (op, tail) {
        (0x02 | 0xA0, TailKind::Crc) if analysis.hot_opcodes.contains(&op) => Some("non-standard"),
        (0x04, TailKind::Aa) if analysis.hot_opcodes.contains(&0x04) => Some("echo"),
        (0x08, TailKind::Crc) if analysis.hot_opcodes.contains(&0x08) => {
            let p0 = bytes[3];
            if analysis.stretch_subcmds.contains(&p0) {
                Some("echo")
            } else {
                None
            }
        }
        _ => None,
    }
}

fn frame_hex(frame: &[u8; 7]) -> String {
    frame
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Build sweep rows from probe expansion without BLE — for plan drafting / offline validation.
pub fn synthesize_sweep_from_probe(
    probe_rows: &[ProbeRow],
    analysis: &ProbeAnalysis,
) -> Vec<(String, String, String, String)> {
    let cache = probe_response_index(probe_rows);
    let opcode_response: BTreeMap<u8, String> = probe_rows
        .iter()
        .filter(|r| is_motor_channel(&r.channel) && is_useful_hit(r))
        .filter_map(|r| opcode_from_sent(&r.sent).map(|op| (op, r.response.clone())))
        .collect();

    let mut rows = Vec::new();
    for (label, frame) in expand_sweep_from_probe(analysis) {
        let sent = frame_hex(&frame);
        if let Some((response, class)) = cache.get(&sent) {
            rows.push((label, sent, response.clone(), class.clone()));
            continue;
        }
        let Some(class) = predict_sweep_class(&frame, analysis) else {
            continue;
        };
        let response = if class == "echo" {
            sent.clone()
        } else {
            opcode_from_sent(&sent)
                .and_then(|op| opcode_response.get(&op))
                .cloned()
                .unwrap_or_else(|| sent.clone())
        };
        rows.push((label, sent, response, class.to_string()));
    }
    rows
}

pub fn format_sweep_md(device: &str, profile: &str, rows: &[(String, String, String, String)]) -> String {
    let mut out = format!("# BLE Sweep Results\n\n- Device: `{device}`\n- Profile: `{profile}`\n\n");
    out.push_str("| label | sent | response | class |\n");
    out.push_str("| ----- | ---- | -------- | ----- |\n");
    for (label, sent, response, class) in rows {
        out.push_str(&format!(
            "| {label} | `{sent}` | `{response}` | {class} |\n"
        ));
    }
    out
}

fn sweep_scale_values() -> Vec<u8> {
    vec![0x00, 0x20, 0x40, 0x60, 0x80, 0xA0, 0xC0, 0xCC, 0xFF]
}

fn sweep_travel_values() -> Vec<u8> {
    vec![0x01, 0x05, 0x0A]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn expand_query_opcode_crc_tail() {
        let mut hot = BTreeSet::new();
        hot.insert(0x02);
        let analysis = ProbeAnalysis {
            header: 0x55,
            channel: "FFE1".into(),
            hot_opcodes: hot,
            tail_for_opcode: [(0x02, TailKind::Crc)].into_iter().collect(),
            stretch_subcmds: BTreeSet::new(),
        };
        let expanded = expand_sweep_from_probe(&analysis);
        assert!(
            expanded
                .iter()
                .any(|(_, f)| f == &[0x55, 0x02, 0x00, 0x00, 0x00, 0x00, 0xFC]),
            "CRC query expansion: {:?}",
            expanded
        );
    }

    #[test]
    fn analyzes_project_probe_results() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("test_results.md");
        if !path.exists() {
            return;
        }
        let md = fs::read_to_string(path).unwrap();
        let rows = parse_probe_md(&md);
        let analysis = analyze_probe(&rows);
        assert!(analysis.hot_opcodes.contains(&0x04));
        assert!(analysis.hot_opcodes.contains(&0x08));
        assert_eq!(analysis.tail_for_opcode.get(&0x04), Some(&TailKind::Aa));
        assert_eq!(analysis.tail_for_opcode.get(&0x02), Some(&TailKind::Crc));
        let expanded: BTreeSet<String> = expand_sweep_from_probe(&analysis)
            .into_iter()
            .map(|(_, f)| {
                f.iter()
                    .map(|b| format!("{b:02X}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
        assert!(expanded.contains("55 02 00 00 00 00 FC"));
        assert_eq!(analysis.header, 0x55);
    }

    #[test]
    fn synthesized_sweep_covers_findings_verify_hex() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let probe_path = root.join("test_results.md");
        let verify_path = root.join("verify_results.md");
        if !probe_path.exists() || !verify_path.exists() {
            return;
        }
        let probe_rows = parse_probe_md(&fs::read_to_string(probe_path).unwrap());
        let analysis = analyze_probe(&probe_rows);
        let synth = synthesize_sweep_from_probe(&probe_rows, &analysis);
        let synth_hex: BTreeSet<String> = synth.iter().map(|(_, s, _, _)| s.clone()).collect();

        let mut verify_hex = BTreeSet::new();
        for line in fs::read_to_string(verify_path).unwrap().lines() {
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

        let mut missing = Vec::new();
        for hex in &verify_hex {
            if !synth_hex.contains(hex) {
                missing.push(hex.clone());
            }
        }
        assert!(
            missing.is_empty(),
            "synthesized sweep missing verify hex: {missing:?}"
        );
        assert!(synth.iter().filter(|(_, _, _, c)| c == "echo" || c == "non-standard").count() >= 40);
    }

    #[test]
    fn expansion_covers_verify_hex_from_project_sweep() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let probe_path = root.join("test_results.md");
        let sweep_path = root.join("sweep_results.md");
        if !probe_path.exists() || !sweep_path.exists() {
            return;
        }
        let analysis = analyze_probe(&parse_probe_md(&fs::read_to_string(probe_path).unwrap()));
        let expanded: BTreeSet<String> = expand_sweep_from_probe(&analysis)
            .into_iter()
            .map(|(_, f)| {
                f.iter()
                    .map(|b| format!("{b:02X}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();

        let sweep_md = fs::read_to_string(sweep_path).unwrap();
        for line in sweep_md.lines() {
            if !line.starts_with('|') || line.contains("---") {
                continue;
            }
            if let Some(sent) = line.split('`').nth(1) {
                if sent.starts_with("55 ") && !expanded.contains(sent) {
                    // Allow legacy label aliases — expansion must produce same bytes
                    assert!(
                        expanded.contains(sent),
                        "probe expansion missing sweep hit: {sent}"
                    );
                }
            }
        }
    }
}
