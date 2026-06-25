# {Brand} {Product} — reverse-engineering status

## Product features (requirements)

Source: {URL to official product page, retailer listing, or manufacturer spec — fetch before first probe.}

| Item | Detail |
| --- | --- |
| Product | {marketing name} |
| Model | {model / SKU} |
| Official app | {app name, if any} |
| **Actuators to map** | {e.g. Suction, Vibration, Heat, Thrust — list every motor/feature from the product page} |
| Form factor | {one line from listing} |
| Intensity / modes | {levels, presets, standalone vs app — from page or packaging} |
| BLE stack hint | {manufacturer data, chipset clues from scan} |

**Reverse-engineering goal:** one verified BLE command family per actuator listed above, plus stop/idle for each. Notify echoes alone do not satisfy a requirement row.

Include the product features fetched from product information page on the top of this file so that requirements are clearly specified (do not delete this sentence).

---

**{Active | Sweep paused | Blocked}** ({YYYY-MM}). {One-line reason or resume condition.}

`STATUS.md` is the **working memory** for an in-progress reverse-engineering session. Update it after every opcode human gate, rejection, or pause. It is **not** the final deliverable — `FINDINGS.md` is.

**Rules**

- **Confirmed** = human **y** on physical actuation (or valid read response for query commands).
- **Rejected** = human **n**, or a hypothesis disproved by a later test.
- **Open** = notify/echo seen but no human movement confirmation yet.
- Never promote a row from Open → Confirmed based on notify mirror or probe class alone.

Copy this file to the project root as `STATUS.md` after the first successful scan. Keep it current until `ble_check` prints `Ready for FINDINGS: true`.

---

## Confirmed (human y)

| Item | Value |
| --- | --- |
| BLE name | {advertised local name} |
| Device UUID | `{from ble_session.json}` |
| Header | `{e.g. 0x55}` |
| Motor channel | {e.g. FFE1 write / FFE2 notify} |
| TX frame ({family}) | {N bytes} — `{hex pattern with named fields}` |
| RX status | {N bytes} — `{pattern}` (mirror / ack — **not movement proof**) |
| {Motor / feature A} | Opcode `{0xNN}`, {levels / modes verified} |
| {Motor / feature B} | {…} |

---

## Rejected / corrected

| Hypothesis | Result |
| --- | --- |
| `{0xNN}` = {suction / heat / …} | **Rejected** — `{example hex}` does not actuate {feature} (notify `{rx}` is not proof) |
| Notify echo = motor proof | **Rejected** — must ask user y/n |
| Handshake required | **Rejected** / **Confirmed** — {evidence} |
| Level `00` = off | **Rejected** / **Confirmed** — {evidence} |
| {other bad assumption} | {outcome} |

---

## Open questions

| Opcode / topic | Notify? | Physical effect (human) |
| --- | --- | --- |
| `0x{NN}` | {yes — `55 8X …` / no / silent} | {not confirmed / none observed / hypothesis} |
| `0x{NN}` | … | … |
| Stop commands | — | {none verified for {motor}} |
| Actuator coverage | — | {map each **Product features** actuator → opcode or “unmapped”} |
| Frame length | — | {6 vs 7 byte TX still TBD for opcode …} |

Add one row per opcode that returned non-silent RX during opcode sweep. Do **not** fill the physical column until the user answers y/n in chat.

---

## Artifacts

| File | Role |
| --- | --- |
| `STATUS.md` | This file — confirmed / rejected / open |
| `FINDINGS.md` | Verified commands only (final deliverable) |
| `scan_results.md` | BLE scan + GATT discovery |
| `test_results.md` | Header + opcode probe grid |
| `sweep_results.md` | Wide sweep candidates — may include echo false positives |
| `verify_plan*.json` | Checkpoint plans — build only after opcode gate |
| `verify_results.md` | Human verify outcomes |
| `{physical_sweep.md}` | Optional per-project human correlation log |

---

## Next steps

1. {e.g. Opcode sweep `0x05`–`0x07` with chat y/n per opcode}
2. {e.g. Lock TX length from RX notify for each responding opcode family}
3. {e.g. Find true stop commands — level `00` may not be off}
4. {e.g. Resume wide sweep only for opcodes with human **y**}

When discovery completes, archive or trim `STATUS.md` — `FINDINGS.md` holds what ships.
