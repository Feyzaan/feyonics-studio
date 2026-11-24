# Feyonics Studio: native app architecture

One idea: every connection is a **route** (`source -> sink`). The Bluetooth bridge (MIDIberry), serial MIDI (Hairless) and MIDI port to port are all routes.

| Piece | Where | Status |
|---|---|---|
| Byte stream parser (running status, realtime, sysex) | `studio-core/midi_stream.rs`, checked against the web page's JS parser on 4000 random streams | tested |
| Route model, validation, loop detection, `Router` (one bridge per route, unchanged routes survive edits) | `studio-core/routes.rs` | tested |
| Virtual-port plan (what to do on this PC) | `studio-core/ports.rs` | tested |
| Serial source/sink with replug backoff | `studio-app/serial_midi.rs` | tested on Linux with a pseudo-terminal |
| MIDI in source (Windows) / MIDI out sink (Windows) | `midiin.rs`, `sink.rs` (midir) | written, **never compiled for Windows** |
| Bluetooth (Windows) | `ble.rs` (btleplug) | written, **never compiled for Windows** |
| Connections page | `ui/index.html` | tested in a browser against sim mode |

## Virtual ports (the platform risk)
- Windows 11: Windows MIDI Services can create loopback endpoints from an app. Needs a small C# helper (no Rust binding yet). Not built.
- Windows 10: loopMIDI cannot be bundled (its driver needs the author's written consent to redistribute). Studio uses ports the user creates in loopMIDI.
- Bome's driver SDK is redistributable but paid (about US$1,950). Not planned.
- Until a port is created natively, the Windows build opens an existing port by name (midir cannot create ports).

## Next
1. CI build on Windows, fix compile errors, user tests BLE + serial + MIDI out.
2. C# helper creating Windows MIDI Services loopback ports.
3. Editors (macro and MIDI) inside the app, then default sound-output switch.
