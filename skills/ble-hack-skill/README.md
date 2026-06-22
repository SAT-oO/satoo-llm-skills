# ble-hack-skill

Self-contained BLE protocol reverse-engineering skill. Copy this entire folder to any project.

| File | Role |
| ---- | ---- |
| `SKILL.md` | Full automated workflow + authoritative `FINDINGS.md` specification |
| `src/bin/ble_scan.rs` | Scan nearby BLE devices, rank by manufacturer, optional GATT discovery |
| `src/manufacturers.rs` | Bluetooth SIG company ID lookup and OEM classification |
| `Cargo.toml` | Rust dependencies (`btleplug`, `tokio`) |

## Quick start

```bash
cd ble-hack-skill

# Step 0 — automated device discovery (run first, always)
cargo run --bin ble_scan
cargo run --bin ble_scan -- --brand Svakom --product Klitty --discover --output scan_results.md
```

Protocol experiments and `FINDINGS.md` for a specific device live in the parent project (or wherever you are reverse-engineering).
