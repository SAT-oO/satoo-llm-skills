//! Shared one-go pipeline helpers for scan → probe → verify gating.

use crate::manufacturers;
use btleplug::api::{bleuuid::uuid_from_u16, Central, Manager, Peripheral, ScanFilter};
use btleplug::platform::{Adapter, Manager as BluetoothManager};
use std::time::Duration;
use tokio::time;
use uuid::Uuid;

const UART_SERVICE: Uuid = uuid_from_u16(0xFFE0);
const UART_RX: Uuid = uuid_from_u16(0xFFE1);
const UART_TX: Uuid = uuid_from_u16(0xFFE2);

#[derive(Debug, Clone)]
pub struct ScannedDevice {
    pub id: String,
    pub local_name: Option<String>,
    pub rssi: Option<i16>,
    pub tier: &'static str,
    pub brand_match: bool,
    pub score: i32,
    pub has_uart_adv: bool,
}

pub async fn adapter() -> anyhow::Result<Adapter> {
    let manager = BluetoothManager::new().await?;
    manager
        .adapters()
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No Bluetooth adapters found"))
}

pub async fn scan(
    adapter: &Adapter,
    seconds: u64,
    brand: Option<&str>,
    product: Option<&str>,
) -> anyhow::Result<Vec<ScannedDevice>> {
    adapter.start_scan(ScanFilter::default()).await?;
    time::sleep(Duration::from_secs(seconds)).await;
    adapter.stop_scan().await?;

    let mut devices = Vec::new();
    for peripheral in adapter.peripherals().await? {
        let props = peripheral.properties().await?.unwrap_or_default();
        let company_id = props.manufacturer_data.keys().copied().next();
        let has_uart_adv = props
            .services
            .iter()
            .any(|u| *u == UART_SERVICE || *u == UART_RX || *u == UART_TX);
        let (score, tier, _) = score_device(
            company_id,
            props.local_name.as_deref(),
            &props.services,
            brand,
            product,
        );
        let brand_match = brand.is_some_and(|b| {
            manufacturers::name_matches(b, product, props.local_name.as_deref())
        });
        devices.push(ScannedDevice {
            id: peripheral.id().to_string(),
            local_name: props.local_name.clone(),
            rssi: props.rssi,
            tier,
            brand_match,
            score,
            has_uart_adv,
        });
    }

    devices.sort_by(|a, b| {
        b.brand_match
            .cmp(&a.brand_match)
            .then_with(|| b.score.cmp(&a.score))
            .then_with(|| {
                let ar = a.rssi.unwrap_or(i16::MIN);
                let br = b.rssi.unwrap_or(i16::MIN);
                br.cmp(&ar)
            })
    });
    Ok(devices)
}

pub fn pick_target<'a>(
    devices: &'a [ScannedDevice],
    require_name_match: bool,
) -> Option<&'a ScannedDevice> {
    let viable: Vec<_> = devices
        .iter()
        .filter(|d| d.tier != "SKIP")
        .collect();
    if viable.is_empty() {
        return None;
    }
    if require_name_match {
        if let Some(d) = viable.iter().find(|d| d.brand_match) {
            return Some(d);
        }
        return None;
    }
    viable.first().copied()
}

/// Automated gate: known motor channel + non-silent response on command probes.
pub fn probe_passes_automation_gate(probe_md: &str) -> bool {
    let mut has_motor_channel = false;
    let mut motor_response = false;

    for line in probe_md.lines() {
        if !line.starts_with('|') || line.contains("label |") || line.contains("---") {
            continue;
        }
        let parts: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
        if parts.len() < 6 {
            continue;
        }
        let channel = parts[2];
        let sent = parts[3].trim_matches('`');
        let class = parts[5];
        let is_motor_channel = channel.contains("FFE1")
            || channel.contains("AE01")
            || channel.contains("ae01");
        if !is_motor_channel {
            continue;
        }
        has_motor_channel = true;
        if sent.starts_with("(listen)") || sent.starts_with("(read)") {
            continue;
        }
        if class == "echo" || class == "non-standard" {
            motor_response = true;
        }
    }

    has_motor_channel && motor_response
}

/// True when probe markdown shows likely motor families (opcode 0x04 AA and/or 0x08 CRC grids).
pub fn motor_families_in_probe(probe_md: &str) -> bool {
    let mut motor_aa = false;
    let mut motor_crc = false;

    for line in probe_md.lines() {
        if !line.starts_with('|') {
            continue;
        }
        let sent = line
            .split('`')
            .nth(1)
            .unwrap_or("")
            .to_uppercase();
        if sent.starts_with("55 04") && sent.ends_with("AA") {
            motor_aa = true;
        }
        if sent.starts_with("55 08 00 03") || sent.starts_with("55 08 00 00") {
            motor_crc = true;
        }
    }

    motor_aa || motor_crc
}

fn score_device(
    company_id: Option<u16>,
    local_name: Option<&str>,
    services: &[Uuid],
    brand: Option<&str>,
    product: Option<&str>,
) -> (i32, &'static str, Vec<String>) {
    use crate::manufacturers::OemClass;
    let mut score = 0i32;
    let mut notes = Vec::new();

    match manufacturers::classify(company_id) {
        OemClass::MajorConsumer => score -= 100,
        OemClass::NicheProduct { .. } => score += 60,
        OemClass::Unknown => score += 40,
        OemClass::NoData => score += 10,
    }

    if services
        .iter()
        .any(|u| *u == UART_SERVICE || *u == UART_RX || *u == UART_TX)
    {
        score += 50;
    }

    if let Some(b) = brand {
        if manufacturers::name_matches(b, product, local_name) {
            score += 80;
            notes.push(format!("name matches ({b})"));
        }
    }

    let tier = if score < 0 {
        "SKIP"
    } else if score >= 80 {
        "PRIMARY"
    } else if score >= 40 {
        "CANDIDATE"
    } else {
        "LOW"
    };
    let _ = notes;
    (score, tier, notes)
}
