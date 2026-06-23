//! CRC-8 C2 — tail byte used by some UART-framed BLE devices (e.g. Svakom-class OEM stacks).

/// CRC-8 C2: poly=0xF0, init=0xFF, xorout=0xFF, refin=false, refout=true.
/// Computed over the first 6 bytes of a 7-byte frame; result is byte 6.
pub fn crc8_c2(data: &[u8]) -> u8 {
    assert!(data.len() >= 6, "need at least 6 bytes for CRC input");
    let mut crc = 0xFFu8;
    for &byte in &data[..6] {
        crc ^= byte;
        for _ in 0..8 {
            if crc & 0x80 != 0 {
                crc = (crc << 1) ^ 0xF0;
            } else {
                crc <<= 1;
            }
        }
    }
    let mut out = crc;
    out = out.reverse_bits();
    out ^ 0xFF
}

/// Build a 7-byte frame with CRC-8 C2 tail.
pub fn frame_with_crc(bytes: [u8; 6]) -> [u8; 7] {
    let crc = crc8_c2(&bytes);
    [bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], crc]
}

/// Build a 7-byte frame with fixed `0xAA` tail (common on AA-suffix UART families).
pub fn frame_with_aa(bytes: [u8; 6]) -> [u8; 7] {
    [
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], 0xAA,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc8_c2_known_stretch_frame() {
        let f = frame_with_crc([0x55, 0x08, 0x00, 0x00, 0x01, 0x01]);
        assert_eq!(f[6], 0xFC);
    }

    #[test]
    fn crc8_c2_battery_query_frame() {
        let f = frame_with_crc([0x55, 0x02, 0x00, 0x00, 0x00, 0x00]);
        assert_eq!(
            f,
            [0x55, 0x02, 0x00, 0x00, 0x00, 0x00, 0xFC],
            "battery query tail"
        );
    }

    #[test]
    fn frame_with_aa_tail() {
        let f = frame_with_aa([0x55, 0x04, 0x00, 0x00, 0x00, 0x40]);
        assert_eq!(f, [0x55, 0x04, 0x00, 0x00, 0x00, 0x40, 0xAA]);
    }
}
