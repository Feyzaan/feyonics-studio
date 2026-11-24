//! Which firmware is on the USB serial port? Both answer `KM HELLO`, with different shapes:
//!   MIDI  firmware 2.x: `KM HELLO 2 4 4 4 <fw> 596`   (protocol 2, 4 layers, 4x4 keys, 596-byte config blob)
//!   macro firmware 1.x: `KM HELLO 1 12 4 4 <fw>`      (protocol 1, 12 layers, 4x4 keys)
//! Older macro firmware (4 layers) also says protocol 1.
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FirmwareKind { Midi, Macro }

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hello {
    pub kind: FirmwareKind,
    pub proto: u32,
    pub layers: u32,
    pub rows: u32,
    pub cols: u32,
    pub fw: Option<String>,
    pub cfg_size: Option<u32>,
}

pub const HELLO_COMMAND: &str = "KM HELLO";

pub fn parse_hello(line: &str) -> Option<Hello> {
    let p: Vec<&str> = line.split_whitespace().collect();
    if p.len() < 6 || p[0] != "KM" || p[1] != "HELLO" { return None; }
    let n = |i: usize| p.get(i).and_then(|s| s.parse::<u32>().ok());
    let proto = n(2)?;
    let (layers, rows, cols) = (n(3)?, n(4)?, n(5)?);
    let fw = p.get(6).map(|s| s.to_string());
    let cfg_size = n(7);
    let kind = if proto >= 2 && cfg_size.is_some() { FirmwareKind::Midi } else if proto == 1 { FirmwareKind::Macro } else { return None };
    Some(Hello { kind, proto, layers, rows, cols, fw, cfg_size })
}

pub fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let v = |s: &str| -> Vec<u32> { s.split('.').map(|x| x.parse().unwrap_or(0)).collect() };
    let (x, y) = (v(a), v(b));
    for i in 0..3 {
        let o = x.get(i).copied().unwrap_or(0).cmp(&y.get(i).copied().unwrap_or(0));
        if o != std::cmp::Ordering::Equal { return o; }
    }
    std::cmp::Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn midi() {
        let h = parse_hello("KM HELLO 2 4 4 4 2.0.0 596").unwrap();
        assert_eq!((h.kind, h.cfg_size, h.fw.as_deref()), (FirmwareKind::Midi, Some(596), Some("2.0.0")));
    }
    #[test] fn macro_pad() {
        let h = parse_hello("KM HELLO 1 12 4 4 1.24.2").unwrap();
        assert_eq!((h.kind, h.layers), (FirmwareKind::Macro, 12));
    }
    #[test] fn old_macro_four_layers() { assert_eq!(parse_hello("KM HELLO 1 4 4 4 1.4.0").unwrap().layers, 4); }
    #[test] fn very_old_no_version() { assert_eq!(parse_hello("KM HELLO 1 3 4 4").unwrap_or_else(|| panic!()).fw, None); }
    #[test] fn rejects() { assert!(parse_hello("KM OK").is_none()); assert!(parse_hello("garbage").is_none()); }
    #[test] fn versions() { assert!(version_cmp("1.24.2", "1.9.0").is_gt()); assert!(version_cmp("2.0.0", "2.0").is_eq()); }
}
