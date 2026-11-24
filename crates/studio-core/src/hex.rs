//! Hex helpers matching the pad's wire format ("-" means empty).
pub fn to_hex(s: &str) -> String {
    if s.is_empty() { return "-".into(); }
    s.bytes().map(|b| format!("{:02X}", b)).collect()
}
pub fn bytes_to_hex(b: &[u8]) -> String { b.iter().map(|x| format!("{:02X}", x)).collect() }
pub fn from_hex(h: &str) -> Option<Vec<u8>> {
    if h == "-" || h.is_empty() { return Some(vec![]); }
    if h.len() % 2 != 0 { return None; }
    (0..h.len()).step_by(2).map(|i| u8::from_str_radix(&h[i..i + 2], 16).ok()).collect()
}
pub fn text_from_hex(h: &str) -> String {
    from_hex(h).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn round_trip() { assert_eq!(text_from_hex(&to_hex("Hi!")), "Hi!"); assert_eq!(to_hex(""), "-"); assert_eq!(from_hex("zz"), None); }
}
