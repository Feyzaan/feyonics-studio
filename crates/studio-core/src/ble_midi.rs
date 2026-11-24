//! BLE-MIDI packet parser. A line-for-line port of `bleParse` in feyonics-studio.html (the JS is the reference
//! and `tests/parity.rs` checks both agree on random packets).
//!
//! Behaviour, quirks included: byte 0 is the header; every byte with bit 7 set starts a timestamp, optionally
//! followed by a status byte; running status lasts for the packet; stray data bytes are skipped; statuses >= 0xF0
//! (SysEx, realtime) are dropped. Because the JS starts with status 0, malformed packets can produce a "message"
//! whose first byte is < 0x80; `forwardable` filters those before anything reaches a MIDI port.

pub const SERVICE_UUID: &str = "03b80e5a-ede8-4b33-a751-6ce34ec4c700";
pub const CHAR_UUID: &str = "7772e5db-3868-4112-a1a9-f2669d106bf3";

pub fn parse(d: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 1usize;
    let mut st: u8 = 0;
    while i < d.len() {
        if d[i] & 0x80 == 0 { i += 1; continue; } // stray data byte
        i += 1; // timestamp
        if i < d.len() && d[i] & 0x80 != 0 { st = d[i]; i += 1; }
        if st >= 0xF0 {
            while i < d.len() && d[i] & 0x80 == 0 { i += 1; }
            continue;
        }
        let need = if (st & 0xF0) == 0xC0 || (st & 0xF0) == 0xD0 { 1 } else { 2 };
        let mut m = vec![st];
        while m.len() <= need && i < d.len() && d[i] & 0x80 == 0 { m.push(d[i]); i += 1; }
        if m.len() == need + 1 { out.push(m); }
    }
    out
}

/// True for a well-formed channel message that is safe to hand to a MIDI port.
pub fn forwardable(m: &[u8]) -> bool {
    if m.is_empty() || m[0] < 0x80 || m[0] >= 0xF0 { return false; }
    let need = if (m[0] & 0xF0) == 0xC0 || (m[0] & 0xF0) == 0xD0 { 1 } else { 2 };
    m.len() == need + 1 && m[1..].iter().all(|b| *b < 0x80)
}

/// For byte-stream sources: channel messages as above, plus system common and real-time messages. System exclusive is dropped.
pub fn forwardable_stream(m: &[u8]) -> bool {
    if m.is_empty() || m[0] < 0x80 { return false; }
    match m[0] {
        0xF0 | 0xF7 => false,
        0xF8..=0xFF => m.len() == 1,
        0xF6 => m.len() == 1,
        0xF1 | 0xF3 => m.len() == 2 && m[1] < 0x80,
        0xF2 => m.len() == 3 && m[1..].iter().all(|b| *b < 0x80),
        0xF4 | 0xF5 => false,
        _ => forwardable(m),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn note_on() { assert_eq!(parse(&[0x80, 0x80, 0x90, 60, 100]), vec![vec![0x90, 60, 100]]); }
    #[test]
    fn two_messages_new_timestamps() {
        let p = [0x80, 0x81, 0x90, 60, 100, 0x82, 0x80, 60, 0];
        assert_eq!(parse(&p), vec![vec![0x90, 60, 100], vec![0x80, 60, 0]]);
    }
    #[test]
    fn program_change_one_data_byte() { assert_eq!(parse(&[0x80, 0x80, 0xC0, 5]), vec![vec![0xC0, 5]]); }
    #[test]
    fn sysex_dropped() { assert!(parse(&[0x80, 0x80, 0xF0, 0x7F, 0x06, 0x02, 0x80, 0xF7]).is_empty()); }
    #[test]
    fn truncated_dropped() { assert!(parse(&[0x80, 0x80, 0x90, 60]).is_empty()); }
    #[test]
    fn forwardable_filters_status_zero() {
        assert!(!forwardable(&[0, 1, 2]));
        assert!(forwardable(&[0x90, 60, 100]));
        assert!(!forwardable(&[0xF8]));
    }
}
