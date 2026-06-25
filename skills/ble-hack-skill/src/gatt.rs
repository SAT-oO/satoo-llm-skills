//! Discover write/notify channel pairs from a connected peripheral's GATT table.

use crate::session::ChannelPair;
use btleplug::api::{bleuuid::uuid_from_u16, CharPropFlags, Characteristic};
use uuid::Uuid;

/// Well-known UART and common third-party notify/write channel pairs.
pub fn static_channels() -> Vec<ChannelPair> {
    vec![
        ChannelPair {
            label: "FFE1/FFE2".into(),
            rx: uuid_from_u16(0xFFE1),
            tx: uuid_from_u16(0xFFE2),
        },
        ChannelPair {
            label: "AE01/AE02".into(),
            rx: uuid_from_u16(0xAE01),
            tx: uuid_from_u16(0xAE02),
        },
        ChannelPair {
            label: "AE03/AE05".into(),
            rx: uuid_from_u16(0xAE03),
            tx: uuid_from_u16(0xAE05),
        },
        ChannelPair {
            label: "FFA1/FFA2".into(),
            rx: uuid_from_u16(0xFFA1),
            tx: uuid_from_u16(0xFFA2),
        },
        ChannelPair {
            label: "007777/008888".into(),
            rx: Uuid::parse_str("00007777-0000-1000-8000-00805f9b34fb").unwrap(),
            tx: Uuid::parse_str("00008888-0000-1000-8000-00805f9b34fb").unwrap(),
        },
        ChannelPair {
            label: "7777 (W+N)".into(),
            rx: Uuid::parse_str("77777777-7777-7777-7777-777777777777").unwrap(),
            tx: Uuid::parse_str("77777777-7777-7777-7777-777777777777").unwrap(),
        },
    ]
}

fn can_write(props: CharPropFlags) -> bool {
    props.contains(CharPropFlags::WRITE) || props.contains(CharPropFlags::WRITE_WITHOUT_RESPONSE)
}

fn can_notify(props: CharPropFlags) -> bool {
    props.contains(CharPropFlags::NOTIFY) || props.contains(CharPropFlags::INDICATE)
}

fn is_fast_pair(uuid: Uuid) -> bool {
    let s = uuid.to_string().to_ascii_lowercase();
    s.contains("fe2c123") || uuid == uuid_from_u16(0xFE2C)
}

fn short_label(uuid: Uuid) -> String {
    let s = uuid.to_string();
    if let Some(pos) = s.find('-') {
        s[..pos].to_string()
    } else {
        s
    }
}

/// Infer motor-control channels from characteristics. Skips Google Fast Pair (`FE2C`).
pub fn discover_channels(characteristics: &[Characteristic]) -> Vec<ChannelPair> {
    let mut out = static_channels();
    let usable: Vec<_> = characteristics
        .iter()
        .filter(|c| !is_fast_pair(c.uuid) && !is_fast_pair(c.service_uuid))
        .collect();

    for c in &usable {
        if can_write(c.properties) && can_notify(c.properties) {
            let label = format!("{} (W+N)", short_label(c.uuid));
            if !out.iter().any(|ch| ch.rx == c.uuid) {
                out.push(ChannelPair {
                    label,
                    rx: c.uuid,
                    tx: c.uuid,
                });
            }
        }
    }

    let mut by_service: std::collections::HashMap<Uuid, Vec<&Characteristic>> =
        std::collections::HashMap::new();
    for c in &usable {
        by_service.entry(c.service_uuid).or_default().push(c);
    }

    for chars in by_service.values() {
        let writers: Vec<_> = chars
            .iter()
            .filter(|c| can_write(c.properties))
            .collect();
        let notifiers: Vec<_> = chars
            .iter()
            .filter(|c| can_notify(c.properties))
            .collect();
        for w in &writers {
            for n in &notifiers {
                if w.uuid == n.uuid {
                    continue;
                }
                let label = format!("{}/{}", short_label(w.uuid), short_label(n.uuid));
                if out.iter().any(|ch| ch.rx == w.uuid && ch.tx == n.uuid) {
                    continue;
                }
                out.push(ChannelPair {
                    label,
                    rx: w.uuid,
                    tx: n.uuid,
                });
            }
        }
    }

    out
}

/// Channels present on this peripheral (static candidates + GATT-inferred, deduped).
pub fn channels_for_device(characteristics: &[Characteristic]) -> Vec<ChannelPair> {
    discover_channels(characteristics)
        .into_iter()
        .filter(|ch| {
            characteristics.iter().any(|c| c.uuid == ch.rx)
                && characteristics.iter().any(|c| c.uuid == ch.tx)
        })
        .collect()
}
