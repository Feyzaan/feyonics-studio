# Feyonics Studio (Windows prototype)

Bluetooth (BLE MIDI) pad -> virtual MIDI port, plus pad detection over USB (MIDI pad vs macro pad).

## Build the .exe
Push this folder to a GitHub repo. The `windows-build` workflow builds `feyonics-studio.exe` and attaches it as an artifact.
(Or on your PC: install Rust, then `cargo build --release -p feyonics-studio`.)

## Try it without Bluetooth
`feyonics-studio.exe --sim` plays a simulated pad that connects, plays notes and drops the link.

## Real test on your PC
1. Create a MIDI output for Studio to write to.
   - Windows 11 (with Windows MIDI Services): create a loopback endpoint named "Feyonics Zeus".
   - Windows 10: install loopMIDI and add a port named "Feyonics Zeus" (needed until Studio creates ports itself).
2. Pair the pad once in Windows Settings > Bluetooth (the pad's 120 s pairing window must be open).
3. Run `feyonics-studio.exe`. A browser tab opens. Set the MIDI port name in Settings if yours differs.
4. Press "Send test note" and watch your DAW's MIDI input. Then play the pad.
5. Plug the pad in by USB, open "My pads", press "Look for pads".

## Layout
- `crates/studio-core`: protocol logic, no OS code, fully unit-tested (BLE-MIDI parser checked against the Studio JS, firmware detection, macro-pad codec, bridge, config).
- `crates/studio-app`: UI server, USB probe, simulated pad; Windows-only BLE (btleplug) and MIDI (midir) backends.
