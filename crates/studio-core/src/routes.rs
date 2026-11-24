//! Routes: every connection Studio makes is `source -> sink`. This one model covers the Bluetooth bridge (BLE -> MIDI port),
//! Hairless-style serial MIDI (serial -> MIDI port and MIDI port -> serial) and plain MIDI port to port.
use crate::bridge::{Bridge, BridgeState, MidiSink, SourceEvent};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const BAUDS: [u32; 9] = [9600, 31250, 38400, 57600, 115200, 230400, 250000, 500000, 1000000];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    Ble { name_filter: String },
    Serial { port: String, baud: u32 },
    MidiIn { port: String },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Sink {
    MidiOut { port: String },
    Serial { port: String, baud: u32 },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Route {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub source: Source,
    pub sink: Sink,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Issue { pub route_id: String, pub error: bool, pub message: String }

pub fn default_routes() -> Vec<Route> {
    vec![Route { id: "ble".into(), name: "Bluetooth pad to MIDI".into(), enabled: true, source: Source::Ble { name_filter: String::new() }, sink: Sink::MidiOut { port: "Feyonics Pad".into() } }]
}

/// Node in the signal graph. A MIDI port is one node (what is written to it is what a reader of it gets: a loopback cable),
/// but a serial port's input and output are separate nodes (nothing comes back unless the device echoes).
fn src_node(s: &Source) -> String { match s { Source::Ble { .. } => "ble".into(), Source::Serial { port, .. } => format!("serial-in:{}", port.to_lowercase()), Source::MidiIn { port } => format!("midi:{}", port.to_lowercase()) } }
fn sink_node(s: &Sink) -> String { match s { Sink::MidiOut { port } => format!("midi:{}", port.to_lowercase()), Sink::Serial { port, .. } => format!("serial-out:{}", port.to_lowercase()) } }

pub fn validate(routes: &[Route]) -> Vec<Issue> {
    let mut out = vec![];
    let mut seen = HashSet::new();
    let bad = |r: &Route, m: String| Issue { route_id: r.id.clone(), error: true, message: m };
    for r in routes {
        if r.id.trim().is_empty() { out.push(bad(r, "A route has no id.".into())); }
        if !seen.insert(r.id.clone()) { out.push(bad(r, format!("Two routes share the id \"{}\".", r.id))); }
        let chk = |p: &str, baud: Option<u32>, what: &str, out: &mut Vec<Issue>| {
            if p.trim().is_empty() { out.push(bad(r, format!("{} has no port chosen.", what))); }
            if let Some(b) = baud { if !(300..=3_000_000).contains(&b) { out.push(bad(r, format!("{} speed {} is not usable.", what, b))); } }
        };
        match &r.source { Source::Serial { port, baud } => chk(port, Some(*baud), "The serial input", &mut out), Source::MidiIn { port } => chk(port, None, "The MIDI input", &mut out), Source::Ble { .. } => {} }
        match &r.sink { Sink::Serial { port, baud } => chk(port, Some(*baud), "The serial output", &mut out), Sink::MidiOut { port } => chk(port, None, "The MIDI output", &mut out) }
    }
    // A cycle through MIDI ports would play every note forever.
    let mut edges: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for r in routes.iter().filter(|r| r.enabled) { edges.entry(src_node(&r.source)).or_default().push((sink_node(&r.sink), r.id.clone())); }
    for r in routes.iter().filter(|r| r.enabled) {
        // does the sink of r lead back to the source of r?
        let target = src_node(&r.source);
        let mut stack = vec![sink_node(&r.sink)]; let mut vis = HashSet::new();
        while let Some(n) = stack.pop() {
            if n == target { out.push(Issue { route_id: r.id.clone(), error: true, message: "This route feeds back into itself through a MIDI port, so notes would repeat forever. Use two different ports for the two directions.".into() }); break; }
            if !vis.insert(n.clone()) { continue; }
            if let Some(next) = edges.get(&n) { for (to, _) in next { stack.push(to.clone()); } }
        }
    }
    out
}

#[derive(Debug, Clone, Serialize)]
pub struct RouteState { pub id: String, pub name: String, pub enabled: bool, pub state: BridgeState }

/// One `Bridge` per route, so each keeps its own held-note bookkeeping and releases only its own notes when it drops.
pub struct Router<S: MidiSink> { routes: Vec<(Route, Bridge<S>)> }
impl<S: MidiSink> Router<S> {
    pub fn new(routes: &[Route], mut make_sink: impl FnMut(&Route) -> S) -> Self {
        Router { routes: routes.iter().filter(|r| r.enabled).map(|r| (r.clone(), Bridge::new(make_sink(r)))).collect() }
    }
    pub fn handle(&mut self, id: &str, ev: SourceEvent) -> bool {
        match self.routes.iter_mut().find(|(r, _)| r.id == id) { Some((_, b)) => { b.handle(ev); true } None => false }
    }
    pub fn states(&self) -> Vec<RouteState> { self.routes.iter().map(|(r, b)| RouteState { id: r.id.clone(), name: r.name.clone(), enabled: r.enabled, state: b.state().clone() }).collect() }
    pub fn bridge_mut(&mut self, id: &str) -> Option<&mut Bridge<S>> { self.routes.iter_mut().find(|(r, _)| r.id == id).map(|(_, b)| b) }
    /// Carries over the bridge (state, held notes, counters) of every route that did not change, so applying edits
    /// does not disturb connections that were left alone. Routes taken over keep their old sink.
    pub fn adopt_unchanged(&mut self, old: &mut Router<S>) {
        for (r, b) in self.routes.iter_mut() {
            if let Some(i) = old.routes.iter().position(|(o, _)| o == r) { std::mem::swap(b, &mut old.routes[i].1); }
        }
    }
    pub fn release_all(&mut self) { for (_, b) in self.routes.iter_mut() { b.release_all(); } }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::{MemorySink, Phase};
    fn r(id: &str, s: Source, k: Sink) -> Route { Route { id: id.into(), name: id.into(), enabled: true, source: s, sink: k } }
    fn serial(p: &str) -> (Source, Sink) { (Source::Serial { port: p.into(), baud: 115200 }, Sink::Serial { port: p.into(), baud: 115200 }) }
    fn mi(p: &str) -> Source { Source::MidiIn { port: p.into() } }
    fn mo(p: &str) -> Sink { Sink::MidiOut { port: p.into() } }

    #[test] fn defaults_are_valid() { assert!(validate(&default_routes()).is_empty()); }
    #[test] fn hairless_style_two_way_on_two_ports_is_fine() {
        let (ss, sk) = serial("COM5");
        assert!(validate(&[r("a", ss, mo("Serial to MIDI")), r("b", mi("MIDI to Serial"), sk)]).is_empty());
    }
    #[test] fn same_port_both_ways_through_serial_is_fine() {
        // a device that does not echo: MIDI port X -> serial and serial -> MIDI port X is the usual single-cable Hairless setup
        let (ss, sk) = serial("COM5");
        assert!(validate(&[r("a", ss, mo("X")), r("b", mi("X"), sk)]).is_empty());
    }
    #[test] fn two_midi_ports_crossed_is_a_loop() {
        let v = validate(&[r("a", mi("X"), mo("Y")), r("b", mi("Y"), mo("X"))]);
        assert_eq!(v.len(), 2); assert!(v.iter().all(|i| i.error && i.message.contains("feeds back")));
    }
    #[test] fn midi_in_to_same_midi_out_is_a_loop() { assert_eq!(validate(&[r("a", mi("X"), mo("x"))]).len(), 1); }
    #[test] fn disabled_routes_do_not_count_toward_loops() {
        let mut b = r("b", mi("Y"), mo("X")); b.enabled = false;
        assert!(validate(&[r("a", mi("X"), mo("Y")), b]).is_empty());
    }
    #[test] fn bad_input_reported() {
        let v = validate(&[r("a", Source::Serial { port: "".into(), baud: 5 }, mo("")), r("a", Source::Ble { name_filter: "".into() }, mo("X"))]);
        let m: Vec<_> = v.iter().map(|i| i.message.as_str()).collect();
        assert!(m.iter().any(|x| x.contains("serial input has no port")));
        assert!(m.iter().any(|x| x.contains("speed 5")));
        assert!(m.iter().any(|x| x.contains("MIDI output has no port")));
        assert!(m.iter().any(|x| x.contains("share the id")));
    }
    #[test] fn routes_serialise_with_readable_tags() {
        let j = serde_json::to_string(&default_routes()).unwrap();
        assert!(j.contains("\"kind\":\"ble\"") && j.contains("\"kind\":\"midi_out\""));
        let back: Vec<Route> = serde_json::from_str(&j).unwrap(); assert_eq!(back, default_routes());
    }
    #[test] fn router_isolates_routes() {
        let routes = vec![r("a", Source::Serial { port: "COM5".into(), baud: 115200 }, mo("X")), r("b", Source::Serial { port: "COM6".into(), baud: 31250 }, mo("X"))];
        let mut rt = Router::new(&routes, |_| MemorySink::default());
        rt.handle("a", SourceEvent::Connected { name: "COM5".into() });
        rt.handle("a", SourceEvent::Message(vec![0x90, 60, 100]));
        rt.handle("b", SourceEvent::Message(vec![0x90, 61, 100]));
        rt.bridge_mut("a").unwrap().sink_mut().sent.clear();
        rt.handle("a", SourceEvent::Disconnected { reason: "unplugged".into() });
        let a = rt.bridge_mut("a").unwrap().sink().sent.clone();
        assert!(a.contains(&vec![0x80, 60, 0]) && !a.contains(&vec![0x80, 61, 0]));   // only a's note released
        assert!(rt.bridge_mut("b").unwrap().sink().sent.iter().all(|m| m != &vec![0x80, 61, 0]));
        let st = rt.states(); assert_eq!(st[0].state.phase, Phase::Idle); assert_eq!(st[1].state.stats.messages, 1);
        assert!(!rt.handle("nope", SourceEvent::Scanning));
    }
    #[test] fn unchanged_routes_keep_their_state() {
        let a = r("a", mi("X"), mo("Y")); let b = r("b", mi("P"), mo("Q"));
        let mut old = Router::new(&[a.clone(), b.clone()], |_| MemorySink::default());
        old.handle("a", SourceEvent::Connected { name: "X".into() }); old.handle("b", SourceEvent::Connected { name: "P".into() });
        let mut b2 = b.clone(); b2.name = "renamed".into();
        let mut new = Router::new(&[a, b2], |_| MemorySink::default());
        new.adopt_unchanged(&mut old);
        let st = new.states(); assert_eq!(st[0].state.phase, Phase::Connected); assert_eq!(st[1].state.phase, Phase::Idle);
    }
    #[test] fn disabled_route_is_not_built() { let mut x = r("a", mi("X"), mo("Y")); x.enabled = false; assert!(Router::new(&[x], |_| MemorySink::default()).states().is_empty()); }
}
