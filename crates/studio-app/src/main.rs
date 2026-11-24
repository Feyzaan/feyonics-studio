mod ble;
mod midiin;
mod serial;
mod serial_midi;
mod sink;

use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use studio_core::bridge::{BridgeState, MidiSink, SourceEvent};
use studio_core::config::{self, Config};
use studio_core::ports::{self, PortPlan, SystemFacts};
use studio_core::routes::{self, Issue, Route, RouteState, Router, Sink, Source};

const UI: &str = include_str!("../ui/index.html");
type Ev = (String, SourceEvent);

#[derive(Serialize, Clone)]
struct Snapshot {
    bridge: BridgeState,            // state of the route with id "ble" (what the Bluetooth page shows)
    routes: Vec<RouteState>,
    route_defs: Vec<Route>,
    issues: Vec<Issue>,
    config: Config,
    midi_port: Option<String>,
    midi_error: Option<String>,
    ports: Vec<String>,             // MIDI output ports
    midi_inputs: Vec<String>,
    serial_ports: Vec<String>,
    plan: PortPlan,
    log: Vec<String>,
    sim: bool,
}

struct Shared { snap: Snapshot, cmds: mpsc::Sender<Cmd> }
enum Cmd { Apply(Vec<Route>), TestNote(Option<String>) }

fn list_ports() -> Vec<String> {
    #[cfg(windows)] { sink::win::list_ports() }
    #[cfg(not(windows))] { vec![] }
}
fn list_inputs() -> Vec<String> {
    #[cfg(windows)] { sink::win::list_inputs() }
    #[cfg(not(windows))] { vec![] }
}
fn system_facts() -> SystemFacts {
    #[cfg(windows)] { sink::win::facts() }
    #[cfg(not(windows))] { SystemFacts::default() }
}
fn open_midi_out(port: &str, sim: bool) -> Result<sink::BoxSink, String> {
    if sim { return Ok(Box::new(sink::LogSink)); }
    #[cfg(windows)] { sink::win::open(port) }
    #[cfg(not(windows))] { let _ = port; Err("Real MIDI output is only built for Windows. Run with --sim to try the app.".into()) }
}
fn make_sink(r: &Route, sim: bool, errs: &mut Vec<String>) -> sink::BoxSink {
    let res: Result<sink::BoxSink, String> = match &r.sink {
        Sink::MidiOut { port } => open_midi_out(port, sim),
        Sink::Serial { port, baud } => serial_midi::SerialSink::open(port, *baud).map(|s| Box::new(s) as sink::BoxSink),
    };
    match res { Ok(s) => s, Err(e) => { errs.push(format!("{}: {}", r.name, e)); Box::new(serial_midi::FailSink(e)) } }
}

fn token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(std::process::id() as u64);
    format!("{:016x}{:016x}", h.finish(), std::collections::hash_map::RandomState::new().build_hasher().finish())
}

/// Brings the saved Bluetooth settings (older config fields) into the route list.
fn sync_legacy(cfg: &mut Config) {
    for r in cfg.routes.iter_mut().filter(|r| r.id == "ble") {
        if let Sink::MidiOut { port } = &mut r.sink { if !cfg.midi_port_name.is_empty() { *port = cfg.midi_port_name.clone(); } }
        if let Source::Ble { name_filter } = &mut r.source { *name_filter = cfg.device_name_filter.clone(); }
    }
}

fn main() {
    let sim = std::env::args().any(|a| a == "--sim") || cfg!(not(windows));
    let cfg_path = config::default_path();
    let mut cfg = Config::load(&cfg_path);
    if cfg.routes.is_empty() { cfg.routes = routes::default_routes(); }
    sync_legacy(&mut cfg);

    let (cmd_tx, cmd_rx) = mpsc::channel::<Cmd>();
    let (ev_tx, ev_rx) = mpsc::channel::<Ev>();
    let shared = Arc::new(Mutex::new(Shared {
        snap: Snapshot {
            bridge: studio_core::bridge::Bridge::new(sink::LogSink).state().clone(), routes: vec![], route_defs: cfg.routes.clone(), issues: routes::validate(&cfg.routes),
            config: cfg.clone(), midi_port: None, midi_error: None, ports: list_ports(), midi_inputs: list_inputs(), serial_ports: serial_midi::list_ports(),
            plan: ports::plan(&system_facts()), log: vec![], sim,
        },
        cmds: cmd_tx,
    }));
    { let s = shared.clone(); let routes = cfg.routes.clone(); std::thread::spawn(move || engine(s, ev_tx, ev_rx, cmd_rx, routes, cfg.device_name_filter, sim)); }

    let server = tiny_http::Server::http("127.0.0.1:0").expect("bind");
    let port = server.server_addr().to_ip().map(|a| a.port()).unwrap_or(0);
    let tok = token();
    let url = format!("http://127.0.0.1:{}/?t={}", port, tok);
    println!("Feyonics Studio is running at {}", url);
    #[cfg(windows)] { let _ = std::process::Command::new("cmd").args(["/C", "start", "", &url]).spawn(); }
    for req in server.incoming_requests() { handle(req, &shared, &tok, port, &cfg_path); }
}

