//! Raw MIDI byte stream -> messages. Used for serial MIDI (what Hairless MIDI to Serial does) and any other plain byte source.
//! Handles running status, real-time bytes inside other messages, system common and system exclusive.
//! Port of the `MidiParser` in the web version; `tests/stream_parity.rs` runs both over the same random bytes.
use std::io::Read;

#[derive(Debug, Default, Clone)]
pub struct StreamParser { status: u8, need: usize, buf: Vec<u8>, sysex: Option<Vec<u8>> }

impl StreamParser {
    pub fn new() -> Self { Self::default() }

    /// Feed one byte; returns a message when one is complete.
    pub fn push(&mut self, b: u8) -> Option<Vec<u8>> {
        if b >= 0xF8 { return Some(vec![b]); }                       // real-time: may appear anywhere, does not disturb running status
        if b & 0x80 != 0 {
            if b == 0xF0 { self.sysex = Some(vec![b]); self.status = 0; return None; }
            if b == 0xF7 { return self.sysex.take().map(|mut s| { s.push(b); s }); }
            self.sysex = None;
            if (0xF1..=0xF6).contains(&b) {
                if b == 0xF6 { self.status = 0; return Some(vec![b]); }
                self.status = b; self.need = if b == 0xF1 || b == 0xF3 { 1 } else { 2 }; self.buf = vec![b]; return None;
            }
            self.status = b; self.need = if (b & 0xF0) == 0xC0 || (b & 0xF0) == 0xD0 { 1 } else { 2 }; self.buf = vec![b]; return None;
        }
        if let Some(s) = self.sysex.as_mut() { s.push(b); return None; }
        if self.status == 0 { return None; }
        self.buf.push(b);
        if self.buf.len() == self.need + 1 {
            let m = self.buf.clone();
            if self.status >= 0xF0 { self.status = 0; self.buf.clear(); } else { self.buf = vec![self.status]; }
            return Some(m);
        }
        None
    }
}

/// Reads until EOF or error, calling `on_msg` for each complete message. Returns the error that stopped it, if any.
pub fn pump<R: Read>(mut r: R, mut on_msg: impl FnMut(Vec<u8>)) -> std::io::Result<()> {
    let mut p = StreamParser::new();
    let mut chunk = [0u8; 256];
    loop {
        match r.read(&mut chunk) {
            Ok(0) => return Ok(()),
            Ok(n) => for &b in &chunk[..n] { if let Some(m) = p.push(b) { on_msg(m); } },
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut || e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(bytes: &[u8]) -> Vec<Vec<u8>> { let mut p = StreamParser::new(); bytes.iter().filter_map(|b| p.push(*b)).collect() }
    #[test]
    fn running_status_realtime_program_sysex_mtc() {
        let got = run(&[0x90, 60, 100, 62, 100, 0xF8, 0x80, 60, 0, 0xC0, 5, 0xF0, 1, 2, 0xF7, 0xB0, 64, 127, 0xF1, 0x10, 0x91, 70, 80]);
        assert_eq!(got, vec![vec![0x90, 60, 100], vec![0x90, 62, 100], vec![0xF8], vec![0x80, 60, 0], vec![0xC0, 5], vec![0xF0, 1, 2, 0xF7], vec![0xB0, 64, 127], vec![0xF1, 0x10], vec![0x91, 70, 80]]);
    }
    #[test]
    fn realtime_inside_a_message_does_not_break_it() { assert_eq!(run(&[0x90, 60, 0xF8, 100]), vec![vec![0xF8], vec![0x90, 60, 100]]); }
    #[test]
    fn garbage_before_first_status_ignored() { assert_eq!(run(&[1, 2, 3, 0x90, 60, 100]), vec![vec![0x90, 60, 100]]); }
    #[test]
    fn new_status_abandons_unfinished_message() { assert_eq!(run(&[0x90, 60, 0x80, 61, 0]), vec![vec![0x80, 61, 0]]); }
    #[test]
    fn pump_reads_until_eof() {
        let mut got = vec![];
        pump(std::io::Cursor::new(vec![0x90, 60, 100, 0x80, 60, 0]), |m| got.push(m)).unwrap();
        assert_eq!(got.len(), 2);
    }
}
