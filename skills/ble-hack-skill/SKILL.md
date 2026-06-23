# BLE Protocol Reverse-Engineering Skill

**Self-contained workflow.** An LLM can execute this document with no other project context. Copy the entire `ble-hack-skill/` folder anywhere; the only runtime dependency is a BLE-capable host with Rust (`btleplug` + `tokio`) or an equivalent BLE stack.

**Deliverable:** a living `FINDINGS.md` — precise, table-driven, actionable for the next human or LLM session.

**Do not assume a frame header** (e.g. `0x55`). That is one convention on some UART-style modules, not universal. Discover framing from automated scan, research, and live traffic.

---

## Purpose

This skill automates reverse-engineering of proprietary BLE peripheral protocols — **any brand, any product**, with emphasis on intimate-wellness / sex-tech devices that rarely publish specs.

The agent always:

1. **Scans** nearby BLE devices and ranks them by manufacturer heuristics.
2. **Selects** the most likely target (non–major-OEM, name/brand match, UART services).
3. **Researches** similar protocols (buttplug.io, web) before byte-sweeping.
4. **Discovers** frame layout, handshake, and control commands on hardware.
5. **Writes and maintains** `FINDINGS.md` per the specification in [FINDINGS.md Specification](#findingsmd-specification-authoritative) below.

User input (device address, product name, captures) **accelerates** the run but is **not required** to start.

---

## Automated Pipeline (Run in Order)

**Requires Bluetooth host permissions** (not sandbox). On macOS, run from a terminal with BLE access.

```
STEP 0  cargo run --bin ble_scan -- --brand X --product Y --discover --output scan_results.md
        → pick UUID; confirm FFE1/FFE2 on target
STEP 1  Research buttplug + GitHub + official app name (before byte sweeps)
        → note OEM stack, likely tail types (CRC / AA / 00)
STEP 2  cargo run --bin ble_probe -- --device UUID --auto --output test_results.md
        → note echo/ack opcodes; do NOT mark verified
STEP 3  cargo run --bin ble_sweep -- --device UUID --output sweep_results.md
        → if tails unclear: sweep CRC, AA, zero-tail per opcode
STEP 4  Write verify_plan.json from probe hits (≤15 checkpoints, all families + stops)
STEP 5  cargo run --bin ble_verify -- --device UUID --plan verify_plan.json
        → user observes movement; y/n/r/q each checkpoint
STEP 6  Write FINDINGS.md from verify_results.md success rows only
```

**One-go invocation:** `/ble-hack-skill` → run Steps 0–4, draft `verify_plan.json`, **stop for Step 6 with user present**, then Step 7.

**If Step 0 yields only SKIP / LOW devices**, fall back to the [Core Loop](#core-loop-fallback).

---

## Anti-patterns — do not repeat

These caused a wrong Jetpack protocol despite plausible BLE logs. **Hard rules:**

1. **Never write FINDINGS.md before `ble_verify` completes.** Probes produce candidates only.
2. **Never mark a command verified because Tx echoed it.** Wrong frames can echo. Confirm **physical movement** (y at checkpoint).
3. **Never assume one tail byte for all opcodes.** On KooSync/Svakom-style devices: test **fixed `AA`**, **CRC-8 C2** (`frame_with_crc`), and **fixed `00`** separately per opcode.
4. **Never skip opcode families.** If research names an official app, sweep its primary path first (e.g. Boost `0x04` before guessing `0x08`).
5. **Never map bytes from echo alone.** Validate byte semantics with isolated checkpoints (one family at a time).
6. **Never treat status/battery byte changes as motor proof.** Read `55 02 …` for SOC; dropping byte ≠ thrust confirmed.
7. **Never merge tail families in one verify checkpoint.** Boost (AA), stretch (CRC `p1=0x00`), M-mode (CRC `p1=0x03`) are separate rows in `verify_plan.json`.

When probe says **echo** → add to `verify_plan.json`, not FINDINGS.md.

---

## Step 0: Automated BLE Scan (`ble_scan`)

### Run

```bash
cd ble-hack-skill
cargo run --bin ble_scan
cargo run --bin ble_scan -- --brand Svakom --product Klitty
cargo run --bin ble_scan -- --discover
cargo run --bin ble_scan -- --seconds 5 --output scan_results.md
```

### What it does

| Action | Detail |
|--------|--------|
| Scan | Default 3 s; enumerates **every** nearby peripheral |
| Manufacturer decode | Reads Bluetooth SIG company ID from advertisement manufacturer data |
| Deprioritize major OEMs | Apple, Microsoft, Samsung, Google, Intel, Broadcom, Meta, Sony, Dell, HP, Bose, Amazon, Xiaomi, Huawei, Logitech, etc. — typical phones, laptops, watches |
| Prioritize candidates | Unknown manufacturer, niche OEM, UART service UUIDs (`FFE0`/`FFE1`/`FFE2`) |
| Brand/product match | Optional `--brand` / `--product` boost devices whose **local name** matches |
| Rank | `PRIMARY` ≥ 80, `CANDIDATE` ≥ 40, `LOW` &lt; 40, `SKIP` &lt; 0 (major OEM) |
| GATT discovery | `--discover` connects to top candidate and lists full service/characteristic UUIDs |
| Output | Terminal table + optional `scan_results.md` |

### Agent obligations after scan

1. Take the **recommended target** (highest tier, highest RSSI).
2. **Web-verify** the manufacturer: confirm the company actually makes this product category (critical for sex-tech — e.g. Svakom, Lovense, We-Vibe, Satisfyer, Kiiroo). Manufacturer ID may be a **chip OEM** (Actions, Nordic) while brand appears in **local name** — match on name when IDs disagree.
3. Copy into `FINDINGS.md` → **Configuration**: device UUID, name, manufacturer ID + verified name, advertised/partial service UUIDs.
4. If `--discover` was not run, run it before protocol work to pin Rx (write) and Tx (notify) characteristics.

### Ranking heuristics (why this works)

Most BLE noise in a home/office is Apple/Microsoft/Samsung devices. A powered-on intimate device typically:

- Advertises a **product local name** (not a laptop name)
- Uses a **non–major-OEM** manufacturer ID or none
- Often exposes **UART-style** 16-bit UUIDs `0xFFE0`–`0xFFE2`

This alone locates the target often enough to skip manual address entry.

---

## Step 6: Human verification (`ble_verify`)

**Required before FINDINGS.md.**

```bash
cargo run --bin ble_verify -- --device UUID --plan verify_plan.json --output verify_results.md
```

1. User watches device during each checkpoint burst.
2. Compare to `expect` in plan — thrust, stop, vibration, etc.
3. Press **y** / **n** / **r** / **q** (see README).

Draft plan from `verify_plan.example.json`. ≤15 checkpoints. Include stop command per family. Only **y** rows → FINDINGS.md.

---

## Core Loop (Fallback)

Use when automated scan does not surface a clear target, or ranking is ambiguous (multiple CANDIDATEs at similar RSSI).

```
0. DISCOVER DEVICE + RESEARCH
1. CONNECT + TRANSPORT VERIFY
2. DISCOVER FRAME HEADER + LAYOUT
3. ESTABLISH SESSION (handshake if required)
4. SEND CANDIDATE COMMAND
5. READ TX NOTIFICATION + OPTIONAL STATUS QUERY
6. CLASSIFY RESULT
7. UPDATE FINDINGS.md
8. CHOOSE NEXT EXPERIMENT (narrow or recurse)
```

**Recurse** when a channel is promising but semantics are unclear: change one byte, fresh connection after handshake, compare physical behavior vs BLE replies.

**Iterate** when a sweep is flat: change header, length, subcmd, params, multi-frame timing.

Stop when a path is a **dead end** (echo-only, silent, idle loop).

### Generic header probes (no assumed sync byte)

```
Probe A — sync sweep:     [H] 00 00 00 00 00 00     H ∈ 00, 55, AA, A5, 5A, FF
Probe B — UART-style:       [H] 01 00 00 00 00 00     H ∈ 00, 55, AA
Probe C — length-prefixed:  03 [H] 00 00               H ∈ 00, 55, AA
```

Ask the user: which writes produced Tx data, response length, physical movement?

---

## Research Pass (Before Hardware Sweeps)

Mandatory large research pass **before** byte grids:

1. **buttplug.io** — https://github.com/buttplugio/buttplug — contributed device protocols by brand/OEM.
2. **GitHub / forums** — `{brand} BLE protocol`, `{product} FFE1`, `buttplug {brand}`.
3. **Bluetooth SIG company IDs** — https://www.bluetooth.com/specifications/assigned-numbers/
4. **Adjacent brands** — Chinese OEMs often share UART framing (`0x55`, 7-byte frames) across lines.

**Spawn a research subagent** (different model/provider) to survey buttplug + web; merge into `FINDINGS.md` → **Research Notes** with source links.

---

## BLE Client Requirements

Any minimal client (Rust `btleplug` recommended) must:

| Step | Action |
|------|--------|
| Scan | 2–3 s; log all devices during discovery |
| Connect | One GATT client; disconnect official app / Bluetility |
| Subscribe | Tx notify **before** sending commands |
| Write | Default write-without-response unless proven otherwise |
| Wait | ~500 ms per notification; drain stale notifications before each send |
| Handshake gap | ~80 ms between init frames when multi-frame init is used |
| Motor sustain | Many devices need commands repeated ~50 ms to hold state |

Constants to set **after scan + GATT discovery** (never hardcode from another device):

- `DEVICE_ADDRESS` — UUID (macOS) or MAC
- `RX_CHARACTERISTIC_UUID` / `TX_CHARACTERISTIC_UUID`

---

## Frame Header Discovery

| Priority | Candidate | Typical families |
|----------|-----------|------------------|
| 1 | `0x55` | Svakom, many UART OEM modules |
| 2 | `0xAA` | Alternate sync |
| 3 | `0xA5` / `0x5A` | Checksum-framed |
| 4 | None | Length-prefixed or raw commands |

Confirmed header = consistent non-idle Tx responses, not random echo.

### Common layouts

```
Pattern A (7-byte UART):  [HDR] [TYPE] [SUB] [P1] [P2] [P3] [SUFFIX]
Pattern B (4-byte):       [HDR] [CMD] [VAL] [CHK]
Pattern C (variable):     [LEN] [PAYLOAD...] [CHK]
```

---

## Handshake Verification

1. Hypothesize init sequence from research (often 2–3 frames in &lt;250 ms).
2. Send immediately after connect + subscribe.
3. Record exact bytes in `FINDINGS.md` → **Session Start** as Rust `const HANDSHAKE`.
4. Re-test commands that failed pre-handshake.
5. **Ask user:** repeated connect → handshake → disconnect → does device **reset** (homing, short buzz, LED)? Record yes/no in FINDINGS.md.

---

## Response Classification

| Class | Meaning | Action |
|-------|---------|--------|
| **echo** | response == sent | **Inconclusive** — queue for `ble_verify`; do not mark verified |
| **standard ack** | fixed short ack | Query candidate — verify read semantics with user |
| **fixed blob** | same long response | Device info — verify once, usually read-only |
| **status read** | stable marker byte | Read-only; status delta alone is **not** motor proof |
| **silent** | no notification | Wrong shape/channel — try other tail or channel |
| **idle** | default loop | Pre-init or wrong channel |
| **non-standard** | unusual length/bytes | Queue for `ble_verify` |
| **physical only** | moves, BLE unchanged | **Verified motor path** — still run `ble_verify` to capture hex |

Only **ble_verify success** rows go into FINDINGS.md. Echo and non-standard are **candidates only**.

---

## Command Sweeps (After Handshake)

Order **most likely → least likely** from research:

1. Type/command sweep — fix header, sweep command byte
2. Subcmd sweep — fix command, sweep subcmd
3. Param sweep — fix command + subcmd, sweep P1, P2
4. Multi-frame — init → query → command without status between
5. Write-with-response — test once; document if unsupported

Do **not** re-sweep dead ends. Always ask user about **physical movement** when BLE status is flat.

---

## Per-Command Verification

Replaced by **`ble_verify`** (Step 6). Do not batch-verify in one headless run.

For each checkpoint in `verify_plan.json`:

1. Single command under test (+ optional stop frame)
2. User selects **y / n / r / q** at terminal
3. Success → row in FINDINGS.md; Error → revise plan and re-probe

---

## Multi-Agent Research

| Agent | Role |
|-------|------|
| Main | BLE scan, connect, sweeps, `FINDINGS.md` updates |
| Research subagent | buttplug survey, SIG IDs, FCC/manuals, handshake hypotheses with sources |

Research agent does **not** send BLE traffic.

---

## When to Stop

- Single/multi-frame and write-type variants exhausted as dead ends
- Control channel confirmed physically; BLE status may stay static
- Remaining work = official app packet capture

Open question template:

> Capture {official app} traffic on opcode `{XX}` post-handshake and diff against probe frames.

---

## FINDINGS.md Specification (Authoritative)

Copy `FINDINGS.template.md` as the starting point. Any LLM writing `FINDINGS.md` **must** follow that structure.

**Principle:** FINDINGS.md is a **verified success command set** only. Scan logs, research, dead ends, and probe grids go in auxiliary files — not FINDINGS.md.

### Required sections (in order)

#### 1. Title

```markdown
# {Brand} {Product} — Verified BLE Commands
```

Opening line: document only commands that work; exclude rejected/no-action probes.

#### 2. Device Info

| Item | Value |
| --- | --- |
| Brand | … |
| Product | … |
| Internal model | … (omit row if unknown) |
| Product code | … (omit if unknown) |
| Official app | … |

#### 3. BLE UUID

| Role | UUID |
| --- | --- |
| Service | full 128-bit UUID |
| Write | Rx characteristic |
| Notify | Tx characteristic |

#### 4. Frame Format

- Layout: `55 <cmd> <p0> <p1> <p2> <p3> <tail>`
- Table: command family → tail rule (CRC-8 C2, fixed `AA`, fixed `00`, …)
- If CRC-8 C2 applies, document poly/init/xorout/refin/refout; use `src/crc.rs` when probing

**Do not assume one tail byte for all opcodes.** KooSync/Jetpack uses CRC on `0x08` and fixed `AA` on `0x04`.

#### 5+. One `##` section per verified command family

Each confirmed family (Boost, Direct Stretch, M-modes, battery query, status query, …) gets:

- **Command format** — hex pattern with named fields
- **Field table** — range and meaning
- **Verified commands** — `{key} | Command | Effect` (or Query / Response for reads)
- **Confirmed behavior** — user-verified bullets only

#### Last: Implementation Notes (optional)

Short table: use case → family → hex pattern.

---

### Explicitly excluded from FINDINGS.md

| Old section | New home |
| --- | --- |
| Scan Results | `scan_results.md` |
| Research Notes | agent / `test_results.md` |
| Session Start | only if verified required |
| Dead Ends / Open Questions | omit |
| Protocol Model | omit |
| Rust `const` blocks | use spaced hex in tables |

---

### FINDINGS.md style rules

- **Verified only** — NACK or no movement → not in FINDINGS.md
- **Hex in fenced blocks or tables**
- **Name fields** in patterns (`<mode>`, `<scale>`, `<travel>`, `<CRC>`)
- **Effect column** requires user-confirmed physical result, not BLE echo alone
- **One section per actuator family**, not one merged opcode table

### Auxiliary files

| File | Contents |
| --- | --- |
| `scan_results.md` | Full scan from `ble_scan` |
| `test_results.md` | Probe grid from `ble_probe` |
| `sweep_results.md` | Parameter sweep from `ble_sweep` |
| `verify_results.md` | Human gate from `ble_verify` — **source of truth for FINDINGS.md** |
| `verify_plan.json` | Checkpoint script for Step 6 |

Merge **success** rows from `verify_results.md` into FINDINGS.md; never from probe grids alone.

### Tail-byte discovery (agent)

After header `0x55` is confirmed:

1. Fixed `AA` — e.g. `55 04 00 00 00 {scale} AA` (Boost on KooSync)
2. CRC-8 C2 — `frame_with_crc([55, cmd, p0, p1, p2, p3])` (Stretch / M-mode)
3. Fixed `00` — legacy Svakom 7-byte (Klitty vibrate)

Validate with **physical effect** — wrong tails may still echo on some firmware.

---

## `ble_scan` / `ble_probe` / `ble_sweep` / `ble_verify` Implementation Reference

This folder ships four Rust binaries (`btleplug` + `tokio`):

| File | Role |
| ---- | ---- |
| `src/manufacturers.rs` | SIG company ID classify (major OEM vs niche vs unknown) |
| `src/crc.rs` | CRC-8 C2 tail (`frame_with_crc`, `frame_with_aa`) |
| `src/session.rs` | Shared connect, subscribe, send, burst, handshake |
| `src/bin/ble_scan.rs` | Scan, rank, optional `--discover` GATT dump |
| `src/bin/ble_probe.rs` | Header probes, opcode sweep, `--auto` pipeline, `--burst` |
| `src/bin/ble_sweep.rs` | Single-session parameter grid with status before/after |
| `src/bin/ble_verify.rs` | Interactive human verification gate (Step 6) |
| `verify_plan.example.json` | Example checkpoint plan |
| `Cargo.toml` | Dependencies |

### `ble_verify` flags

| Flag | Purpose |
| ---- | ------- |
| `--device UUID` | Target peripheral |
| `--plan path` | JSON checkpoint plan (required) |
| `--output path` | Write `verify_results.md` (default) |

### `ble_probe` flags

| Flag | Purpose |
| ---- | ------- |
| `--device UUID` | Target peripheral (from ble_scan) |
| `--auto` | Full discovery: headers → opcode sweep → burst candidates |
| `--channel ffe1` | Limit to one Rx/Tx pair |
| `--header-sweep` / `--opcode-sweep` | Individual phases |
| `--handshake` | Send Svakom-style 3-frame init before probes |
| `--burst "55 …"` | Sustain frames for N seconds |
| `--output path` | Write `test_results.md` |

Flags: `--brand`, `--product`, `--seconds`, `--output`, `--discover`

Ranking scores (approximate):

| Signal | Score |
|--------|-------|
| Major consumer OEM | −100 (SKIP) |
| Unknown / non-major manufacturer | +40 |
| UART FFE0/FFE1/FFE2 advertised | +50 |
| `--brand`/`--product` name match | +80 |

---

## Checklist (Every Session)

- [ ] `ble_scan --discover` → UUID + Rx/Tx UUIDs
- [ ] Research app/OEM **before** sweeps
- [ ] `ble_probe --auto` → candidates in `test_results.md`
- [ ] `ble_sweep` if tails/families unclear
- [ ] `verify_plan.json` drafted (all families + stops)
- [ ] **`ble_verify` with user watching device**
- [ ] FINDINGS.md ← success rows only
- [ ] Official app disconnected during connect

---

## External References

| Resource | URL |
|----------|-----|
| Buttplug protocols | https://github.com/buttplugio/buttplug |
| Bluetooth SIG IDs | https://www.bluetooth.com/specifications/assigned-numbers/ |
| Buttplug docs | https://buttplug.io |

---

## Worked examples (not defaults)

| Device | FINDINGS style | Key protocol facts |
| ------ | -------------- | ------------------ |
| Svakom Klitty | Verified commands | `0x55` 7-byte, tail `00`; opcodes `03`/`09`/`14`; sustain ~50 ms |
| Kaotik Jetpack / HF470 | Verified commands | Boost `0x04` tail `AA`; Stretch/M `0x08` tail CRC-8 C2; see reference capture |

Do not copy bytes across devices. See `FINDINGS.template.md` for output format.