struct Sources { stops: HashMap<String, Arc<AtomicBool>>, ble_started: HashSet<String>, tx: Sender<Ev>, sim: bool, ble_filter: String }
impl Sources {
    fn stop_all_streams(&mut self) { for (_, s) in self.stops.drain() { s.store(true, Ordering::Relaxed); } }
    fn start(&mut self, routes: &[Route]) {
        self.stop_all_streams();
        for r in routes.iter().filter(|r| r.enabled) {
            let stop = Arc::new(AtomicBool::new(false));
            match &r.source {
                Source::Serial { port, baud } => { serial_midi::spawn_source(r.id.clone(), port.clone(), *baud, self.tx.clone(), stop.clone()); self.stops.insert(r.id.clone(), stop); }
                Source::MidiIn { port } => { midiin::spawn_source(r.id.clone(), port.clone(), self.tx.clone(), stop.clone()); self.stops.insert(r.id.clone(), stop); }
                Source::Ble { name_filter } => {
                    if self.ble_started.insert(r.id.clone()) {          // the Bluetooth worker lives for the whole run
                        let (btx, brx) = mpsc::channel();
                        if self.sim { ble::spawn_sim(btx); } else { #[cfg(windows)] ble::win::spawn(btx, if name_filter.is_empty() { self.ble_filter.clone() } else { name_filter.clone() }); #[cfg(not(windows))] { let _ = (&btx, name_filter); } }
                        let (tx, id) = (self.tx.clone(), r.id.clone());
                        std::thread::spawn(move || { for e in brx { if tx.send((id.clone(), e)).is_err() { break; } } });
                    }
                }
            }
        }
    }
}

fn engine(shared: Arc<Mutex<Shared>>, ev_tx: Sender<Ev>, ev: Receiver<Ev>, cmds: Receiver<Cmd>, routes0: Vec<Route>, ble_filter: String, sim: bool) {
    let mut log: VecDeque<String> = VecDeque::new();
    let push = |l: &mut VecDeque<String>, s: String| { l.push_back(s); while l.len() > 300 { l.pop_front(); } };
    let build = |rs: &[Route], log: &mut VecDeque<String>| -> Router<sink::BoxSink> {
        let mut errs = vec![];
        let r = Router::new(rs, |route| make_sink(route, sim, &mut errs));
        for e in errs { log.push_back(format!("output: {}", e)); }
        r
    };
    let mut routes = routes0;
    let mut router = build(&routes, &mut log);
    let mut sources = Sources { stops: HashMap::new(), ble_started: HashSet::new(), tx: ev_tx, sim, ble_filter };
    sources.start(&routes);
    loop {
        let mut changed = true;
        while let Ok(c) = cmds.try_recv() {
            match c {
                Cmd::Apply(new) => {
                    routes = new; let mut fresh = build(&routes, &mut log); fresh.adopt_unchanged(&mut router);
                    router.release_all(); router = fresh; sources.start(&routes);
                    push(&mut log, format!("routes applied ({} active)", routes.iter().filter(|r| r.enabled).count()));
                }
                Cmd::TestNote(only) => {
                    let ids: Vec<String> = routes.iter().filter(|r| r.enabled && matches!(r.sink, Sink::MidiOut { .. }) && only.as_ref().map(|o| *o == r.id).unwrap_or(true)).map(|r| r.id.clone()).collect();
                    if ids.is_empty() { push(&mut log, "test note: no route with a MIDI output".into()); }
                    for id in ids {
                        if let Some(b) = router.bridge_mut(&id) { let _ = b.sink_mut().send(&[0x90, 72, 100]); std::thread::sleep(Duration::from_millis(250)); let _ = b.sink_mut().send(&[0x80, 72, 0]); push(&mut log, format!("test note sent on route {}", id)); }
                    }
                }
            }
        }
        match ev.recv_timeout(Duration::from_millis(100)) {
            Ok((id, e)) => {
                let one = |id: &str, e: SourceEvent, router: &mut Router<sink::BoxSink>, log: &mut VecDeque<String>| {
                    match &e {
                        SourceEvent::Connected { name } => push(log, format!("[{}] connected to {}", id, name)),
                        SourceEvent::Disconnected { reason } => push(log, format!("[{}] disconnected: {}", id, reason)),
                        SourceEvent::Error(m) => push(log, format!("[{}] error: {}", id, m)),
                        SourceEvent::Packet(p) => push(log, format!("[{}] ble in: {:02x?}", id, p)),
                        SourceEvent::Message(m) => push(log, format!("[{}] midi in: {:02x?}", id, m)),
                        _ => {}
                    }
                    router.handle(id, e);
                };
                one(&id, e, &mut router, &mut log);
                while let Ok((id, e)) = ev.try_recv() { one(&id, e, &mut router, &mut log); }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => { changed = false; }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if changed {
            let states = router.states();
            let mut g = shared.lock().unwrap();
            if let Some(b) = states.iter().find(|s| s.id == "ble") { g.snap.bridge = b.state.clone(); }
            let first_out = routes.iter().find_map(|r| if let Sink::MidiOut { port } = &r.sink { Some(port.clone()) } else { None });
            g.snap.midi_port = first_out.map(|p| if sim { "(simulated, printed to console)".to_string() } else { p });
            g.snap.midi_error = None;
            g.snap.routes = states; g.snap.route_defs = routes.clone(); g.snap.issues = routes::validate(&routes);
            g.snap.log = log.iter().cloned().collect();
        }
    }
}

fn handle(mut req: tiny_http::Request, shared: &Arc<Mutex<Shared>>, tok: &str, port: u16, cfg_path: &std::path::Path) {
    use tiny_http::{Header, Method, Response};
    let json = |s: String| Response::from_string(s).with_header(Header::from_bytes("Content-Type", "application/json").unwrap());
    // DNS-rebinding and cross-site protection: only our own host and origin, and a per-run token.
    let host_ok = req.headers().iter().find(|h| h.field.equiv("Host")).map(|h| { let v = h.value.as_str(); v == format!("127.0.0.1:{}", port) || v == format!("localhost:{}", port) }).unwrap_or(false);
    let origin_ok = req.headers().iter().find(|h| h.field.equiv("Origin")).map(|h| h.value.as_str().starts_with(&format!("http://127.0.0.1:{}", port))).unwrap_or(true);
    let url = req.url().to_string();
    let (path, query) = url.split_once('?').unwrap_or((&url, ""));
    let tok_ok = query.split('&').any(|kv| kv == format!("t={}", tok))
        || req.headers().iter().any(|h| h.field.equiv("X-Token") && h.value.as_str() == tok);
    if !host_ok || !origin_ok || !tok_ok {
        let _ = req.respond(Response::from_string("forbidden").with_status_code(403)); return;
    }
    let method = req.method().clone();
    let mut body = String::new();
    if method == Method::Post { let _ = req.as_reader().read_to_string(&mut body); }
    let resp = match (method, path) {
        (Method::Get, "/") => Response::from_string(UI.replace("__TOKEN__", tok)).with_header(Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap()).boxed(),
        (Method::Get, "/api/state") => {
            let mut g = shared.lock().unwrap();
            g.snap.ports = list_ports(); g.snap.midi_inputs = list_inputs(); g.snap.serial_ports = serial_midi::list_ports();
            json(serde_json::to_string(&g.snap).unwrap()).boxed()
        }
        (Method::Post, "/api/test-note") => {
            let id = serde_json::from_str::<serde_json::Value>(&body).ok().and_then(|v| v.get("id").and_then(|x| x.as_str().map(String::from)));
            let _ = shared.lock().unwrap().cmds.send(Cmd::TestNote(id)); json("{}".into()).boxed()
        }
        (Method::Post, "/api/routes") => match serde_json::from_str::<Vec<Route>>(&body) {
            Ok(rs) => {
                let issues: Vec<Issue> = routes::validate(&rs).into_iter().filter(|i| i.error).collect();
                if !issues.is_empty() { json(serde_json::to_string(&issues).unwrap()).with_status_code(400).boxed() }
                else {
                    let mut g = shared.lock().unwrap();
                    g.snap.config.routes = rs.clone(); g.snap.route_defs = rs.clone(); let _ = g.snap.config.save(cfg_path);
                    let _ = g.cmds.send(Cmd::Apply(rs)); json("{}".into()).boxed()
                }
            }
            Err(e) => Response::from_string(e.to_string()).with_status_code(400).boxed(),
        },
        (Method::Post, "/api/config") => match serde_json::from_str::<Config>(&body) {
            Ok(mut c) => {
                if c.routes.is_empty() { c.routes = shared.lock().unwrap().snap.config.routes.clone(); }
                sync_legacy(&mut c);
                let mut g = shared.lock().unwrap(); g.snap.config = c.clone(); g.snap.route_defs = c.routes.clone(); let _ = c.save(cfg_path); let _ = g.cmds.send(Cmd::Apply(c.routes.clone())); json("{}".into()).boxed()
            }
            Err(e) => Response::from_string(e.to_string()).with_status_code(400).boxed(),
        },
        (Method::Post, "/api/detect") => json(serde_json::to_string(&serial::probe_all()).unwrap()).boxed(),
        _ => Response::from_string("not found").with_status_code(404).boxed(),
    };
    let _ = req.respond(resp);
}
