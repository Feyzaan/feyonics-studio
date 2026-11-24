//! Runs the web page's `MidiParser` (copied verbatim from feyonics-studio-web.html) and `StreamParser` over the same random bytes
//! and requires identical messages. Skipped when node is not installed.
use std::io::Write;
use std::process::{Command, Stdio};
use studio_core::midi_stream::StreamParser;

const JS: &str = r#"
function MidiParser(emit){let st=0,need=0,buf=[],sys=null;
  return b=>{
    if(b>=0xF8){emit([b]);return}
    if(b&0x80){
      if(b===0xF0){sys=[b];st=0;return}
      if(b===0xF7){if(sys){sys.push(b);emit(sys)}sys=null;return}
      sys=null;
      if(b>=0xF1&&b<=0xF6){if(b===0xF6){emit([b]);st=0;return}st=b;need=(b===0xF1||b===0xF3)?1:2;buf=[b];return}
      st=b;need=((b&0xF0)===0xC0||(b&0xF0)===0xD0)?1:2;buf=[b];return}
    if(sys){sys.push(b);return}
    if(!st)return;
    buf.push(b);
    if(buf.length===need+1){emit(buf.slice());if(st>=0xF0){st=0;buf=[]}else buf=[st]}}}

const lines = require('fs').readFileSync(0, 'utf8').split('\n').filter(Boolean);
for (const l of lines) {
  const b = Uint8Array.from(l.match(/../g) || [], x => parseInt(x, 16)), out = [];
  const f = MidiParser(m => out.push(m)); b.forEach(f);
  console.log(JSON.stringify(out));
}
"#;

struct Rng(u64);
impl Rng { fn next(&mut self) -> u64 { self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; self.0 } }

#[test]
fn rust_stream_parser_matches_js() {
    if Command::new("node").arg("--version").output().is_err() { eprintln!("node not found, skipping"); return; }
    let mut r = Rng(0xD1B54A32D192ED03);
    let mut streams: Vec<Vec<u8>> = Vec::new();
    for _ in 0..4000 {
        let len = 1 + (r.next() % 40) as usize;
        let s: Vec<u8> = (0..len).map(|_| { let x = r.next(); match x % 6 { 0 => 0x80 | ((x >> 8) as u8 & 0x7F), 1 => 0x90 | ((x >> 8) as u8 & 0x0F), 2 => 0xF0 | ((x >> 8) as u8 & 0x0F), _ => (x >> 8) as u8 & 0x7F } }).collect();
        streams.push(s);
    }
    let input: String = streams.iter().map(|p| p.iter().map(|b| format!("{:02x}", b)).collect::<String>() + "\n").collect();
    let mut child = Command::new("node").arg("-e").arg(JS).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap(); assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    let js: Vec<Vec<Vec<u8>>> = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(js.len(), streams.len());
    let mut nonempty = 0;
    for (s, j) in streams.iter().zip(js.iter()) {
        let mut p = StreamParser::new();
        let rs: Vec<Vec<u8>> = s.iter().filter_map(|b| p.push(*b)).collect();
        assert_eq!(&rs, j, "mismatch for stream {:02x?}", s);
        if !rs.is_empty() { nonempty += 1; }
    }
    assert!(nonempty > 2000, "test not exercising real messages ({})", nonempty);
}
