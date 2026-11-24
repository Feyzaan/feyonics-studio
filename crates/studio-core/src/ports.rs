//! How Studio gets a virtual MIDI port on this PC. Pure decision logic (testable anywhere); the OS probing lives in the app.
//!
//! Findings behind the rules (checked Nov 2026):
//! * Windows MIDI Services (Microsoft, Windows 11) has built-in loopback endpoints that an app may create through its SDK.
//! * loopMIDI's driver is proprietary; its SDK needs loopMIDI installed, and its software may not be redistributed without
//!   the author's written consent. So Studio never bundles it: on Windows 10 it only *uses* ports the user made in loopMIDI.
//! * Bome's BMIDI SDK is a paid redistributable driver (about US$1,950 for a perpetual licence) and is not planned.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SystemFacts {
    pub windows_build: Option<u32>,   // 22000+ is Windows 11
    pub midi_services_installed: bool,
    pub loopmidi_installed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "plan", rename_all = "snake_case")]
pub enum PortPlan {
    /// Studio can create and remove its own loopback ports.
    CreateNative { backend: &'static str },
    /// Studio can only route to ports the user created in another tool.
    UseExisting { tool: &'static str, note: String },
    /// Nothing usable yet; show these steps.
    NeedsSetup { steps: Vec<String> },
}

pub fn plan(f: &SystemFacts) -> PortPlan {
    if f.midi_services_installed { return PortPlan::CreateNative { backend: "Windows MIDI Services" }; }
    let win11 = f.windows_build.map(|b| b >= 22000).unwrap_or(false);
    if win11 {
        return PortPlan::NeedsSetup { steps: vec![
            "Install Windows MIDI Services (free, from Microsoft). Studio can then create its own ports.".into(),
            "Or create a loopback pair yourself in its MIDI Loopback Setup tool and pick it in Studio.".into(),
        ] };
    }
    if f.loopmidi_installed {
        return PortPlan::UseExisting { tool: "loopMIDI", note: "Create the ports you need in loopMIDI (the + button) and pick them in Studio. Keep loopMIDI running.".into() };
    }
    PortPlan::NeedsSetup { steps: vec![
        "Install loopMIDI (free for personal use) and create a port named \"Feyonics Pad\".".into(),
        "On Windows 11 you can use Windows MIDI Services instead.".into(),
    ] }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn f(build: Option<u32>, wms: bool, lm: bool) -> SystemFacts { SystemFacts { windows_build: build, midi_services_installed: wms, loopmidi_installed: lm } }
    #[test] fn midi_services_wins() { assert!(matches!(plan(&f(Some(26100), true, true)), PortPlan::CreateNative { .. })); }
    #[test] fn win11_without_services_asks_for_it() { assert!(matches!(plan(&f(Some(26100), false, true)), PortPlan::NeedsSetup { .. })); }
    #[test] fn win10_with_loopmidi_uses_it() { assert!(matches!(plan(&f(Some(19045), false, true)), PortPlan::UseExisting { tool: "loopMIDI", .. })); }
    #[test] fn win10_nothing_installed() { assert!(matches!(plan(&f(Some(19045), false, false)), PortPlan::NeedsSetup { .. })); }
    #[test] fn unknown_build_is_treated_like_win10() { assert!(matches!(plan(&f(None, false, true)), PortPlan::UseExisting { .. })); }
}
