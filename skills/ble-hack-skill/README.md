# ble-hack-skill

Discovers a BLE device’s command protocol by scanning, probing, and sweeping candidate frames, then builds `FINDINGS.md` from commands you confirm actually work on hardware. Full workflow: `SKILL.md`.

## What you provide

- **Brand and product name** (as shown in the device’s Bluetooth name)
- **Device powered on**, official app disconnected
- **Terminal with Bluetooth** (macOS: run outside sandbox)
- **Your eyes at verify time** — press **y** / **n** / **r** / **q** when each test command runs

Run from the project root (where `FINDINGS.md` will live).

## What to run

```bash
cargo run -p ble-hack-skill --bin ble_run -- --brand YOUR_BRAND --product YOUR_PRODUCT --workdir .
```

Stay at the device when `ble_verify` asks. Done when it prints `Ready for FINDINGS: true` — open `FINDINGS.md`.

If verify was skipped earlier:

```bash
cargo run -p ble-hack-skill --bin ble_verify -- --workdir .
cargo run -p ble-hack-skill --bin ble_check -- --workdir . --brand "YOUR_BRAND" --product "YOUR_PRODUCT"
```
