//! Runs the Studio's JavaScript `bleParse` (copied verbatim from feyonics-studio.html) and the Rust `ble_midi::parse`
//! over the same random packets and requires identical output. Skipped when node is not installed.
use std::io::Write;
use std::process::{Command, Stdio};
use studio_core::ble_midi;

const JS: &str = r#"
function bleParse(v) {
  const d = new Uint8Array(v.buffer, v.byteOffset, v.byteLength), out = []; let i = 1, st = 0;
  while (i < d.length) {
    if (!(d[i] & 0x80)) { i++; continue; }
    i++;
    if (i < d.length && (d[i] & 0x80)) st = d[i++];
    if (st >= 0xF0) { while (i < d.length && !(d[i] & 0x80)) i++; continue; }
    const need = (st & 0xF0) === 0xC0 || (st & 0xF0) === 0xD0 ? 1 : 2, m = [st];
    while (m.length <= need && i < d.length && !(d[i] & 0x80)) m.push(d[i++]);
    if (m.length === need + 1) out.push(m);
  }
  return out;
}
const lines = require('fs').readFileSync(0, 'utf8').split('\n').filter(Boolean);
for (const l of lines) {
  const b = Uint8Array.from(l.match(/../g) || [], x => parseInt(x, 16));
  console.log(JSON.stringify(bleParse(new DataView(b.buffer))));
}
"#;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 { self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; self.0 }
}

#[test]
fn rust_parser_matches_js_parser() {
    if Command::new("node").arg("--version").output().is_err() { eprintln!("node not found, skipping parity test"); return; }
    let mut r = Rng(0x9E3779B97F4A7C15);
    let mut packets: Vec<Vec<u8>> = Vec::new();
    for n in 0..4000 {
        let len = 1 + (r.next() % 24) as usize; // empty packets would be blank lines in the node input
        let mut p = Vec::with_capacity(len);
        for _ in 0..len {
            let x = r.next();
            // bias towards status/timestamp bytes so interesting paths are exercised
            let b = match x % 4 { 0 => 0x80 | (x >> 8) as u8 & 0x7F, 1 => 0x90 | ((x >> 8) as u8 & 0x0F), _ => (x >> 8) as u8 };
            p.push(b);
        }
        if n % 7 == 0 && !p.is_empty() { p[0] = 0x80 | (p[0] & 0x7F); }
        packets.push(p);
    }
    let input: String = packets.iter().map(|p| p.iter().map(|b| format!("{:02x}", b)).collect::<String>() + "\n").collect();
    let mut child = Command::new("node").arg("-e").arg(JS).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    let js: Vec<Vec<Vec<u8>>> = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(js.len(), packets.len());
    let mut nonempty = 0;
    for (p, j) in packets.iter().zip(js.iter()) {
        let rs = ble_midi::parse(p);
        assert_eq!(&rs, j, "mismatch for packet {:02x?}", p);
        if !rs.is_empty() { nonempty += 1; }
    }
    assert!(nonempty > 200, "test not exercising real messages ({})", nonempty);
}
