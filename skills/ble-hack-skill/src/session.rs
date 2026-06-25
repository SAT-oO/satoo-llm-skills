use anyhow::{Context, Result, anyhow};
use btleplug::api::{Central, CharPropFlags, Manager, Peripheral, ScanFilter, WriteType};
use btleplug::platform::{Adapter, Manager as BluetoothManager, Peripheral as BlePeripheral};
use futures::StreamExt;
use std::time::Duration;
use tokio::time;
use uuid::Uuid;

use crate::gatt;

pub const RESPONSE_WAIT: Duration = Duration::from_millis(500);
pub const INTER_CMD_GAP: Duration = Duration::from_millis(50);
pub const HANDSHAKE_GAP: Duration = Duration::from_millis(80);

pub struct ChannelPair {
    pub label: String,
    pub rx: Uuid,
    pub tx: Uuid,
}

impl Clone for ChannelPair {
    fn clone(&self) -> Self {
        Self {
            label: self.label.clone(),
            rx: self.rx,
            tx: self.tx,
        }
    }
}

pub struct Session {
    pub peripheral: BlePeripheral,
    pub rx_char: btleplug::api::Characteristic,
    pub tx_char: btleplug::api::Characteristic,
    pub channel: ChannelPair,
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

pub fn spaced_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub async fn adapter() -> Result<Adapter> {
    let manager = BluetoothManager::new().await?;
    manager
        .adapters()
        .await?
        .into_iter()
        .next()
        .context("No Bluetooth adapters found")
}

async fn try_connect_cached(
    adapter: &Adapter,
    device_id: &str,
    channel: &ChannelPair,
) -> Result<Option<Session>> {
    let peripheral = match adapter
        .peripherals()
        .await?
        .into_iter()
        .find(|p| p.id().to_string().eq_ignore_ascii_case(device_id))
    {
        Some(p) => p,
        None => return Ok(None),
    };

    if peripheral.is_connected().await.unwrap_or(false) {
        let _ = peripheral.disconnect().await;
        time::sleep(Duration::from_millis(200)).await;
    }

    peripheral.connect().await?;
    peripheral.discover_services().await?;

    let rx_char = peripheral
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == channel.rx)
        .ok_or_else(|| anyhow!("Rx {} not found on device", channel.rx))?;
    let tx_char = peripheral
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == channel.tx)
        .ok_or_else(|| anyhow!("Tx {} not found on device", channel.tx))?;

    let _ = peripheral.notifications().await?;
    peripheral.subscribe(&tx_char).await?;
    time::sleep(Duration::from_millis(200)).await;

    Ok(Some(Session {
        peripheral,
        rx_char,
        tx_char,
        channel: ChannelPair {
            label: channel.label.clone(),
            rx: channel.rx,
            tx: channel.tx,
        },
    }))
}

pub async fn connect(adapter: &Adapter, device_id: &str, channel: &ChannelPair) -> Result<Session> {
    // Fast path: peripheral may still be in the OS cache without an active scan.
    if let Some(session) = try_connect_cached(adapter, device_id, channel).await? {
        return Ok(session);
    }

    for attempt in 1..=3 {
        adapter.start_scan(ScanFilter::default()).await?;
        time::sleep(Duration::from_secs(3)).await;

        if let Some(peripheral) = adapter
            .peripherals()
            .await?
            .into_iter()
            .find(|p| p.id().to_string().eq_ignore_ascii_case(device_id))
        {
            adapter.stop_scan().await?;
            peripheral.connect().await?;
            peripheral.discover_services().await?;

            let rx_char = peripheral
                .characteristics()
                .into_iter()
                .find(|c| c.uuid == channel.rx)
                .ok_or_else(|| anyhow!("Rx {} not found on device", channel.rx))?;
            let tx_char = peripheral
                .characteristics()
                .into_iter()
                .find(|c| c.uuid == channel.tx)
                .ok_or_else(|| anyhow!("Tx {} not found on device", channel.tx))?;

            let _ = peripheral.notifications().await?;
            peripheral.subscribe(&tx_char).await?;
            time::sleep(Duration::from_millis(200)).await;

            return Ok(Session {
                peripheral,
                rx_char,
                tx_char,
                channel: ChannelPair {
                    label: channel.label.clone(),
                    rx: channel.rx,
                    tx: channel.tx,
                },
            });
        }

        adapter.stop_scan().await?;
        eprintln!("Scan attempt {attempt}/3: device not found, retrying...");
        time::sleep(Duration::from_secs(1)).await;
    }

    Err(anyhow!("Device not found: {device_id}"))
}

