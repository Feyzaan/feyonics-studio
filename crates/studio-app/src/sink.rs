use studio_core::bridge::MidiSink;

pub type BoxSink = Box<dyn MidiSink + Send>;

/// Logs instead of sending; used by `--sim` and on non-Windows hosts.
pub struct LogSink;
impl MidiSink for LogSink {
    fn send(&mut self, m: &[u8]) -> Result<(), String> { println!("midi out: {:02x?}", m); Ok(()) }
}

#[cfg(windows)]
pub mod win {
    use super::*;
    use midir::{MidiOutput, MidiOutputConnection};

    pub fn list_ports() -> Vec<String> {
        MidiOutput::new("Feyonics Studio").map(|o| o.ports().iter().filter_map(|p| o.port_name(p).ok()).collect()).unwrap_or_default()
    }
    pub fn list_inputs() -> Vec<String> {
        midir::MidiInput::new("Feyonics Studio").map(|o| o.ports().iter().filter_map(|p| o.port_name(p).ok()).collect()).unwrap_or_default()
    }
    /// What is installed on this PC, to decide how virtual ports can be provided (see studio_core::ports).
    pub fn facts() -> studio_core::ports::SystemFacts {
        let exists = |p: &str| std::path::Path::new(p).exists();
        let build = std::process::Command::new("cmd").args(["/C", "ver"]).output().ok().and_then(|o| {
            let t = String::from_utf8_lossy(&o.stdout).to_string();
            t.split(|c: char| !(c.is_ascii_digit() || c == '.')).filter(|w| w.matches('.').count() >= 2).next().and_then(|v| v.split('.').nth(2).and_then(|b| b.parse().ok()))
        });
        studio_core::ports::SystemFacts {
            windows_build: build,
            midi_services_installed: exists(r"C:\Program Files\Windows MIDI Services") || exists(r"C:\Windows\System32\Midi2"),
            loopmidi_installed: exists(r"C:\Program Files (x86)\Tobias Erichsen\loopMIDI") || exists(r"C:\Program Files\Tobias Erichsen\loopMIDI"),
        }
    }
    pub struct WinSink(MidiOutputConnection);
    impl MidiSink for WinSink {
        fn send(&mut self, m: &[u8]) -> Result<(), String> { self.0.send(m).map_err(|e| e.to_string()) }
    }
    /// Opens the first output whose name contains `name` (case-insensitive). The port must already exist:
    /// a Windows MIDI Services loopback endpoint (Win 11) or a loopMIDI port (Win 10). Studio does not create ports yet.
    pub fn open(name: &str) -> Result<BoxSink, String> {
        let out = MidiOutput::new("Feyonics Studio").map_err(|e| e.to_string())?;
        let want = name.to_lowercase();
        let port = out.ports().into_iter().find(|p| out.port_name(p).map(|n| n.to_lowercase().contains(&want)).unwrap_or(false))
            .ok_or_else(|| format!("No MIDI output named \"{}\". Available: {}", name, list_ports().join(", ")))?;
        let conn = out.connect(&port, "Feyonics bridge").map_err(|e| e.to_string())?;
        Ok(Box::new(WinSink(conn)))
    }
}
