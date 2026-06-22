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

```
STEP 0  cd ble-hack-skill && cargo run --bin ble_scan [--brand X] [--product Y] [--discover] [--output scan_results.md]
STEP 1  Web-verify top candidate manufacturer + product category
STEP 2  Create FINDINGS.md (Configuration + Scan Results sections)
STEP 3  Research pass (buttplug, GitHub, FCC) → Research Notes in FINDINGS.md
STEP 4  Connect transport; subscribe Tx; discover Rx/Tx UUIDs if not from scan
STEP 5  Discover frame header (no assumed 0x55)
STEP 6  Verify handshake; confirm reset-on-repeat with user
STEP 7  Sweep commands (most likely → least likely); classify responses
STEP 8  Verify each interesting command in an isolated round
STEP 9  Update FINDINGS.md after every experiment
```

**If Step 0 yields a PRIMARY or CANDIDATE device**, proceed with Steps 1–9 on that target.

**If Step 0 yields only SKIP / LOW devices**, fall back to the [Core Loop](#core-loop-fallback) — broader scan, manual brand/product flags, generic header probes, user confirmation of physical behavior.

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
| **echo** | response == sent | Dead end for control |
| **standard ack** | fixed short ack | Usually query-only |
| **fixed blob** | same long response | Device info |
| **status read** | stable marker byte | Read-only |
| **silent** | no notification | Wrong shape/channel |
| **idle** | default loop | Pre-init or wrong channel |
| **non-standard** | unusual length/bytes | **Investigate** |
| **physical only** | moves, BLE unchanged | **Critical** — document physical effect |

Only **non-standard** and **physical-only** deserve new command rows or probe sections.

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

For each **non-standard** or **physical-only** result:

1. Fresh connection + full handshake
2. **Single** command under test (+ optional status read)
3. User confirms movement / suction / heat / LED / sound
4. Document: sent hex, response hex, status after, physical effect, repeatable across reconnects

Never batch-verify interesting commands in one run.

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

Any LLM writing `FINDINGS.md` **must** follow this structure. Sections appear **in this order**. Use `{Product}` / `{Brand}` placeholders until confirmed. Update after **every** experiment.

### Required sections

#### 1. Title

```markdown
# {Brand} {Product} — BLE Control Reference
```

#### 2. Connection (table)

| Item | Value |
| ---- | ----- |
| Device UUID | `{macOS UUID or MAC from ble_scan}` |
| Device name | `{local_name from advertisement}` |
| Manufacturer | `0x{ID}` — {verified company name from web} |
| Rx (write) | `0x....` — write-without-response or write-with-response |
| Tx (notify) | `0x....` |

**Rules:** Rx = characteristic accepting command writes. Tx = characteristic with notify/indicate for responses. State write type explicitly. Include scan tier (`PRIMARY`/`CANDIDATE`) and date if useful.

#### 3. Scan Results (table) — fill from `ble_scan` / `scan_results.md`

| tier | device_id | name | mfg_id | rssi | notes |
| ---- | --------- | ---- | ------ | ---- | ----- |

One row per scanned device; highlight selected target.

#### 4. Research Notes (bullets)

- buttplug / GitHub / web sources with URLs
- Comparable devices and framing hypotheses
- Header byte hypothesis + evidence status: confirmed / refuted / open

#### 5. Session Start (handshake)

Prose: whether handshake is **required** for control or only mirrors app connect behavior.

```rust
const HANDSHAKE: [&[u8]; N] = [
    &[0x.., ...], // phase label
];
```

- Timing: gap between frames (e.g. ~80 ms), total window (e.g. &lt;250 ms)
- **Reset-on-repeat:** yes / no / not tested (user-confirmed)

#### 6. Frame Layout

ASCII diagram of byte positions with confirmed names:

```
[HDR] [opcode] [subcmd] [P1] [P2] [P3] [suffix]
```

Table mapping bytes to semantics per opcode family when known.

**Rules:** Use confirmed `[HDR]` — never default to `55` without evidence. Note sustain rule if motor commands must repeat (~50 ms).

#### 7. Stop Sequence (when applicable)

Hex frames + Rust `const STOP` block. Note whether ceasing commands without stop burst halts the motor.

#### 8. Motor / Actuator Commands

Per effect (vibrate, suction, lick, heat, etc.):

- Hex pattern with byte semantics
- Rust `const` arrays for levels/modes
- What Tx echoes vs what status read returns
- Confirmed physical effects (user-verified)

#### 9. Status Read

```
55 02 01 00 00 00 00   ← example; use confirmed bytes
```

Document marker bytes (e.g. idle `0x50` vs post-burst `0x46`).

#### 10. Command Channel Reference (table)

For channels that are not motor-specific (info, echo, mode query):

| opcode | subcmd | suffix | purpose | notes |
| ------ | ------ | ------ | ------- | ----- |

**Cell style:** short — `echo only`, `read-only`, `standard ack only`. Never combine `echo only` with `status unchanged`.

#### 11. Probe Commands (when replay set exists)

- One-line frame convention
- Rust `const PROBES` or `TYPE_PROBES` block with comments
- Table: `label | hex`
- Burst test results table if run:

| label | sent | BLE ack | status after | physical effect |

#### 12. Example Responses (optional)

| opcode | subcmd | sent | response |

Use only for non-obvious frames.

#### 13. Protocol Model (Current)

```
[HDR] [TYPE] [SUB] [P1] [P2] [P3] [P4]
TYPE xx → one-line behavior
```

Update when a new opcode role is confirmed.

#### 14. Dead Ends (Automated Session)

| test | result |

Short, final — no hedging. Move exhausted paths here; remove from open questions.

#### 15. Open Questions / Next Steps

Bullet list — usually app capture, unmapped opcodes, stop command, param semantics.

---

### FINDINGS.md style rules

- **Table-first** — facts in tables, not lab-note prose
- **Hex in backticks** — spaced hex in tables when helpful: `` `55 09 01 00 00 00 00` ``
- **Separate facts from hypothesis** — confirmed rows vs open questions
- **Physical effects** — user-confirmed movement overrides BLE-only "no change"
- **No template overwrite** — merge `test_results.md` by hand; do not auto-regenerate curated sections
- **Living doc** — timestamp or session note when a section materially changes

### Auxiliary file: `test_results.md`

Simple machine-generated table from last probe run:

| label | sent | response | status_after |

Does not replace `FINDINGS.md`; agent merges interesting rows into canonical sections.

---

## `ble_scan` Implementation Reference

This folder ships the `ble_scan` binary (Rust + `btleplug`):

| File | Role |
| ---- | ---- |
| `src/manufacturers.rs` | SIG company ID classify (major OEM vs niche vs unknown) |
| `src/bin/ble_scan.rs` | Scan, rank, optional `--discover` GATT dump |
| `Cargo.toml` | Dependencies |

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

- [ ] `cd ble-hack-skill && cargo run --bin ble_scan` (+ `--discover`, `--output scan_results.md`)
- [ ] Web-verify manufacturer for top candidate
- [ ] Create/update `FINDINGS.md` § Connection + Scan Results
- [ ] Research buttplug + web; subagent if needed
- [ ] Discover frame header — **do not** assume `0x55`
- [ ] Disconnect competing BLE apps
- [ ] Verify handshake; ask user about reset-on-repeat
- [ ] Sweep commands; classify every response
- [ ] Isolated verification per interesting command
- [ ] User confirms physical behavior per probe
- [ ] Update command tables, dead ends, protocol model
- [ ] Trim client to handshake + active probes only

---

## External References

| Resource | URL |
|----------|-----|
| Buttplug protocols | https://github.com/buttplugio/buttplug |
| Bluetooth SIG IDs | https://www.bluetooth.com/specifications/assigned-numbers/ |
| Buttplug docs | https://buttplug.io |

---

## Worked Example (Svakom Klitty — not a default)

One device this skill was exercised on; **do not copy bytes to other brands** without confirmation.

| Phase | Outcome |
|-------|---------|
| Scan + name match | Device UUID + FFE1/FFE2 |
| Research | `0x55` 7-byte UART hypothesis |
| Handshake | 3-frame init; app-mirror, not strictly required for motor |
| Motors | opcodes `03` vibrate, `09` suction, `14` lick; sustain ~50 ms |
| Stop | burst `03`/`09`/`14` off-frames with bytes 4–5 zero |

See a completed `FINDINGS.md` in the parent project for tone and density — but **this specification** is the normative source.