/// Scan once, connect immediately when `device_id` appears (avoids cache miss after standalone scan).
pub async fn connect_fresh(
    adapter: &Adapter,
    device_id: &str,
    channel: &ChannelPair,
    scan_seconds: u64,
) -> Result<Session> {
    adapter.start_scan(ScanFilter::default()).await?;
    let deadline = time::Instant::now() + Duration::from_secs(scan_seconds);
    loop {
        if let Some(peripheral) = adapter
            .peripherals()
            .await?
            .into_iter()
            .find(|p| p.id().to_string().eq_ignore_ascii_case(device_id))
        {
            adapter.stop_scan().await?;
            peripheral.connect().await?;
            peripheral.discover_services().await?;

            let rx_char = peripheral
                .characteristics()
                .into_iter()
                .find(|c| c.uuid == channel.rx)
                .ok_or_else(|| anyhow!("Rx {} not found on device", channel.rx))?;
            let tx_char = peripheral
                .characteristics()
                .into_iter()
                .find(|c| c.uuid == channel.tx)
                .ok_or_else(|| anyhow!("Tx {} not found on device", channel.tx))?;

            let _ = peripheral.notifications().await?;
            peripheral.subscribe(&tx_char).await?;
            time::sleep(Duration::from_millis(200)).await;

            return Ok(Session {
                peripheral,
                rx_char,
                tx_char,
                channel: ChannelPair {
                    label: channel.label.clone(),
                    rx: channel.rx,
                    tx: channel.tx,
                },
            });
        }
        if time::Instant::now() >= deadline {
            break;
        }
        time::sleep(Duration::from_millis(200)).await;
    }
    adapter.stop_scan().await?;
    Err(anyhow!("Device not found during fresh scan: {device_id}"))
}

/// Connect briefly, enumerate GATT, return channel pairs present on the device.
pub async fn discover_channels_on_device(
    adapter: &Adapter,
    device_id: &str,
) -> Result<Vec<ChannelPair>> {
    if let Some(peripheral) = adapter
        .peripherals()
        .await?
        .into_iter()
        .find(|p| p.id().to_string().eq_ignore_ascii_case(device_id))
    {
        peripheral.connect().await?;
        peripheral.discover_services().await?;
        let channels = gatt::channels_for_device(
            &peripheral.characteristics().into_iter().collect::<Vec<_>>(),
        );
        peripheral.disconnect().await?;
        return Ok(channels);
    }

    for attempt in 1..=3 {
        adapter.start_scan(ScanFilter::default()).await?;
        time::sleep(Duration::from_secs(3)).await;

        if let Some(peripheral) = adapter
            .peripherals()
            .await?
            .into_iter()
            .find(|p| p.id().to_string().eq_ignore_ascii_case(device_id))
        {
            adapter.stop_scan().await?;
            peripheral.connect().await?;
            peripheral.discover_services().await?;
            let channels = gatt::channels_for_device(
                &peripheral.characteristics().into_iter().collect::<Vec<_>>(),
            );
            peripheral.disconnect().await?;
            return Ok(channels);
        }

        adapter.stop_scan().await?;
        eprintln!("Channel discovery attempt {attempt}/3: device not found");
        time::sleep(Duration::from_secs(1)).await;
    }

    Err(anyhow!("Device not found: {device_id}"))
}

pub fn write_type(char: &btleplug::api::Characteristic) -> WriteType {
    if char
        .properties
        .contains(CharPropFlags::WRITE_WITHOUT_RESPONSE)
    {
        WriteType::WithoutResponse
    } else {
        WriteType::WithResponse
    }
}

pub async fn drain_notifications(
    notifications: &mut (impl StreamExt<Item = btleplug::api::ValueNotification> + Unpin),
    tx_uuid: Uuid,
    duration: Duration,
) -> Vec<Vec<u8>> {
    let mut collected = Vec::new();
    let deadline = time::Instant::now() + duration;
    while time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(time::Instant::now());
        match time::timeout(remaining, notifications.next()).await {
            Ok(Some(n)) if n.uuid == tx_uuid => collected.push(n.value),
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => break,
        }
    }
    collected
}

