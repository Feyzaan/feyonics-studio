//! Driver for the macro-pad firmware (Oled.ino 1.24.x). Pure encode/decode of the `KM ...` lines taken from
//! `handleKeymapCommand` / `handleSettingsCommand`; no I/O. Limits are the firmware's own.
use crate::hex::{text_from_hex, to_hex};
use serde::{Deserialize, Serialize};

pub const N_LAYERS: usize = 12;
pub const N_ROWS: usize = 4;
pub const N_COLS: usize = 4;
pub const N_KEYS: usize = N_ROWS * N_COLS;
pub const N_ENC: usize = 5; // left cw, left ccw, left press, display cw, display ccw
pub const LABEL_MAX: usize = 13;
pub const TEXT_MAX: usize = 51;
pub const NAME_MAX: usize = 15;
pub const LAUNCH_NAME_MAX: usize = 23;
pub const OWNER_MAX: usize = 19;
pub const DEVICE_MAX: usize = 11;
pub const BT_NAME_MAX: usize = 14;

/// Firmware `ActType`.
pub mod act {
    pub const NONE: u8 = 0;
    pub const TRANS: u8 = 1; // "same as layer 1" (keys only)
    pub const KEY: u8 = 2;
    pub const COMBO: u8 = 3;
    pub const TEXT: u8 = 4;
    pub const MEDIA: u8 = 5;
    pub const WHEEL: u8 = 6;
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Action {
    pub t: u8,
    pub code: u8,
    pub m1: u8,
    pub m2: u8,
    pub label: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroKeymap {
    pub names: Vec<String>,           // N_LAYERS
    pub keys: Vec<Vec<Action>>,       // [layer][row*4+col]
    pub enc: Vec<Vec<Action>>,        // [layer][slot]
}

impl Default for MacroKeymap {
    fn default() -> Self {
        MacroKeymap {
            names: vec![String::new(); N_LAYERS],
            keys: vec![vec![Action::default(); N_KEYS]; N_LAYERS],
            enc: vec![vec![Action::default(); N_ENC]; N_LAYERS],
        }
    }
}

fn clean(s: &str, max: usize) -> String {
    s.chars().filter(|c| (' '..='~').contains(c)).take(max).collect()
}
// The firmware's sscanf("%s") stops at whitespace, and the hex of any printable string has none, so any text is safe.

pub fn cmd_hello() -> String { "KM HELLO".into() }
pub fn cmd_get() -> String { "KM GET".into() }
pub fn cmd_save() -> String { "KM SAVE".into() }
pub fn cmd_defaults() -> String { "KM DEFAULTS".into() }
pub fn cmd_factory_reset() -> String { "KM FACTORYRESET".into() }
pub fn cmd_settings() -> String { "KM SETTINGS".into() }
pub fn cmd_enable_get() -> String { "KM ENABLEGET".into() }
pub fn cmd_launch_get() -> String { "KM LAUNCHGET".into() }
pub fn cmd_padname_get() -> String { "KM PADNAMEGET".into() }
pub fn cmd_bt_status() -> String { "KM BTSTATUS".into() }
pub fn cmd_update() -> String { "KM UPDATE".into() }

pub fn cmd_set_key(l: usize, r: usize, c: usize, a: &Action) -> Result<String, String> {
    if l >= N_LAYERS || r >= N_ROWS || c >= N_COLS || a.t > act::WHEEL { return Err("key out of range".into()); }
    let text = if a.t == act::TEXT { clean(&a.text, TEXT_MAX) } else { String::new() };
    Ok(format!("KM SETKEY {} {} {} {} {} {} {} {} {}", l, r, c, a.t, a.code, a.m1, a.m2, to_hex(&clean(&a.label, LABEL_MAX)), to_hex(&text)))
}
pub fn cmd_set_enc(l: usize, slot: usize, a: &Action) -> Result<String, String> {
    if l >= N_LAYERS || slot >= N_ENC || a.t > act::WHEEL || a.t == act::TRANS { return Err("knob action out of range".into()); }
    let text = if a.t == act::TEXT { clean(&a.text, TEXT_MAX) } else { String::new() };
    Ok(format!("KM SETENC {} {} {} {} {} {} {} {}", l, slot, a.t, a.code, a.m1, a.m2, to_hex(&clean(&a.label, LABEL_MAX)), to_hex(&text)))
}
pub fn cmd_set_name(l: usize, name: &str) -> Result<String, String> {
    if l >= N_LAYERS { return Err("layer out of range".into()); }
    Ok(format!("KM SETNAME {} {}", l, to_hex(&clean(name, NAME_MAX))))
}
pub fn cmd_enable_set(l: usize, on: bool) -> Result<String, String> {
    if l >= N_LAYERS { return Err("layer out of range".into()); }
    if l == 0 && !on { return Err("My macros can't be switched off".into()); }
    Ok(format!("KM ENABLESET {} {}", l, on as u8))
}
pub fn cmd_padname_set(owner: &str, device: &str) -> Result<String, String> {
    let (o, d) = (clean(owner, OWNER_MAX), clean(device, DEVICE_MAX));
    if o.is_empty() && d.is_empty() { return Err("give it at least one line".into()); }
    Ok(format!("KM PADNAMESET {} {}", to_hex(&o), to_hex(&d)))
}
/// type: 0 nothing, 1 Windows Start search, 2 shortcut
pub fn cmd_launch_set(l: usize, t: u8, m1: u8, m2: u8, code: u8, name: &str) -> Result<String, String> {
    if l >= N_LAYERS || t > 2 { return Err("launch out of range".into()); }
    Ok(format!("KM LAUNCHSET {} {} {} {} {} {}", l, t, m1, m2, code, to_hex(&clean(name, LAUNCH_NAME_MAX))))
}
pub fn cmd_layer(l: i32, open: bool) -> String { format!("KM LAYER {}{}", l, if open { " 1" } else { "" }) }
pub fn cmd_time(y: u32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> String { format!("KM TIME {} {} {} {} {} {}", y, mo, d, h, mi, s) }
pub fn cmd_theme(i: u32) -> String { format!("KM THEME {}", i) }
pub fn cmd_rotate(i: u32) -> String { format!("KM ROTATE {}", i) }
pub fn cmd_bright(p: u32) -> String { format!("KM BRIGHT {}", p) }
pub fn cmd_bt_mode(m: u32) -> String { format!("KM BTMODE {}", m) }
pub fn cmd_bt_pair(on: bool) -> String { format!("KM BTPAIR {}", on as u8) }
pub fn cmd_bt_forget() -> String { "KM BTFORGET".into() }
pub fn cmd_bt_name(n: &str) -> Result<String, String> {
    let c = clean(n, BT_NAME_MAX);
    if c.is_empty() { Err("name is empty".into()) } else { Ok(format!("KM BTNAME {}", to_hex(&c))) }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    Name { l: usize, name: String },
    Key { l: usize, r: usize, c: usize, a: Action },
    Enc { l: usize, slot: usize, a: Action },
    End,
    Ok,
    Saved,
    Err(String),
    Settings(Vec<i32>),
    Enable { l: usize, on: bool },
    PadName { owner: String, device: String },
    Launch { l: usize, t: u8, m1: u8, m2: u8, code: u8, name: String },
    BtStatus { mode: u8, link: bool, state: u8, bonds: u8, pair_left: u8, module_fw: String, name: String },
    Img { l: usize, w: u32, h: u32, frames: u32, bytes: u32 },
    Fs { ok: bool, used: u64, total: u64 },
    Other(String),
}

fn parse_action(p: &[&str]) -> Option<Action> {
    // type code m1 m2 labelhex texthex
    if p.len() < 6 { return None; }
    Some(Action {
        t: p[0].parse().ok()?, code: p[1].parse().ok()?, m1: p[2].parse().ok()?, m2: p[3].parse().ok()?,
        label: text_from_hex(p[4]), text: text_from_hex(p[5]),
    })
}

pub fn parse_reply(line: &str) -> Option<Reply> {
    let line = line.trim_end();
    let rest = line.strip_prefix("KM ")?;
    let p: Vec<&str> = rest.split_whitespace().collect();
    let u = |i: usize| p.get(i).and_then(|s| s.parse::<usize>().ok());
    Some(match p.first().copied()? {
        "OK" => Reply::Ok,
        "SAVED" => Reply::Saved,
        "END" => Reply::End,
        "ERR" => Reply::Err(rest.strip_prefix("ERR").unwrap_or("").trim().to_string()),
        "NAME" => Reply::Name { l: u(1)?, name: text_from_hex(p.get(2)?) },
        "KEY" => Reply::Key { l: u(1)?, r: u(2)?, c: u(3)?, a: parse_action(&p[4..])? },
        "ENC" => Reply::Enc { l: u(1)?, slot: u(2)?, a: parse_action(&p[3..])? },
        "SETTINGS" => Reply::Settings(p[1..].iter().filter_map(|s| s.parse().ok()).collect()),
        "ENABLE" => Reply::Enable { l: u(1)?, on: u(2)? != 0 },
        "PADNAME" => Reply::PadName { owner: text_from_hex(p.get(1)?), device: text_from_hex(p.get(2)?) },
        "LAUNCH" => Reply::Launch { l: u(1)?, t: u(2)? as u8, m1: u(3)? as u8, m2: u(4)? as u8, code: u(5)? as u8, name: text_from_hex(p.get(6).copied().unwrap_or("-")) },
        "BTSTATUS" => Reply::BtStatus { mode: u(1)? as u8, link: u(2)? != 0, state: u(3)? as u8, bonds: u(4)? as u8, pair_left: u(5)? as u8, module_fw: p.get(6)?.to_string(), name: text_from_hex(p.get(7).copied().unwrap_or("-")) },
        "IMG" => Reply::Img { l: u(1)?, w: u(2)? as u32, h: u(3)? as u32, frames: u(4)? as u32, bytes: u(5)? as u32 },
        "FS" => Reply::Fs { ok: u(1)? != 0, used: p.get(2)?.parse().ok()?, total: p.get(3)?.parse().ok()? },
        _ => Reply::Other(rest.to_string()),
    })
}

/// Folds the reply lines of `KM GET` into a keymap. Unknown or out-of-range lines are ignored, like the old editor.
pub fn keymap_from_get(lines: &[String]) -> MacroKeymap {
    let mut km = MacroKeymap::default();
    for line in lines {
        match parse_reply(line) {
            Some(Reply::Name { l, name }) if l < N_LAYERS => km.names[l] = clean(&name, NAME_MAX),
            Some(Reply::Key { l, r, c, a }) if l < N_LAYERS && r < N_ROWS && c < N_COLS => km.keys[l][r * N_COLS + c] = a,
            Some(Reply::Enc { l, slot, a }) if l < N_LAYERS && slot < N_ENC => km.enc[l][slot] = a,
            _ => {}
        }
    }
    km
}

/// Commands that make the pad match `want` given what it currently has (`have`); only differences are sent.
pub fn diff_commands(have: &MacroKeymap, want: &MacroKeymap) -> Vec<String> {
    let mut out = Vec::new();
    for l in 0..N_LAYERS {
        if have.names[l] != want.names[l] { if let Ok(c) = cmd_set_name(l, &want.names[l]) { out.push(c); } }
        for k in 0..N_KEYS {
            if have.keys[l][k] != want.keys[l][k] { if let Ok(c) = cmd_set_key(l, k / N_COLS, k % N_COLS, &want.keys[l][k]) { out.push(c); } }
        }
        for e in 0..N_ENC {
            if have.enc[l][e] != want.enc[l][e] { if let Ok(c) = cmd_set_enc(l, e, &want.enc[l][e]) { out.push(c); } }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(t: u8, code: u8, m1: u8, label: &str, text: &str) -> Action { Action { t, code, m1, m2: 0, label: label.into(), text: text.into() } }

    #[test]
    fn set_key_format_matches_firmware_sscanf() {
        let a = key(act::COMBO, b'c', 0x80, "Copy", "");
        assert_eq!(cmd_set_key(0, 0, 1, &a).unwrap(), "KM SETKEY 0 0 1 3 99 128 0 436F7079 -");
    }
    #[test]
    fn text_only_sent_for_text_actions() {
        let a = key(act::KEY, b'a', 0, "A", "should not be sent");
        assert!(cmd_set_key(0, 0, 1, &a).unwrap().ends_with(" -"));
        let t = key(act::TEXT, 0, 0, "Hi", "Thank you!");
        assert!(cmd_set_key(0, 0, 1, &t).unwrap().ends_with("5468616E6B20796F7521"));
    }
    #[test]
    fn limits_enforced_like_firmware() {
        let long = key(act::TEXT, 0, 0, &"x".repeat(40), &"y".repeat(80));
        let line = cmd_set_key(1, 1, 1, &long).unwrap();
        let hexes: Vec<&str> = line.split(' ').rev().take(2).collect();
        assert_eq!(hexes[0].len(), TEXT_MAX * 2);
        assert_eq!(hexes[1].len(), LABEL_MAX * 2);
        // firmware reads label into %31s and text into %127s
        assert!(hexes[1].len() <= 31 && hexes[0].len() <= 127);
    }
    #[test]
    fn rejects_bad_input() {
        assert!(cmd_set_key(12, 0, 0, &Action::default()).is_err());
        assert!(cmd_set_enc(0, 0, &Action { t: act::TRANS, ..Default::default() }).is_err());
        assert!(cmd_enable_set(0, false).is_err());
        assert!(cmd_padname_set("", "").is_err());
    }
    #[test]
    fn parse_get_lines() {
        let lines: Vec<String> = [
            "KM NAME 0 4D79206D6163726F73",
            "KM KEY 0 0 1 3 99 128 0 436F7079 -",
            "KM KEY 1 3 3 4 0 0 0 5468616E6B2079 5468616E6B20796F7521",
            "KM ENC 0 2 5 226 0 0 4D757465 -",
            "KM END",
        ].iter().map(|s| s.to_string()).collect();
        let km = keymap_from_get(&lines);
        assert_eq!(km.names[0], "My macros");
        assert_eq!(km.keys[0][1], key(act::COMBO, 99, 128, "Copy", ""));
        assert_eq!(km.keys[1][15].text, "Thank you!");
        assert_eq!(km.enc[0][2].t, act::MEDIA);
        assert_eq!(km.enc[0][2].label, "Mute");
    }
    #[test]
    fn round_trip_through_wire_format() {
        let mut want = MacroKeymap::default();
        want.names[2] = "Claude Code".into();
        want.keys[2][5] = key(act::TEXT, 0, 0, "/clear", "/clear");
        want.keys[3][9] = key(act::COMBO, b'z', 0x80, "Undo", "");
        want.enc[3][0] = Action { t: act::WHEEL, code: 1, m1: 0x82, m2: 0, label: "Alt+wheel up".into(), text: String::new() };
        let cmds = diff_commands(&MacroKeymap::default(), &want);
        assert_eq!(cmds.len(), 4);
        // emulate the pad: parse our own commands back into the "firmware" state and echo them as GET lines
        let mut echo = Vec::new();
        for c in &cmds {
            let p: Vec<&str> = c.split(' ').collect();
            match p[1] {
                "SETNAME" => echo.push(format!("KM NAME {} {}", p[2], p[3])),
                "SETKEY" => echo.push(format!("KM KEY {} {} {} {} {} {} {} {} {}", p[2], p[3], p[4], p[5], p[6], p[7], p[8], p[9], p[10])),
                "SETENC" => echo.push(format!("KM ENC {} {} {} {} {} {} {} {}", p[2], p[3], p[4], p[5], p[6], p[7], p[8], p[9])),
                _ => unreachable!(),
            }
        }
        assert_eq!(keymap_from_get(&echo), want);
        assert!(diff_commands(&want, &want).is_empty());
    }
    #[test]
    fn replies() {
        assert_eq!(parse_reply("KM SAVED"), Some(Reply::Saved));
        assert_eq!(parse_reply("KM ERR keymap too big to save (5000 of 4000 bytes)"), Some(Reply::Err("keymap too big to save (5000 of 4000 bytes)".into())));
        assert_eq!(parse_reply("KM ENABLE 3 1"), Some(Reply::Enable { l: 3, on: true }));
        assert_eq!(parse_reply("KM PADNAME 46657A 5A657573"), Some(Reply::PadName { owner: "Fez".into(), device: "Zeus".into() }));
        assert!(matches!(parse_reply("KM SETTINGS 0 0 100 1 -1 -1 20 20 0 0 0 0 0"), Some(Reply::Settings(v)) if v.len() == 13));
        assert!(parse_reply("hello").is_none());
    }
}
