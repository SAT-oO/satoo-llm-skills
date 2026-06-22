//! Scan nearby BLE devices, rank by manufacturer, surface protocol-relevant candidates.
//!
//! Usage (from `ble-hack-skill/`):
//!   cargo run --bin ble_scan
//!   cargo run --bin ble_scan -- --brand Svakom --product Klitty
//!   cargo run --bin ble_scan -- --discover
//!   cargo run --bin ble_scan -- --seconds 5 --output scan_results.md

use anyhow::{Context, Result};
use ble_hack_skill::manufacturers::{self, OemClass};
use btleplug::api::{bleuuid::uuid_from_u16, Central, CharPropFlags, Manager, Peripheral, ScanFilter};
use btleplug::platform::Manager as BluetoothManager;
use std::collections::HashMap;
use std::fs;
use std::time::Duration;
use tokio::time;
use uuid::Uuid;

const UART_SERVICE: Uuid = uuid_from_u16(0xFFE0);
const UART_RX: Uuid = uuid_from_u16(0xFFE1);
const UART_TX: Uuid = uuid_from_u16(0xFFE2);

struct ScannedDevice {
    id: String,
    address: String,
    local_name: Option<String>,
    rssi: Option<i16>,
    company_id: Option<u16>,
    manufacturer_hex: String,
    service_uuids: Vec<Uuid>,
    score: i32,
    tier: &'static str,
    brand_match: bool,
    notes: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let seconds = arg_value(&args, "--seconds")
        .and_then(|s| s.parse().ok())
        .unwrap_or(3);
    let brand = arg_value(&args, "--brand");
    let product = arg_value(&args, "--product");
    let discover = args.iter().any(|a| a == "--discover");
    let output = arg_value(&args, "--output");

    let manager = BluetoothManager::new().await?;
    let adapter = manager
        .adapters()
        .await?
        .into_iter()
        .next()
        .context("No Bluetooth adapters found")?;

    println!("Scanning for {seconds}s...\n");
    adapter.start_scan(ScanFilter::default()).await?;
    time::sleep(Duration::from_secs(seconds)).await;
    adapter.stop_scan().await?;

    let peripherals = adapter.peripherals().await?;
    let mut devices = Vec::new();

    for peripheral in peripherals {
        let props = peripheral.properties().await?.unwrap_or_default();
        let company_id = props.manufacturer_data.keys().copied().next();
        let manufacturer_hex = props
            .manufacturer_data
            .iter()
            .map(|(id, data)| {
                format!(
                    "0x{id:04X} ({})",
                    data.iter()
                        .map(|b| format!("{b:02X}"))
                        .collect::<Vec<_>>()
                        .join("")
                )
            })
            .collect::<Vec<_>>()
            .join("; ");

        let (score, tier, notes) = score_device(
            company_id,
            props.local_name.as_deref(),
            &props.services,
            brand.as_deref(),
            product.as_deref(),
        );

        let brand_match = brand.as_ref().is_some_and(|b| {
            manufacturers::name_matches(b, product.as_deref(), props.local_name.as_deref())
        });

        devices.push(ScannedDevice {
            id: peripheral.id().to_string(),
            address: peripheral.address().to_string(),
            local_name: props.local_name.clone(),
            rssi: props.rssi,
            company_id,
            manufacturer_hex: if manufacturer_hex.is_empty() {
                "(none)".into()
            } else {
                manufacturer_hex
            },
            service_uuids: props.services.clone(),
            score,
            tier,
            brand_match,
            notes,
        });
    }

    devices.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| b.brand_match.cmp(&a.brand_match))
            .then_with(|| {
                let ar = a.rssi.unwrap_or(i16::MIN);
                let br = b.rssi.unwrap_or(i16::MIN);
                br.cmp(&ar)
            })
    });

    print_report(&devices, brand.as_deref(), product.as_deref());

    if discover {
        if let Some(top) = devices.first().filter(|d| d.tier != "SKIP") {
            println!("\n--- GATT discovery: {} ---", top.local_name.as_deref().unwrap_or("?"));
            discover_gatt(&adapter, top).await?;
        } else {
            println!("\nNo non-skipped candidate for GATT discovery.");
        }
    }

    if let Some(path) = output {
        fs::write(&path, format_report_md(&devices, brand.as_deref(), product.as_deref()))?;
        println!("\nWrote {path}");
    }

    Ok(())
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

