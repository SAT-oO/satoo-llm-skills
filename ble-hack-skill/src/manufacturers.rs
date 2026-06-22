//! Bluetooth SIG company identifier lookup for BLE scan ranking.
//! Major consumer OEMs are deprioritized; niche / unknown IDs are candidates.

pub enum OemClass {
    MajorConsumer,
    NicheProduct { brand: &'static str },
    Unknown,
    NoData,
}

pub fn company_name(id: u16) -> &'static str {
    COMPANY_NAMES
        .iter()
        .find_map(|(cid, name)| (*cid == id).then_some(*name))
        .unwrap_or("Unknown")
}

pub fn classify(id: Option<u16>) -> OemClass {
    let Some(id) = id else {
        return OemClass::NoData;
    };
    if MAJOR_CONSUMER_OEMS.contains(&id) {
        return OemClass::MajorConsumer;
    }
    if let Some((_, brand)) = NICHE_PRODUCT_OEMS.iter().find(|(cid, _)| *cid == id) {
        return OemClass::NicheProduct { brand };
    }
    OemClass::Unknown
}

/// Returns true when `needle` appears in `brand` or `local_name` (case-insensitive).
pub fn name_matches(brand: &str, product: Option<&str>, local_name: Option<&str>) -> bool {
    let brand_l = brand.to_ascii_lowercase();
    let check = |s: &str| {
        let s = s.to_ascii_lowercase();
        s.contains(&brand_l) || brand_l.contains(&s)
    };
    if product.is_some_and(check) {
        return true;
    }
    local_name.is_some_and(check)
}

/// Major OEMs — phones, laptops, watches, mainstream accessories.
const MAJOR_CONSUMER_OEMS: &[u16] = &[
    0x0002, // Intel
    0x0006, // Microsoft
    0x000F, // Broadcom
    0x0046, // Dell
    0x004C, // Apple
    0x0059, // Nordic (often dev kits; still common noise)
    0x0075, // Samsung
    0x0087, // Meta
    0x008F, // HP
    0x00D2, // Bose
    0x00E0, // Google
    0x012D, // Sony
    0x0157, // Anker
    0x0171, // Amazon
    0x01AB, // Xiaomi
    0x0277, // Huawei
    0x0310, // OnePlus
    0x0499, // Logitech
];

/// Known intimate-wellness / niche product OEM company IDs (expand as confirmed).
const NICHE_PRODUCT_OEMS: &[(u16, &str)] = &[
    // Many products use generic UART modules; brand often appears in local_name instead.
    // Add confirmed SIG IDs here as they are verified on hardware.
];

const COMPANY_NAMES: &[(u16, &str)] = &[
    (0x0002, "Intel"),
    (0x0006, "Microsoft"),
    (0x000F, "Broadcom"),
    (0x0046, "Dell"),
    (0x004C, "Apple"),
    (0x0059, "Nordic Semiconductor"),
    (0x0075, "Samsung"),
    (0x0087, "Meta"),
    (0x008F, "HP"),
    (0x00D2, "Bose"),
    (0x00E0, "Google"),
    (0x012D, "Sony"),
    (0x0157, "Anker"),
    (0x0171, "Amazon"),
    (0x01AB, "Xiaomi"),
    (0x0277, "Huawei"),
    (0x0310, "OnePlus"),
    (0x0499, "Logitech"),
    (0x05F1, "Actions (Zhuhai) Technology"),
    (0x0A5C, "Broadcom (alt)"),
];