pub async fn send_and_wait(
    session: &Session,
    notifications: &mut (impl StreamExt<Item = btleplug::api::ValueNotification> + Unpin),
    payload: &[u8],
) -> Result<Option<Vec<u8>>> {
    send_and_wait_write(session, notifications, payload, None).await
}

pub async fn send_and_wait_write(
    session: &Session,
    notifications: &mut (impl StreamExt<Item = btleplug::api::ValueNotification> + Unpin),
    payload: &[u8],
    force_write: Option<WriteType>,
) -> Result<Option<Vec<u8>>> {
    drain_notifications(
        notifications,
        session.tx_char.uuid,
        Duration::from_millis(30),
    )
    .await;
    let wt = force_write.unwrap_or_else(|| write_type(&session.rx_char));
    session
        .peripheral
        .write(&session.rx_char, payload, wt)
        .await?;
    let response = match time::timeout(RESPONSE_WAIT, notifications.next()).await {
        Ok(Some(n)) if n.uuid == session.tx_char.uuid => Some(n.value),
        Ok(Some(_)) => None,
        Ok(None) | Err(_) => None,
    };
    time::sleep(INTER_CMD_GAP).await;
    Ok(response)
}

pub async fn listen_notifications(
    _session: &Session,
    notifications: &mut (impl StreamExt<Item = btleplug::api::ValueNotification> + Unpin),
    duration: Duration,
) -> Vec<(Uuid, Vec<u8>)> {
    let mut collected = Vec::new();
    let deadline = time::Instant::now() + duration;
    while time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(time::Instant::now());
        match time::timeout(remaining, notifications.next()).await {
            Ok(Some(n)) => collected.push((n.uuid, n.value)),
            Ok(None) | Err(_) => break,
        }
    }
    collected
}

pub async fn read_readable_chars(session: &Session) -> Result<Vec<(Uuid, Vec<u8>)>> {
    use btleplug::api::CharPropFlags;
    let mut out = Vec::new();
    for c in session.peripheral.characteristics() {
        if c.properties.contains(CharPropFlags::READ) {
            if let Ok(data) = session.peripheral.read(&c).await {
                out.push((c.uuid, data));
            }
        }
    }
    Ok(out)
}

pub async fn send_handshake(
    session: &Session,
    notifications: &mut (impl StreamExt<Item = btleplug::api::ValueNotification> + Unpin),
    frames: &[&[u8]],
) -> Result<()> {
    for (i, packet) in frames.iter().enumerate() {
        session
            .peripheral
            .write(&session.rx_char, packet, write_type(&session.rx_char))
            .await?;
        if i + 1 < frames.len() {
            time::sleep(HANDSHAKE_GAP).await;
        }
    }
    drain_notifications(
        notifications,
        session.tx_char.uuid,
        Duration::from_millis(300),
    )
    .await;
    Ok(())
}

pub async fn send_burst(
    session: &Session,
    notifications: &mut (impl StreamExt<Item = btleplug::api::ValueNotification> + Unpin),
    frames: &[Vec<u8>],
    duration: Duration,
) -> Result<Option<Vec<u8>>> {
    drain_notifications(
        notifications,
        session.tx_char.uuid,
        Duration::from_millis(30),
    )
    .await;
    let deadline = time::Instant::now() + duration;
    let mut last_response = None;
    let mut frame_idx = 0usize;

    while time::Instant::now() < deadline {
        let payload = &frames[frame_idx % frames.len()];
        frame_idx += 1;
        session
            .peripheral
            .write(&session.rx_char, payload, write_type(&session.rx_char))
            .await?;
        if let Ok(Some(n)) = time::timeout(Duration::from_millis(30), notifications.next()).await {
            if n.uuid == session.tx_char.uuid {
                last_response = Some(n.value);
            }
        }
        time::sleep(INTER_CMD_GAP).await;
    }

    Ok(last_response)
}

pub fn classify_response(sent: &[u8], response: &Option<Vec<u8>>) -> &'static str {
    match response {
        None => "silent",
        Some(r) if r.is_empty() => "silent",
        Some(r) if r.as_slice() == sent => "echo",
        Some(r) if r.len() <= 3 => "standard ack",
        _ => "non-standard",
    }
}