fn has_uart_services(services: &[Uuid]) -> bool {
    services.iter().any(|u| *u == UART_SERVICE || *u == UART_RX || *u == UART_TX)
}

fn score_device(
    company_id: Option<u16>,
    local_name: Option<&str>,
    services: &[Uuid],
    brand: Option<&str>,
    product: Option<&str>,
) -> (i32, &'static str, Vec<String>) {
    let mut score = 0i32;
    let mut notes = Vec::new();

    match manufacturers::classify(company_id) {
        OemClass::MajorConsumer => {
            score -= 100;
            notes.push(format!(
                "major OEM: {}",
                manufacturers::company_name(company_id.unwrap_or(0))
            ));
        }
        OemClass::NicheProduct { brand: b } => {
            score += 60;
            notes.push(format!("known niche OEM: {b}"));
        }
        OemClass::Unknown => {
            score += 40;
            if let Some(id) = company_id {
                notes.push(format!(
                    "non-major manufacturer 0x{id:04X} — verify on web"
                ));
            }
        }
        OemClass::NoData => {
            score += 10;
            notes.push("no manufacturer data in advertisement".into());
        }
    }

    if has_uart_services(services) {
        score += 50;
        notes.push("advertises UART-style service (FFE0/FFE1/FFE2)".into());
    }

    if let Some(b) = brand {
        if manufacturers::name_matches(b, product, local_name) {
            score += 80;
            notes.push(format!("name matches --brand/--product ({b})"));
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

    (score, tier, notes)
}

fn print_report(devices: &[ScannedDevice], brand: Option<&str>, product: Option<&str>) {
    println!("=== BLE scan report ===");
    if let Some(b) = brand {
        println!("Filter brand: {b}");
    }
    if let Some(p) = product {
        println!("Filter product: {p}");
    }
    println!("Devices found: {}\n", devices.len());

    println!(
        "{:<6} {:<36} {:<20} {:<10} {:<8} {}",
        "TIER", "DEVICE_ID", "NAME", "MFG_ID", "RSSI", "NOTES"
    );
    println!("{}", "-".repeat(120));

    for d in devices {
        let mfg = d
            .company_id
            .map(|id| format!("0x{id:04X}"))
            .unwrap_or_else(|| "—".into());
        let name = d.local_name.as_deref().unwrap_or("—");
        let rssi = d
            .rssi
            .map(|r| format!("{r}"))
            .unwrap_or_else(|| "—".into());
        let notes = d.notes.join("; ");
        println!(
            "{:<6} {:<36} {:<20} {:<10} {:<8} {}",
            d.tier, d.id, name, mfg, rssi, notes
        );
    }

    if let Some(top) = devices.iter().find(|d| d.tier == "PRIMARY" || d.tier == "CANDIDATE") {
        println!("\n=== Recommended target ===");
        println!("Device ID:  {}", top.id);
        println!("Name:       {}", top.local_name.as_deref().unwrap_or("—"));
        println!("Address:    {}", top.address);
        println!(
            "Manufacturer data: {}",
            if top.manufacturer_hex.is_empty() {
                "(none)".to_string()
            } else {
                top.manufacturer_hex.clone()
            }
        );
        if let Some(id) = top.company_id {
            println!(
                "Manufacturer: 0x{id:04X} ({})",
                manufacturers::company_name(id)
            );
            println!("  → Verify on web that this company makes the product category.");
        }
        if !top.service_uuids.is_empty() {
            println!("Advertised services:");
            for u in &top.service_uuids {
                let short = u.to_string();
                let label = if *u == UART_SERVICE {
                    " (UART service)"
                } else if *u == UART_RX {
                    " (UART Rx)"
                } else if *u == UART_TX {
                    " (UART Tx)"
                } else {
                    ""
                };
                println!("  {short}{label}");
            }
        }
        println!("\nRe-run with --discover to connect and enumerate full GATT table.");
    } else {
        println!("\nNo strong candidate found. Use core-loop protocol discovery (see SKILLS.md).");
    }
}

fn format_report_md(devices: &[ScannedDevice], brand: Option<&str>, product: Option<&str>) -> String {
    let mut out = String::from("# BLE Scan Results\n\n");
    if let Some(b) = brand {
        out.push_str(&format!("- Brand filter: `{b}`\n"));
    }
    if let Some(p) = product {
        out.push_str(&format!("- Product filter: `{p}`\n"));
    }
    out.push_str(&format!("- Devices found: {}\n\n", devices.len()));

    out.push_str("| tier | device_id | address | name | mfg_id | mfg_data | mfg_name | rssi | services | notes |\n");
    out.push_str("| ---- | --------- | ------- | ---- | ------ | -------- | -------- | ---- | -------- | ----- |\n");

    for d in devices {
        let mfg_id = d
            .company_id
            .map(|id| format!("`0x{id:04X}`"))
            .unwrap_or_else(|| "—".into());
        let mfg_name = d
            .company_id
            .map(manufacturers::company_name)
            .unwrap_or("—");
        let mfg_data = if d.manufacturer_hex.is_empty() {
            "—".to_string()
        } else {
            d.manufacturer_hex.clone()
        };
        let services: String = d
            .service_uuids
            .iter()
            .map(|u| format!("`{u}`"))
            .collect::<Vec<_>>()
            .join(", ");
        let name = d.local_name.as_deref().unwrap_or("—");
        let rssi = d.rssi.map(|r| r.to_string()).unwrap_or_else(|| "—".into());
        let notes = d.notes.join("; ");
        out.push_str(&format!(
            "| {} | `{}` | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            d.tier,
            d.id,
            d.address,
            name,
            mfg_id,
            mfg_data,
            mfg_name,
            rssi,
            if services.is_empty() { "—".into() } else { services },
            notes
        ));
    }

    out
}

async fn discover_gatt(adapter: &btleplug::platform::Adapter, device: &ScannedDevice) -> Result<()> {
    let peripheral = adapter
        .peripherals()
        .await?
        .into_iter()
        .find(|p| p.id().to_string() == device.id)
        .context("Peripheral vanished after scan")?;

    peripheral.connect().await?;
    peripheral.discover_services().await?;

    println!("Connected. GATT characteristics:\n");

    let mut by_service: HashMap<Uuid, Vec<_>> = HashMap::new();
    for c in peripheral.characteristics() {
        by_service.entry(c.service_uuid).or_default().push(c);
    }

    for (service, chars) in by_service {
        println!("\nService {service}");
        for c in chars {
            let props = format!(
                "{}{}{}{}",
                if c.properties.contains(CharPropFlags::READ) {
                    "R"
                } else {
                    "-"
                },
                if c.properties.contains(CharPropFlags::WRITE) {
                    "W"
                } else {
                    "-"
                },
                if c.properties.contains(CharPropFlags::WRITE_WITHOUT_RESPONSE) {
                    "w"
                } else {
                    "-"
                },
                if c.properties.contains(CharPropFlags::NOTIFY)
                    || c.properties.contains(CharPropFlags::INDICATE)
                {
                    "N"
                } else {
                    "-"
                },
            );
            let tag = if c.uuid == UART_RX {
                " ← likely Rx (write)"
            } else if c.uuid == UART_TX {
                " ← likely Tx (notify)"
            } else {
                ""
            };
            println!("  {}{}", c.uuid, tag);
            println!("         props: {props}");
        }
    }

    peripheral.disconnect().await?;
    Ok(())
}
