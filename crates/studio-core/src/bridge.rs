//! Bluetooth-to-virtual-MIDI bridge core. Synchronous and OS-free: a BLE backend feeds `BleEvent`s in,
//! decoded MIDI goes out through a `MidiSink`. Windows backends live in a separate crate.
use crate::ble_midi;
use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub enum SourceEvent {
    Scanning,
    Connecting,
    Connected { name: String },
    Packet(Vec<u8>),          // one BLE-MIDI packet
    Message(Vec<u8>),         // one finished MIDI message from a byte-stream source (serial, MIDI port)
    Disconnected { reason: String },
    Error(String),
}

/// Older name, kept so existing code and tests read the same.
pub type BleEvent = SourceEvent;

pub trait MidiSink {
    fn send(&mut self, msg: &[u8]) -> Result<(), String>;
}

impl<T: MidiSink + ?Sized> MidiSink for Box<T> {
    fn send(&mut self, m: &[u8]) -> Result<(), String> { (**self).send(m) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase { Idle, Scanning, Connecting, Connected }

#[derive(Debug, Clone, Default, Serialize)]
pub struct Stats {
    pub packets: u64,
    pub messages: u64,
    pub dropped: u64,
    pub sink_errors: u64,
    pub reconnects: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BridgeState {
    pub phase: Phase,
    pub device: Option<String>,
    pub last_error: Option<String>,
    pub stats: Stats,
}

/// 1s, 2s, 3s, 4s, 5s then stays at 5s; reset on a successful connection.
#[derive(Debug, Default)]
pub struct Backoff { n: u32 }
impl Backoff {
    pub fn next_delay(&mut self) -> Duration { self.n = (self.n + 1).min(5); Duration::from_secs(self.n as u64) }
    pub fn reset(&mut self) { self.n = 0; }
}

pub struct Bridge<S: MidiSink> {
    sink: S,
    state: BridgeState,
    held: [u128; 16],      // bit n of held[ch] = note n is on
    sustain: [bool; 16],
    active: u16,           // channels that have sent anything
    ever_connected: bool,
}

impl<S: MidiSink> Bridge<S> {
    pub fn new(sink: S) -> Self {
        Bridge {
            sink,
            state: BridgeState { phase: Phase::Idle, device: None, last_error: None, stats: Stats::default() },
            held: [0; 16], sustain: [false; 16], active: 0, ever_connected: false,
        }
    }
    pub fn state(&self) -> &BridgeState { &self.state }
    pub fn sink(&self) -> &S { &self.sink }
    pub fn sink_mut(&mut self) -> &mut S { &mut self.sink }

    pub fn handle(&mut self, ev: BleEvent) {
        match ev {
            BleEvent::Scanning => self.state.phase = Phase::Scanning,
            BleEvent::Connecting => self.state.phase = Phase::Connecting,
            BleEvent::Connected { name } => {
                if self.ever_connected { self.state.stats.reconnects += 1; }
                self.ever_connected = true;
                self.state.phase = Phase::Connected;
                self.state.device = Some(name);
                self.state.last_error = None;
            }
            BleEvent::Packet(p) => self.packet(&p),
            BleEvent::Message(m) => self.message(&m),
            BleEvent::Disconnected { reason } => {
                self.release_all();
                self.state.phase = Phase::Idle;
                self.state.device = None;
                self.state.last_error = Some(reason);
            }
            BleEvent::Error(e) => self.state.last_error = Some(e),
        }
    }

    fn packet(&mut self, p: &[u8]) {
        self.state.stats.packets += 1;
        for m in ble_midi::parse(p) {
            if !ble_midi::forwardable(&m) { self.state.stats.dropped += 1; continue; }
            self.track(&m);
            match self.sink.send(&m) {
                Ok(()) => self.state.stats.messages += 1,
                Err(e) => { self.state.stats.sink_errors += 1; self.state.last_error = Some(e); }
            }
        }
    }

    fn message(&mut self, m: &[u8]) {
        if !ble_midi::forwardable_stream(m) { self.state.stats.dropped += 1; return; }
        if m[0] < 0xF0 { self.track(m); }
        match self.sink.send(m) {
            Ok(()) => self.state.stats.messages += 1,
            Err(e) => { self.state.stats.sink_errors += 1; self.state.last_error = Some(e); }
        }
    }

    fn track(&mut self, m: &[u8]) {
        let ch = (m[0] & 0x0F) as usize;
        self.active |= 1 << ch;
        match m[0] & 0xF0 {
            0x90 if m.len() == 3 && m[2] > 0 => self.held[ch] |= 1u128 << (m[1] & 0x7F),
            0x90 | 0x80 if m.len() == 3 => self.held[ch] &= !(1u128 << (m[1] & 0x7F)),
            0xB0 if m.len() == 3 && m[1] == 64 => self.sustain[ch] = m[2] >= 64,
            _ => {}
        }
    }

    /// On link loss: NoteOff for held notes, sustain off, All Notes Off on active channels, so nothing hangs in the DAW.
    pub fn release_all(&mut self) {
        for ch in 0..16usize {
            for n in 0..128u32 {
                if self.held[ch] & (1u128 << n) != 0 { let _ = self.sink.send(&[0x80 | ch as u8, n as u8, 0]); }
            }
            if self.sustain[ch] { let _ = self.sink.send(&[0xB0 | ch as u8, 64, 0]); }
            if self.active & (1 << ch) != 0 { let _ = self.sink.send(&[0xB0 | ch as u8, 123, 0]); }
            self.held[ch] = 0; self.sustain[ch] = false;
        }
        self.active = 0;
    }
}

/// Test helper / "send test note" feature.
#[derive(Default, Debug)]
pub struct MemorySink { pub sent: Vec<Vec<u8>>, pub fail: bool }
impl MidiSink for MemorySink {
    fn send(&mut self, m: &[u8]) -> Result<(), String> {
        if self.fail { return Err("port closed".into()); }
        self.sent.push(m.to_vec()); Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pkt(bytes: &[u8]) -> BleEvent { BleEvent::Packet(bytes.to_vec()) }

    #[test]
    fn forwards_notes() {
        let mut b = Bridge::new(MemorySink::default());
        b.handle(BleEvent::Connected { name: "Feyonics".into() });
        b.handle(pkt(&[0x80, 0x80, 0x90, 60, 100]));
        assert_eq!(b.sink().sent, vec![vec![0x90, 60, 100]]);
        assert_eq!(b.state().stats.messages, 1);
        assert_eq!(b.state().phase, Phase::Connected);
    }
    #[test]
    fn disconnect_releases_held_notes_and_sustain() {
        let mut b = Bridge::new(MemorySink::default());
        b.handle(pkt(&[0x80, 0x80, 0x90, 60, 100]));
        b.handle(pkt(&[0x80, 0x80, 0xB0, 64, 127]));
        b.sink_mut().sent.clear();
        b.handle(BleEvent::Disconnected { reason: "link lost".into() });
        let s = &b.sink().sent;
        assert!(s.contains(&vec![0x80, 60, 0]));
        assert!(s.contains(&vec![0xB0, 64, 0]));
        assert!(s.contains(&vec![0xB0, 123, 0]));
        assert_eq!(b.state().phase, Phase::Idle);
        assert_eq!(b.state().last_error.as_deref(), Some("link lost"));
        b.sink_mut().sent.clear();
        b.handle(BleEvent::Disconnected { reason: "again".into() });
        assert!(b.sink().sent.is_empty()); // nothing left to release
    }
    #[test]
    fn released_note_not_repeated() {
        let mut b = Bridge::new(MemorySink::default());
        b.handle(pkt(&[0x80, 0x80, 0x90, 60, 100]));
        b.handle(pkt(&[0x80, 0x80, 0x80, 60, 0]));
        b.sink_mut().sent.clear();
        b.release_all();
        assert!(!b.sink().sent.contains(&vec![0x80, 60, 0]));
    }
    #[test]
    fn note_on_velocity_zero_counts_as_off() {
        let mut b = Bridge::new(MemorySink::default());
        b.handle(pkt(&[0x80, 0x80, 0x90, 61, 90]));
        b.handle(pkt(&[0x80, 0x80, 0x90, 61, 0]));
        b.sink_mut().sent.clear();
        b.release_all();
        assert!(!b.sink().sent.contains(&vec![0x80, 61, 0]));
    }
    #[test]
    fn sink_errors_counted_not_fatal() {
        let mut b = Bridge::new(MemorySink { fail: true, ..Default::default() });
        b.handle(pkt(&[0x80, 0x80, 0x90, 60, 100]));
        assert_eq!(b.state().stats.sink_errors, 1);
        assert_eq!(b.state().last_error.as_deref(), Some("port closed"));
    }
    #[test]
    fn sysex_and_junk_dropped() {
        let mut b = Bridge::new(MemorySink::default());
        b.handle(pkt(&[0x80, 0x80, 0xF0, 1, 2, 0xF7]));
        assert!(b.sink().sent.is_empty());
    }
    #[test]
    fn backoff_ramps_and_resets() {
        let mut k = Backoff::default();
        let v: Vec<u64> = (0..7).map(|_| k.next_delay().as_secs()).collect();
        assert_eq!(v, vec![1, 2, 3, 4, 5, 5, 5]);
        k.reset();
        assert_eq!(k.next_delay().as_secs(), 1);
    }
    #[test]
    fn reconnect_counted() {
        let mut b = Bridge::new(MemorySink::default());
        b.handle(BleEvent::Connected { name: "a".into() });
        b.handle(BleEvent::Disconnected { reason: "x".into() });
        b.handle(BleEvent::Connected { name: "a".into() });
        assert_eq!(b.state().stats.reconnects, 1);
    }
    #[test]
    fn stream_messages_forward_realtime_and_drop_sysex() {
        let mut b = Bridge::new(MemorySink::default());
        b.handle(SourceEvent::Message(vec![0x90, 60, 100]));
        b.handle(SourceEvent::Message(vec![0xF8]));
        b.handle(SourceEvent::Message(vec![0xF0, 1, 0xF7]));
        assert_eq!(b.sink().sent, vec![vec![0x90, 60, 100], vec![0xF8]]);
        assert_eq!(b.state().stats.dropped, 1);
        b.sink_mut().sent.clear(); b.release_all();
        assert!(b.sink().sent.contains(&vec![0x80, 60, 0]));
    }
}
