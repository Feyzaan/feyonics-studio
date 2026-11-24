//! MIDI input port as a route source (for MIDI port -> serial, or port to port).
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use studio_core::bridge::SourceEvent;

#[cfg(not(windows))]
pub fn spawn_source(id: String, port: String, tx: Sender<(String, SourceEvent)>, _stop: Arc<AtomicBool>) {
    let _ = tx.send((id, SourceEvent::Error(format!("MIDI input \"{}\": real MIDI ports are only built for Windows", port))));
}

#[cfg(windows)]
pub fn spawn_source(id: String, port: String, tx: Sender<(String, SourceEvent)>, stop: Arc<AtomicBool>) {
    use midir::{Ignore, MidiInput};
    use std::sync::atomic::Ordering;
    std::thread::spawn(move || {
        let mut backoff = studio_core::bridge::Backoff::default();
        while !stop.load(Ordering::Relaxed) {
            let _ = tx.send((id.clone(), SourceEvent::Connecting));
            let attempt = (|| -> Result<midir::MidiInputConnection<()>, String> {
                let mut inp = MidiInput::new("Feyonics Studio").map_err(|e| e.to_string())?;
                inp.ignore(Ignore::None);
                let want = port.to_lowercase();
                let p = inp.ports().into_iter().find(|p| inp.port_name(p).map(|n| n.to_lowercase().contains(&want)).unwrap_or(false)).ok_or_else(|| format!("No MIDI input named \"{}\"", port))?;
                let (tx2, id2) = (tx.clone(), id.clone());
                inp.connect(&p, "Feyonics route", move |_, data, _| { let _ = tx2.send((id2.clone(), SourceEvent::Message(data.to_vec()))); }, ()).map_err(|e| e.to_string())
            })();
            match attempt {
                Ok(conn) => {
                    backoff.reset();
                    let _ = tx.send((id.clone(), SourceEvent::Connected { name: port.clone() }));
                    while !stop.load(Ordering::Relaxed) { std::thread::sleep(std::time::Duration::from_millis(100)); }
                    drop(conn);
                    let _ = tx.send((id.clone(), SourceEvent::Disconnected { reason: "stopped".into() }));
                }
                Err(e) => { let _ = tx.send((id.clone(), SourceEvent::Error(e))); }
            }
            let mut left = backoff.next_delay();
            while left > std::time::Duration::ZERO && !stop.load(Ordering::Relaxed) { let s = left.min(std::time::Duration::from_millis(100)); std::thread::sleep(s); left -= s; }
        }
    });
}
