# {Brand} {Product} — Verified BLE Commands

Document **only** commands that produce the intended physical effect or a valid read response. Source: **`verify_results.md` success rows** from `ble_verify`. Rejected probes stay in `test_results.md`.

## Device Info

| Item | Value |
| --- | --- |
| Brand | {Brand} |
| Product | {Product} |
| Internal model | {if known} |
| Product code | {if known} |
| Official app | {app name} |

## BLE UUID

| Role | UUID |
| --- | --- |
| Service | `{full service UUID}` |
| Write | `{Rx characteristic UUID}` |
| Notify | `{Tx characteristic UUID}` |

## Frame Format

Most commands are 7 bytes:

```text
55 <cmd> <p0> <p1> <p2> <p3> <tail>
```

| Command family | Tail rule |
| --- | --- |
| {e.g. `0x08` stretch / modes} | CRC-8 C2 over bytes 0–5 |
| {e.g. `0x04` boost} | Fixed `AA` |

If CRC-8 C2 applies, document parameters:

```text
poly   = 0xF0
init   = 0xFF
xorout = 0xFF
refin  = false
refout = true
```

Use `ble_hack_skill::crc::frame_with_crc` when probing CRC families.

---

## {Query or command family name}

### Query

```text
{hex bytes}
```

### Response example

```text
{hex bytes}
```

{One line explaining response fields.}

---

## {Actuator family — e.g. Boost / Thrust / Vibrate}

{One sentence: when to use this family.}

### Command format

```text
{pattern with named fields}
```

| Field | Meaning |
| --- | --- |
| `{field}` | {range and semantics} |

### Verified commands

| {key column} | Command | Effect |
| --- | --- | --- |
| {e.g. scale `00`} | `{hex}` | {stop / level 1 / …} |
| … | … | … |

### Confirmed behavior

- {bullet — only user- or physically-verified facts}
- {e.g. single non-zero frame latched until stop}
- {e.g. scale maps to stroke depth}

---

## {Second actuator family — repeat section as needed}

…

---

## Implementation Notes

| Use case | Command family | Format |
| --- | --- | --- |
| {video sync} | {Boost} | `{hex pattern}` |
| {preset modes} | {M1–M8} | `{hex pattern}` |
| {level control} | {Direct stretch} | `{hex pattern}` |
