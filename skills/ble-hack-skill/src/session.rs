use anyhow::{anyhow, Context, Result};
use btleplug::api::{Central, CharPropFlags, Manager, Peripheral, ScanFilter, WriteType};
use btleplug::platform::{Adapter, Manager as BluetoothManager, Peripheral as BlePeripheral};
use futures::StreamExt;
use std::time::Duration;
use tokio::time;
use uuid::Uuid;

pub const RESPONSE_WAIT: Duration = Duration::from_millis(500);
pub const INTER_CMD_GAP: Duration = Duration::from_millis(50);
pub const HANDSHAKE_GAP: Duration = Duration::from_millis(80);

pub struct ChannelPair {
    pub label: &'static str,
    pub rx: Uuid,
    pub tx: Uuid,
}

impl Clone for ChannelPair {
    fn clone(&self) -> Self {
        Self {
            label: self.label,
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

pub async fn connect(
    adapter: &Adapter,
    device_id: &str,
    channel: &ChannelPair,
) -> Result<Session> {
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
                    label: channel.label,
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
    drain_notifications(
        notifications,
        session.tx_char.uuid,
        Duration::from_millis(30),
    )
    .await;
    session
        .peripheral
        .write(
            &session.rx_char,
            payload,
            write_type(&session.rx_char),
        )
        .await?;
    let response = match time::timeout(RESPONSE_WAIT, notifications.next()).await {
        Ok(Some(n)) if n.uuid == session.tx_char.uuid => Some(n.value),
        Ok(Some(_)) => None,
        Ok(None) | Err(_) => None,
    };
    time::sleep(INTER_CMD_GAP).await;
    Ok(response)
}

pub async fn send_handshake(
    session: &Session,
    notifications: &mut (impl StreamExt<Item = btleplug::api::ValueNotification> + Unpin),
    frames: &[&[u8]],
) -> Result<()> {
    for (i, packet) in frames.iter().enumerate() {
        session
            .peripheral
            .write(
                &session.rx_char,
                packet,
                write_type(&session.rx_char),
            )
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
            .write(
                &session.rx_char,
                payload,
                write_type(&session.rx_char),
            )
            .await?;
        if let Ok(Some(n)) =
            time::timeout(Duration::from_millis(30), notifications.next()).await
        {
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
