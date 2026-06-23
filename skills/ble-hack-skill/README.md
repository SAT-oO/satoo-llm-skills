# ble-hack-skill

Reverse-engineer BLE device protocols. Full spec: `SKILL.md`. Output: `FINDINGS.md` (verified commands only).

## Workflow

Device powered on. Disconnect official app. Run from `ble-hack-skill/` (needs Bluetooth — not sandboxed).

**1. Find device**
```bash
cargo run --bin ble_scan -- --brand BRAND --product NAME --discover --output scan_results.md
```
Check: target name matches, tier PRIMARY/CANDIDATE, note UUID + `FFE1`/`FFE2`.

**2. Research before sweeps**
Search buttplug, GitHub, official app name. Check for shared OEM stack (e.g. KooSync = Svakom). Note likely tail bytes (CRC vs `AA` vs `00`).

**3. Probe candidates**
```bash
cargo run --bin ble_probe -- --device UUID --auto --output test_results.md
```
Check: which channel responds, header byte, opcodes that ack/echo. **Do not treat echo as verified.**

**4. Sweep parameters** (if multiple tail families or unclear bytes)
```bash
cargo run --bin ble_sweep -- --device UUID --output sweep_results.md
```
Check: test CRC-8 C2 (`src/crc.rs`), fixed `AA`, and zero-tail separately per opcode.

**5. Draft verification plan**
Copy `verify_plan.example.json` → `verify_plan.json`. Add ≤15 checkpoints — one per command family (Boost, stretch, M-mode, stop, battery query). Each `expect` field describes **physical movement** to watch for.

**6. Human gate — required**
```bash
cargo run --bin ble_verify -- --device UUID --plan verify_plan.json --output verify_results.md
```
User at device. At each checkpoint: observe movement. Press **y** if correct, **n** if wrong/none, **r** to replay, **q** to quit.

**7. Write FINDINGS.md**
Copy **success** rows from `verify_results.md` into `FINDINGS.md` using `FINDINGS.template.md`. Skip everything that got **n** or never passed Step 6.

## Tools

| Binary | Purpose |
| ------ | ------- |
| `ble_scan` | Scan, rank, GATT discovery |
| `ble_probe` | Header/opcode probes, `--auto` |
| `ble_sweep` | Parameter grid (CRC / AA / zero-tail) |
| `ble_verify` | Interactive movement confirmation |

## Do not

- Write FINDINGS.md from probe output alone
- Treat Tx echo as motor proof
- Assume one tail byte for all opcodes
- Treat status/battery byte changes as movement
- Skip `ble_verify`

See **Anti-patterns** in `SKILL.md` for why.
