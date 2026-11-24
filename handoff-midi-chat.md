# Feyonics Studio web shell: handoff for the MIDI-editor chat

Date: 2026-11-09. Status: web shell built and tested in headless Chrome with simulated devices. Not yet tried on real hardware.

## 1. What exists

`feyonics-studio-web.html` is one page that does three jobs:

1. **Bluetooth bridge.** Web Bluetooth reads BLE-MIDI from the ESP32 (service `03b80e5a-ede8-4b33-a751-6ce34ec4c700`, characteristic `7772e5db-3868-4112-a1a9-f2669d106bf3`), parses it with the same `bleParse` the MIDI editor uses, and writes the messages to a Web MIDI output (loopMIDI or a Windows 11 loopback port). It releases held notes and sustain when the link drops and reconnects with a 1,2,3,4,5,5 s back-off.
2. **Detection.** Web Serial opens the pad at 115200, sends `KM HELLO`, and picks the editor from the answer.
3. **Editors.** Macro pad: a native editor inside the shell. MIDI pad: your existing `feyonics-studio.html`, hosted in an iframe.

The ESP32 sketch is unchanged. The working name "Zeus" does not appear in any visible text.

## 2. Detection rules (the contract on the pad side)

| Firmware | Answer to `KM HELLO` | Rule |
|---|---|---|
| MIDI 2.x | `KM HELLO 2 4 4 4 <fw> 596` | protocol token >= 2 and a numeric 8th token (setup size) |
| Macro 1.x | `KM HELLO 1 12 4 4 <fw>` | protocol token == 1 |

The firmware version is token 6 in both. **Please keep the shape of the MIDI HELLO line stable.** If you add tokens, add them after the size. If you ever change the protocol number, tell the shell chat first.

## 3. How the shell hosts the MIDI editor (what you must keep working)

Only one app can hold the USB port, so the shell detects the pad, then **closes** the port and hands the same `SerialPort` object to the editor:

1. The shell fetches `feyonics-studio.html` from its own folder. If that fails (file opened from disk) it asks for the file once and keeps a copy in IndexedDB.
2. It injects a small script right after `<head>` and loads the result as `iframe.srcdoc` with `allow="serial; bluetooth; midi; autoplay"`. The script does this and nothing else:
   ```js
   navigator.serial.requestPort = () => Promise.resolve(parent.__feyonicsPort);
   navigator.serial.getPorts    = () => Promise.resolve([parent.__feyonicsPort]);
   // if navigator.serial is missing in the frame, a stub with add/removeEventListener is defined first
   ```
3. After the frame loads, the shell calls `contentWindow.connectPad()` so the user does not have to press Connect again.
4. On leaving, the shell calls `contentWindow.disconnectPad(true)` so the editor releases its reader/writer locks and closes the port, then removes the iframe.

The editor therefore has to keep these properties (they are true today, so this is a "don't break it" list):

- `connectPad()` and `disconnectPad(quiet)` stay **global functions** that work with no arguments. `connectPad()` toggles, so the shell calls it only when disconnected.
- The port comes **only** from `navigator.serial.requestPort()` (no filters needed). The editor opens it itself with `port.open({ baudRate: 115200 })`.
- `disconnectPad` cancels the reader, releases both locks, then calls `port.close()`.
- A top-level `navigator.serial.addEventListener('disconnect', ...)` is fine.
- Page storage keys stay under `feyonics-studio-2` (the shell uses `feyonics-web-*`, no clash).

Tested with a stub that mirrors your `connectPad` path (requestPort, open, `KM HELLO`, `KM ZGET`, disconnect, reopen). **Not tested with the real file**, because it has to be run on the user's PC.

## 4. Things I'd like the MIDI chat to decide or add (small)

1. **Embedded mode.** When `window.parent !== window`, hide the editor's own header "Connect pad" button (the shell already connected it), or leave it; both work. Hiding it avoids a double toggle.
2. **Bluetooth in the editor.** The editor has its own Bluetooth button (BLE-MIDI to drive its on-screen sound). With the shell bridge running there would be two GATT connections to the same pad in one tab. Proposal: when embedded, the editor takes MIDI **from the loopback port the bridge writes to** (its existing "MIDI in" select) and its own Bluetooth button is hidden. Does that lose anything for you?
3. **Is a firmware major version check still right?** `fwOk` = major >= 2. The shell does not check; it only forwards.
4. If the editor wants a "return to Studio" affordance, there is none yet; the shell has a Disconnect button above the iframe.

## 5. What the macro side looks like (for reference; you don't need to implement it)

Separate command set on the same serial port, replies prefixed `KM `: `GET` (12 layers: `NAME`, 16 `KEY`, 5 `ENC`, `END`), `SETKEY`, `SETENC`, `SETNAME`, `SAVE`, `DEFAULTS`, `FACTORYRESET`, `SETTINGS`, `THEME`, `ROTATE`, `BRIGHT`, `AODPCT`, `AODSEC`, `USAGESTYLE`, `USAGE`, `FONT`, `HIDENAME`, `ENABLEGET/SET`, `LAUNCHGET/SET`, `PADNAMEGET/SET`, `BTMODE`, `BTPAIR`, `BTFORGET`, `BTNAME`, `BTSTATUS`, `TIME`, `LAYER`, `KEYTEST`, `SCREENTEST`, `HOME`, `UPDATE`. The shell never sends these to a MIDI pad, and never sends `ZGET/ZPUT/ZAPPLY` to a macro pad.

## 6. How to run it

1. Put `feyonics-studio-web.html` and `feyonics-studio.html` in one folder.
2. Open the shell in Chrome or Edge, either by double-clicking it or serving the folder (`python -m http.server 8765`, then `http://localhost:8765/feyonics-studio-web.html`). With a plain file open, the MIDI editor asks for `feyonics-studio.html` once.
3. Bridge: create a MIDI port (Windows 11 loopback, or loopMIDI named "Feyonics Pad"), pick it in the shell, pair the pad in Windows Bluetooth settings (120 s window), press Connect pad, play. Choose the same port as input in a music app.
4. MIDI pad: plug in over USB, My pads, Connect pad over USB, pick the port. The editor should open already connected and show the pad's setup.
5. "Try a demo MIDI pad" runs the same flow against a simulated pad (answers `KM HELLO`, `ZGET`, `ZPUT`, `ZAPPLY`, `STATUS`; the setup is a blank 596-byte blob), so you can see the hosting work without hardware.

## 7. Limits and unverified points

- Web version: Chrome/Edge only, the tab must stay open, no auto-start, no virtual port creation, one app per serial port.
- Verified in headless Chrome: BLE-MIDI parse and forward, stuck-note release on drop, reconnect, detection, macro read/edit/save with minimal diffs, error display, MIDI editor hand-over and re-open.
- Not verified: real ESP32 notifications through Web Bluetooth on Windows, the real `feyonics-studio.html` inside the iframe, loopMIDI/loopback delivery.
- The macro display commands (`THEME`, `AODPCT`, ...) use argument forms taken from the firmware summary. If one answers `KM ERR`, the shell shows the message.
- Not built yet in the web shell: macro picture upload (`IMGBEGIN`...), the .exe track (virtual-port creation, auto-start).
