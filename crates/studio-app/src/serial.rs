//! USB serial probing: ask each USB serial port `KM HELLO` and classify the answer, so Studio can pick the right editor.
use serde::Serialize;
use std::io::{Read, Write};
use std::time::{Duration, Instant};
use studio_core::firmware::{parse_hello, FirmwareKind, Hello, HELLO_COMMAND};

#[derive(Debug, Serialize)]
pub struct Probe {
    pub port: String,
    pub product: String,
    pub kind: Option<FirmwareKind>,
    pub hello: Option<Hello>,
    pub error: Option<String>,
}

pub fn probe_all() -> Vec<Probe> {
    let ports = serialport::available_ports().unwrap_or_default();
    ports.into_iter().filter_map(|p| match &p.port_type {
        serialport::SerialPortType::UsbPort(u) => Some((p.port_name.clone(), u.product.clone().unwrap_or_default())),
        _ => None, // never poke Bluetooth/other serial ports
    }).map(|(name, product)| probe(&name, product)).collect()
}

fn probe(name: &str, product: String) -> Probe {
    let mut out = Probe { port: name.to_string(), product, kind: None, hello: None, error: None };
    let mut port = match serialport::new(name, 115_200).timeout(Duration::from_millis(100)).open() {
        Ok(p) => p, Err(e) => { out.error = Some(e.to_string()); return out; }
    };
    let _ = port.write_data_terminal_ready(true);
    let _ = port.write_request_to_send(true);
    if port.write_all(format!("{}\n", HELLO_COMMAND).as_bytes()).is_err() { out.error = Some("write failed".into()); return out; }
    let (mut buf, mut chunk) = (String::new(), [0u8; 256]);
    let end = Instant::now() + Duration::from_millis(900);
    while Instant::now() < end {
        match port.read(&mut chunk) { Ok(n) if n > 0 => buf.push_str(&String::from_utf8_lossy(&chunk[..n])), _ => {} }
        if let Some(h) = buf.lines().find_map(parse_hello) { out.kind = Some(h.kind); out.hello = Some(h); return out; }
    }
    out.error = Some("no KM HELLO answer (not a Feyonics pad, or busy)".into());
    out
}
