# {Brand} {Product} — BLE Commands

Write to **{write_target}**. Each speed byte is `0x00` (off) through `0xFF` (max).

**Motor frame ({N} bytes):**

| Byte | Value | Meaning |
| --- | --- | --- |
| 0 | `{header}` | Header (fixed) |
| 1 | `{opcode}` | Opcode (fixed) |
| 2 | `00`–`FF` | {byte2_label} |
| … | … | … |

Example: `{example_hex}`.

**Orgasm (3 steps):** *(if applicable)*

| Step | Bytes | Meaning |
| --- | --- | --- |
| Arm | `01` | 1-byte prime before sustain |
| Sustain | `{sustain_hex}` | Repeat ~10 Hz after arm |
| Stop | `{stop_hex}` | End orgasm mode |

## Cautions

- {caution 1}
- Disconnect the official app before sending commands.
