use std::sync::mpsc::Sender;
use std::time::Duration;
use studio_core::bridge::BleEvent;

/// Simulated pad: connects, plays a C major arpeggio every second, drops the link now and then (holding a note,
/// to prove the bridge releases it). Lets the whole app be exercised without Bluetooth hardware.
pub fn spawn_sim(tx: Sender<BleEvent>) {
    std::thread::spawn(move || {
        let ble = |b: &[u8]| { let mut v = vec![0x80 | 0x01]; for (i, x) in b.iter().enumerate() { if i % 3 == 0 { v.push(0x80 | 0x02); } v.push(*x); } v };
        let mut round = 0u32;
        loop {
            let _ = tx.send(BleEvent::Scanning); std::thread::sleep(Duration::from_millis(600));
            let _ = tx.send(BleEvent::Connecting); std::thread::sleep(Duration::from_millis(600));
            let _ = tx.send(BleEvent::Connected { name: "Feyonics (simulated)".into() });
            for n in [60u8, 64, 67, 72] {
                let _ = tx.send(BleEvent::Packet(ble(&[0x90, n, 100]))); std::thread::sleep(Duration::from_millis(400));
                if n != 72 { let _ = tx.send(BleEvent::Packet(ble(&[0x80, n, 0]))); }
            }
            round += 1;
            std::thread::sleep(Duration::from_millis(500));
            let _ = tx.send(BleEvent::Disconnected { reason: format!("simulated link loss #{}", round) });
            std::thread::sleep(Duration::from_secs(2));
        }
    });
}

#[cfg(windows)]
pub mod win {
    use super::*;
    use btleplug::api::{Central, CentralEvent, Manager as _, Peripheral as _, ScanFilter};
    use btleplug::platform::{Manager, Peripheral};
    use futures::StreamExt;
    use studio_core::ble_midi::{CHAR_UUID, SERVICE_UUID};
    use studio_core::bridge::Backoff;
    fn uuid(s: &str) -> uuid::Uuid { uuid::Uuid::parse_str(s).expect("valid uuid") }

    /// Background BLE worker on its own tokio runtime. Reconnects forever with 1..5 s backoff.
    pub fn spawn(tx: Sender<BleEvent>, name_filter: String) {
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio");
            rt.block_on(async move {
                let mut backoff = Backoff::default();
                loop {
                    match session(&tx, &name_filter).await {
                        Ok(()) => { backoff.reset(); let _ = tx.send(BleEvent::Disconnected { reason: "Bluetooth link closed".into() }); }
                        Err(e) => { let _ = tx.send(BleEvent::Error(e)); }
                    }
                    tokio::time::sleep(backoff.next_delay()).await;
                }
            });
        });
    }

    async fn session(tx: &Sender<BleEvent>, filter: &str) -> Result<(), String> {
        let svc = uuid(SERVICE_UUID); let chr_id = uuid(CHAR_UUID);
        let mgr = Manager::new().await.map_err(|e| e.to_string())?;
        let adapter = mgr.adapters().await.map_err(|e| e.to_string())?.into_iter().next().ok_or("No Bluetooth adapter found. Is Bluetooth on?")?;
        let _ = tx.send(BleEvent::Scanning);
        adapter.start_scan(ScanFilter { services: vec![svc] }).await.map_err(|e| e.to_string())?;
        let mut events = adapter.events().await.map_err(|e| e.to_string())?;
        let mut found: Option<Peripheral> = None;
        let want = filter.to_lowercase();
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
        while found.is_none() && tokio::time::Instant::now() < deadline {
            let ev = match tokio::time::timeout(std::time::Duration::from_secs(2), events.next()).await { Ok(Some(e)) => e, Ok(None) => break, Err(_) => continue };
            if let CentralEvent::DeviceDiscovered(id) | CentralEvent::DeviceUpdated(id) = ev {
                if let Ok(p) = adapter.peripheral(&id).await {
                    if let Ok(Some(props)) = p.properties().await {
                        let n = props.local_name.unwrap_or_default().to_lowercase();
                        if props.services.contains(&svc) && (want.is_empty() || n.contains(&want) || n.contains("feyonics")) { found = Some(p); }
                    }
                }
            }
        }
        let _ = adapter.stop_scan().await;
        let p = found.ok_or("Pad not found. Hold the pairing button on the pad and try again.")?;
        let name = p.properties().await.ok().flatten().and_then(|x| x.local_name).unwrap_or_else(|| "Feyonics pad".into());
        let _ = tx.send(BleEvent::Connecting);
        p.connect().await.map_err(|e| format!("connect: {}", e))?;
        p.discover_services().await.map_err(|e| e.to_string())?;
        let ch = p.characteristics().into_iter().find(|c| c.uuid == chr_id).ok_or("The device has no BLE-MIDI characteristic.")?;
        // Subscribing to an encrypted characteristic is what makes Windows start pairing, if the pad is not bonded yet.
        p.subscribe(&ch).await.map_err(|e| format!("subscribe (is the pad paired in Windows Bluetooth settings?): {}", e))?;
        let _ = tx.send(BleEvent::Connected { name });
        let mut notes = p.notifications().await.map_err(|e| e.to_string())?;
        while let Some(n) = notes.next().await {
            if n.uuid == chr_id { if tx.send(BleEvent::Packet(n.value)).is_err() { break; } }
            if !p.is_connected().await.unwrap_or(false) { break; }
        }
        let _ = p.disconnect().await;
        Ok(())
    }
}
