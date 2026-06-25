# ble-hack-skill

Discovers a BLE device’s command protocol by scanning, probing, and sweeping candidate frames, then builds `FINDINGS.md` from commands you confirm on hardware. Full workflow: `SKILL.md`.

## What you provide

- **Brand and product name** (Bluetooth local name)
- **Device powered on**, official app disconnected
- **Terminal with Bluetooth** (macOS: outside sandbox)
- **Your eyes at verify time** — **y** / **n** / **r** / **q** at each checkpoint

Run from the project root (where `FINDINGS.md` will live).

## What to run

```bash
cargo run -p ble-hack-skill --bin ble_run -- --brand YOUR_BRAND --product YOUR_PRODUCT --workdir .
```

Stay at the device when `ble_verify` asks. Done when `ble_check` prints `Ready for FINDINGS: true`.

If verify was skipped:

```bash
cargo run -p ble-hack-skill --bin ble_verify -- --workdir .
cargo run -p ble-hack-skill --bin ble_check -- --workdir . --brand "YOUR_BRAND" --product "YOUR_PRODUCT"
```

Session artifacts (`scan_results.md`, `test_results.md`, etc.) and `FINDINGS.md` belong in the project root, not inside `ble-hack-skill/`. See `SKILL.md`.
