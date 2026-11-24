//! Raw MIDI over a serial port: a source (read bytes, parse, emit messages) and a sink (write message bytes).
//! This is what Hairless MIDI to Serial does; here each direction is its own route.
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;
use studio_core::bridge::{Backoff, MidiSink, SourceEvent};
use studio_core::midi_stream::StreamParser;

pub struct SerialSink { port: Box<dyn serialport::SerialPort> }
impl SerialSink {
    pub fn open(name: &str, baud: u32) -> Result<Self, String> {
        let p = serialport::new(name, baud).timeout(Duration::from_millis(200)).open().map_err(|e| format!("{}: {}", name, e))?;
        Ok(SerialSink { port: p })
    }
}
impl MidiSink for SerialSink {
    fn send(&mut self, m: &[u8]) -> Result<(), String> { self.port.write_all(m).and_then(|_| self.port.flush()).map_err(|e| e.to_string()) }
}

/// Always fails with a fixed message: stands in for a sink that could not be opened, so the route shows the reason.
pub struct FailSink(pub String);
impl MidiSink for FailSink { fn send(&mut self, _: &[u8]) -> Result<(), String> { Err(self.0.clone()) } }

/// Reads `port` at `baud` until `stop` is set. Reconnects with 1..5 s backoff (so unplug and replug just works).
pub fn spawn_source(id: String, port: String, baud: u32, tx: Sender<(String, SourceEvent)>, stop: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let send = |e: SourceEvent| { let _ = tx.send((id.clone(), e)); };
        let mut backoff = Backoff::default();
        while !stop.load(Ordering::Relaxed) {
            send(SourceEvent::Connecting);
            match serialport::new(&port, baud).timeout(Duration::from_millis(100)).open() {
                Err(e) => { send(SourceEvent::Error(format!("{}: {}", port, e))); }
                Ok(mut p) => {
                    let _ = p.write_data_terminal_ready(true);
                    backoff.reset();
                    send(SourceEvent::Connected { name: format!("{} at {} baud", port, baud) });
                    let mut parser = StreamParser::new();
                    let mut buf = [0u8; 256];
                    let reason = loop {
                        if stop.load(Ordering::Relaxed) { break "stopped".to_string(); }
                        match std::io::Read::read(&mut p, &mut buf) {
                            Ok(0) => break "port closed".to_string(),
                            Ok(n) => for &b in &buf[..n] { if let Some(m) = parser.push(b) { send(SourceEvent::Message(m)); } },
                            Err(e) if matches!(e.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted) => {}
                            Err(e) => break e.to_string(),
                        }
                    };
                    send(SourceEvent::Disconnected { reason });
                }
            }
            let mut left = backoff.next_delay();
            while left > Duration::ZERO && !stop.load(Ordering::Relaxed) { let s = left.min(Duration::from_millis(100)); std::thread::sleep(s); left -= s; }
        }
    });
}

pub fn list_ports() -> Vec<String> {
    serialport::available_ports().unwrap_or_default().into_iter().map(|p| p.port_name).collect()
}
