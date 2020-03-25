// PACKET PROCESSING ENGINE
//
// This module implements configuration and execution of the packet processing
// engine.
//
//   EngineStats - struct containing global engine statistics
//   stats() -> EngineStats - get engine statistics
//   EngineState - struct representing engine state
//   state() -> &'static EngineState - get engine state
//   SharedLink - type for shared links (between apps, also in EngineState)   
//   AppState - struct representing an app in the current app network
//   App, AppConfig - traits that defines an app, and its configuration
//   PULL_NPACKETS - number of packets to be inhaled in app’s pull() methods
//   configure(&mut EngineState, &config) - apply configuration to app network
//   main(&EngineState, Options) - run the engine breathe loop
//   Options - engine breathe loop options
//   now() -> Instant - return current monotonic engine time
//   timeout(Duration) -> [()->bool] - make timer returning true after duration
//   report_load() - print load report
//   report_links() - print link statistics

use super::link;
use super::config;
use super::lib;

use std::collections::HashMap;
use std::collections::HashSet;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use std::thread::sleep;
use std::cmp::min;
use once_cell::unsync::Lazy;

// Counters for global engine statistics.
pub struct EngineStats {
    pub breaths: u64,  // Total breaths taken
    pub frees: u64,    // Total packets freed
    pub freebits: u64, // Total packet bits freed (for 10GbE)
    pub freebytes: u64 // Total packet bytes freed
}
static mut STATS: EngineStats = EngineStats {
    breaths: 0, frees: 0, freebits: 0, freebytes: 0
};
pub fn add_frees    ()           { unsafe { STATS.frees += 1 } }
pub fn add_freebytes(bytes: u64) { unsafe { STATS.freebytes += bytes; } }
pub fn add_freebits (bits: u64)  { unsafe { STATS.freebits += bits; } }
pub fn stats() -> &'static EngineStats { unsafe { &STATS } }

// Global engine state; singleton obtained via engine::state()
//
// The set of all active apps and links in the system, indexed by name.
pub struct EngineState {
    pub link_table: HashMap<String, SharedLink>,
    pub app_table: HashMap<String, AppState>,
    pub inhale: Vec<String>,
    pub exhale: Vec<String>
}
static mut STATE: Lazy<EngineState> = Lazy::new(
    || EngineState { app_table: HashMap::new(),
                     link_table: HashMap::new(),
                     inhale: Vec::new(),
                     exhale: Vec::new() }
);
pub fn state() -> &'static EngineState { unsafe { &STATE } }

// Type for links shared between apps.
//
// Links are borrowed at runtime by apps to perform packet I/O, or via the
// global engine state (to query link statistics etc.)
pub type SharedLink = Rc<RefCell<link::Link>>;

// State for a sigle app instance managed by the engine
//
// Tracks a reference to the AppConfig used to instantiate the app, and maps of
// its active input and output links.
pub struct AppState {
    pub app: Box<dyn App>,
    pub conf: Box<dyn AppArg>,
    pub input: HashMap<String, SharedLink>,
    pub output: HashMap<String, SharedLink>
}

// Callbacks that can be implented by apps
//
//   pull: inhale packets into the app network (put them onto output links)
//   push: exhale packets out the the app network (move them from input links
//         to output links, or peripheral device queues)
//   stop: stop the app (deinitialize)
//   report: print information about itself
pub trait App {
    fn has_pull(&self) -> bool { false }
    fn pull(&self, _app: &AppState) { panic!("Pull called but not implemented"); }
    fn has_push(&self) -> bool { false }
    fn push(&self, _app: &AppState) { panic!("Push called but not implemented"); }
    fn has_report(&self) -> bool { false }
    fn report(&self) { panic!("Report called but not implemented"); }
    fn has_stop(&self) -> bool { false }
    fn stop(&self) { panic!("Stop called but not implemented"); }
}
// Recommended number of packets to inhale in pull()
pub const PULL_NPACKETS: usize = link::LINK_MAX_PACKETS / 10;

// Constructor trait/callback for app instance specifications
//
//   new: initialize and return app (resulting app must implement App trait)
//
// Objects that implement the AppConfig trait can be used to configure apps
// via config::app().
pub trait AppConfig: std::fmt::Debug {
    fn new(&self) -> Box<dyn App>;
}

// Trait used internally by engine/config to provide an equality predicate for
// implementors of AppConfig. Sort of a hack based on the Debug trait.
//
// Auto-implemented for all implementors of AppConfig.
pub trait AppArg: AppConfig + AppClone {
    fn identity(&self) -> String { format!("{}::{:?}", module_path!(), self) }
    fn equal(&self, y: &dyn AppArg) -> bool { self.identity() == y.identity() }
}
impl<T: AppConfig + AppClone> AppArg for T { }

// We need to be able to copy (clone) AppConfig objects from configurations
// into the engine state. However, the Rust compiler does not allow
// AppConfig/AppArg to implement Clone(/Sized) if we want to use them for trait
// objects.
//
// The AppClone trait below (which we can bind AppArg to) auto-implements a
// box_clone[1] method for all implementors of AppConfig as per
// https://users.rust-lang.org/t/solved-is-it-possible-to-clone-a-boxed-trait-object/1714/6
pub trait AppClone: AppConfig {
    fn box_clone(&self) -> Box<dyn AppArg>;
}
impl<T: AppConfig + Clone + 'static> AppClone for T {
    fn box_clone(&self) -> Box<dyn AppArg> { Box::new((*self).clone()) }
}
impl Clone for Box<dyn AppArg> {
    fn clone(&self) -> Self { (*self).box_clone() }
}

// Configure the running app network to match (new) config.
//
// Successive calls to configure() will migrate from the old to the
// new app network by making the changes needed.
pub fn configure(config: &config::Config) {
    let state = unsafe { &mut STATE };
    // First determine the links that are going away and remove them.
    for link in state.link_table.clone().keys() {
        if config.links.get(link).is_none() {
            unlink_apps(state, link)
        }
    }
    // Do the same for apps.
    let apps: Vec<_> = state.app_table.keys().map(Clone::clone).collect();
    for name in apps {
        let old = &state.app_table.get(&name).unwrap().conf;
        match config.apps.get(&name) {
            Some(new) => if !old.equal(&**new) { stop_app(state, &name) },
            None => stop_app(state, &name)
        }
    }
    // Start new apps.
    for (name, app) in config.apps.iter() {
        if state.app_table.get(name).is_none() {
            start_app(state, name, &**app)
        }
    }
    // Rebuild links.
    for link in config.links.iter() {
        link_apps(state, link);
    }
    // Compute breathe order.
    compute_breathe_order(state);
}

// Insert new app instance into network.
fn start_app(state: &mut EngineState, name: &str, conf: &dyn AppArg) {
    let conf = conf.box_clone();
    state.app_table.insert(name.to_string(),
                           AppState { app: conf.new(),
                                      conf: conf,
                                      input: HashMap::new(),
                                      output: HashMap::new() });
}

// Remove app instance from network.
fn stop_app (state: &mut EngineState, name: &str) {
    let removed = state.app_table.remove(name).unwrap();
    if removed.app.has_stop() { removed.app.stop(); }
}

// Allocate a fresh shared link.
fn new_shared_link() -> SharedLink { Rc::new(RefCell::new(link::new())) }

// Link two apps in the network.
fn link_apps(state: &mut EngineState, spec: &str) {
    let link = state.link_table.entry(spec.to_string())
        .or_insert_with(new_shared_link);
    let spec = config::parse_link(spec);
    state.app_table.get_mut(&spec.from).unwrap()
        .output.insert(spec.output, link.clone());
    state.app_table.get_mut(&spec.to).unwrap()
        .input.insert(spec.input, link.clone());
}

// Remove link between two apps.
fn unlink_apps(state: &mut EngineState, spec: &str) {
    state.link_table.remove(spec);
    let spec = config::parse_link(spec);
    state.app_table.get_mut(&spec.from).unwrap()
        .output.remove(&spec.output);
    state.app_table.get_mut(&spec.to).unwrap()
        .input.remove(&spec.input);
}

// Compute engine breathe order
//
// Ensures that the order in which pull/push callbacks are processed in
// breathe()...
//   - follows link dependencies when possible (to optimize for latency)
//   - executes each app’s callbacks at most once (cycles imply that some
//     packets may remain on links after breathe() returns)
//   - is deterministic with regard to the configuration
fn compute_breathe_order(state: &mut EngineState) {
    state.inhale.clear();
    state.exhale.clear();
    // Build map of successors
    let mut successors: HashMap<String, HashSet<String>> = HashMap::new();
    for link in state.link_table.keys() {
        let spec = config::parse_link(&link);
        successors.entry(spec.from).or_insert(HashSet::new()).insert(spec.to);
    }
    // Put pull apps in inhalers
    for (name, app) in state.app_table.iter() {
        if app.app.has_pull() {
            state.inhale.push(name.to_string());
        }
    }
    // Sort inhalers by name (to ensure breathe order determinism)
    state.inhale.sort();
    // Collect initial dependents
    let mut dependents = Vec::new();
    for name in &state.inhale {
        if let Some(successors) = successors.get(name) {
            for successor in successors.iter() {
                let app = state.app_table.get(successor).unwrap();
                if app.app.has_push() && !dependents.contains(successor) {
                    dependents.push(successor.to_string());
                }
            }
        }
    }
    // Remove processed successors (resolved dependencies)
    for name in &state.inhale { successors.remove(name); }
    // Compute sorted push order
    while dependents.len() > 0 {
        // Attempt to delay dependents after their inputs, but break cycles by
        // selecting at least one dependent.
        let mut selected = HashSet::new();
        for name in dependents.clone() {
            if let Some(successors) = successors.get(&name) {
                for successor in successors.iter() {
                    if !selected.contains(successor) &&
                        dependents.contains(successor) &&
                        dependents.len() > 1
                    {
                        selected.insert(name.to_string());
                        dependents.retain(|name| name != successor);
                    }
                }
            }
        }
        // Sort dependents by name (to ensure breathe order determinism)
        dependents.sort();
        // Drain and append dependents to exhalers
        let exhaled = dependents.clone();
        state.exhale.append(&mut dependents);
        // Collect further dependents
        for name in &exhaled {
            if let Some(successors) = successors.get(name) {
                for successor in successors.iter() {
                    let app = state.app_table.get(successor).unwrap();
                    if app.app.has_push() && 
                        !state.exhale.contains(successor) && 
                        !dependents.contains(successor)
                    {
                        dependents.push(successor.to_string());
                    }
                }
            }
        }
        // Remove processed successors (resolved dependencies)
        for name in &exhaled { successors.remove(name); }
    }
}

// Call this to “run snabb”.
pub fn main(options: Option<Options>) {
    let options = match options {
        Some(options) => options,
        None => Options{..Default::default()}
    };
    let mut done = options.done;
    if let Some(duration) = options.duration {
        if done.is_some() { panic!("You can not have both 'duration' and 'done'"); }
        done = Some(timeout(duration));
    }

    breathe();
    while match &done { Some(done) => !done(), None => true } {
        pace_breathing();
        breathe();
    }
    if !options.no_report {
        if options.report_load  { report_load(); }
        if options.report_links { report_links(); }
        if options.report_apps  { report_apps(); }
    }

    unsafe { MONOTONIC_NOW = None; }
}

// Engine breathe loop Options
//
//  done: run the engine until predicate returns true
//  duration: run the engine for duration (mutually exclusive with 'done')
//  no_report: disable engine reporting before return
//  report_load: print a load report upon return
//  report_links: print summarized statistics for each link upon return
//  report_apps: print app defined report for each app
#[derive(Default)]
pub struct Options {
    pub done: Option<Box<dyn Fn() -> bool>>,
    pub duration: Option<Duration>,
    pub no_report: bool,
    pub report_load: bool,
    pub report_links: bool,
    pub report_apps: bool
}

// Return current monotonic time.
// Can be used to drive timers in apps.
static mut MONOTONIC_NOW: Option<Instant> = None;
pub fn now() -> Instant {
    match unsafe { MONOTONIC_NOW } {
        Some(instant) => instant,
        None => Instant::now()
    }
}

// Make a closure which when called returns true after duration,
// and false otherwise.
pub fn timeout(duration: Duration) -> Box<dyn Fn() -> bool> {
    let deadline = now() + duration;
    Box::new(move || now() > deadline)
}

// Return a throttle function.
//
// The throttle returns true at most once in any <duration> time interval.
pub fn throttle(duration: Duration) -> Box<dyn FnMut() -> bool> {
    let mut deadline = now();
    Box::new(move || if now() > deadline { deadline = now() + duration; true }
                     else                { false })
}

// Perform a single breath (inhale / exhale)
fn breathe() {
    unsafe { MONOTONIC_NOW = Some(Instant::now()); }
    for name in &state().inhale {
        let app = state().app_table.get(name).unwrap();
        app.app.pull(&app);
    }
    for name in &state().exhale {
        let app = state().app_table.get(name).unwrap();
        app.app.push(&app);
    }
    unsafe { STATS.breaths += 1; }
}

// Breathing regluation to reduce CPU usage when idle by calling sleep.
//
// Dynamic adjustment automatically scales the time to sleep between
// breaths from nothing up to MAXSLEEP (default: 100us). If packets
// are processed during a breath then the SLEEP period is halved, and
// if no packets are processed during a breath then the SLEEP interval
// is increased by one microsecond.
static mut LASTFREES: u64 = 0;
static mut SLEEP: u64 = 0;
const MAXSLEEP: u64 = 100;
fn pace_breathing() {
    unsafe {
        if LASTFREES == STATS.frees {
            SLEEP = min(SLEEP + 1, MAXSLEEP);
            sleep(Duration::from_micros(SLEEP));
        } else {
            SLEEP /= 2;
        }
        LASTFREES = STATS.frees;
    }
}

// Load reporting prints several metrics:
//   time  - period of time that the metrics were collected over
//   fps   - frees per second (how many calls to packet::free())
//   fpb   - frees per breath
//   bpp   - bytes per packet (average packet size)
//   sleep - usecs of sleep between breaths
static mut LASTLOADREPORT: Option<Instant> = None;
static mut REPORTEDFREES: u64 = 0;
static mut REPORTEDFREEBITS: u64 = 0;
static mut REPORTEDFREEBYTES: u64 = 0;
static mut REPORTEDBREATHS: u64 = 0;
pub fn report_load() {
    unsafe {
        let frees = STATS.frees;
        let freebits = STATS.freebits;
        let freebytes = STATS.freebytes;
        let breaths = STATS.breaths;
        if let Some(lastloadreport) = LASTLOADREPORT {
            let interval = now().duration_since(lastloadreport).as_secs_f64();
            let newfrees = frees - REPORTEDFREES;
            let newbits = freebits - REPORTEDFREEBITS;
            let newbytes = freebytes - REPORTEDFREEBYTES;
            let newbreaths = breaths - REPORTEDBREATHS;
            let fps = (newfrees as f64 / interval) as u64;
            let fbps = newbits as f64 / interval;
            let fpb = if newbreaths > 0 { newfrees / newbreaths } else { 0 };
            let bpp = if newfrees > 0 { newbytes / newfrees } else { 0 };
            println!("load: time: {:.2} fps: {} fpGbps: {:.3} fpb: {} bpp: {} sleep: {}",
                     interval,
                     lib::comma_value(fps),
                     fbps / 1e9,
                     lib::comma_value(fpb),
                     lib::comma_value(bpp),
                     SLEEP);
        }
        LASTLOADREPORT = Some(now());
        REPORTEDFREES = frees;
        REPORTEDFREEBITS = freebits;
        REPORTEDFREEBYTES = freebytes;
        REPORTEDBREATHS = breaths;
    }
}

// Print a link report (packets sent, percent dropped)
pub fn report_links() {
    println!("Link report:");
    let mut names: Vec<_> = state().link_table.keys().collect();
    names.sort();
    for name in names {
        let link = state().link_table.get(name).unwrap().borrow();
        let txpackets = link.txpackets;
        let txdrop = link.txdrop;
        println!("  {} sent on {} (loss rate: {}%)",
                 lib::comma_value(txpackets),
                 name,
                 loss_rate(txdrop, txpackets));
    }
}

// Print a report of all active apps
pub fn report_apps() {
    for (name, app) in state().app_table.iter() {
        println!("App report for {}:", name);
        match app.input.len()
        { 0 => (),
          1 => println!("  receiving from one input link"),
          n => println!("  receiving from {} input links", n) }
        match app.output.len()
        { 0 => (),
          1 => println!("  transmitting to one output link"),
          n => println!("  transmitting to {} output links", n) }
        if app.app.has_report() { app.app.report(); }
    }
}

fn loss_rate(drop: u64, sent: u64) -> u64 {
    if sent == 0 { return 0; }
    drop * 100 / (drop + sent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use crate::basic_apps;

    #[test]
    fn engine() {
        let mut c = config::new();
        config::app(&mut c, "source", &basic_apps::Source {size: 60});
        config::app(&mut c, "sink", &basic_apps::Sink {});
        config::link(&mut c, "source.output -> sink.input");
        configure(&c);
        println!("Configured the app network: source(60).output -> sink.input");
        main(Some(Options{
            duration: Some(Duration::new(0,0)),
            report_load: true, report_links: true,
            ..Default::default()
        }));
        let mut c = c.clone();
        config::app(&mut c, "source", &basic_apps::Source {size: 120});
        configure(&c);
        println!("Cloned, mutated, and applied new configuration:");
        println!("source(120).output -> sink.input");
        main(Some(Options{
            done: Some(Box::new(|| true)),
            report_load: true, report_links: true,
            ..Default::default()
        }));
        let stats = stats();
        println!("engine: frees={} freebytes={} freebits={}",
                 stats.frees, stats.freebytes, stats.freebits);
    }

    #[test]
    fn breathe_order() {
        println!("Case 1:");
        let mut c = config::new();
        config::app(&mut c, "a_io1", &PseudoIO {});
        config::app(&mut c, "b_t1", &basic_apps::Tee {});
        config::app(&mut c, "c_t2", &basic_apps::Tee {});
        config::app(&mut c, "d_t3", &basic_apps::Tee {});
        config::link(&mut c, "a_io1.output -> b_t1.input");
        config::link(&mut c, "b_t1.output -> c_t2.input");
        config::link(&mut c, "b_t1.output2 -> d_t3.input");
        config::link(&mut c, "d_t3.output -> b_t1.input2");
        configure(&c);
        report_links();
        for name in &state().inhale { println!("pull {}", &name); }
        for name in &state().exhale { println!("push {}", &name); }
        println!("Case 2:");
        let mut c = config::new();
        config::app(&mut c, "a_io1", &PseudoIO {});
        config::app(&mut c, "b_t1", &basic_apps::Tee {});
        config::app(&mut c, "c_t2", &basic_apps::Tee {});
        config::app(&mut c, "d_t3", &basic_apps::Tee {});
        config::link(&mut c, "a_io1.output -> b_t1.input");
        config::link(&mut c, "b_t1.output -> c_t2.input");
        config::link(&mut c, "b_t1.output2 -> d_t3.input");
        config::link(&mut c, "c_t2.output -> d_t3.input2");
        configure(&c);
        report_links();
        for name in &state().inhale { println!("pull {}", &name); }
        for name in &state().exhale { println!("push {}", &name); }
        println!("Case 3:");
        let mut c = config::new();
        config::app(&mut c, "a_io1", &PseudoIO {});
        config::app(&mut c, "b_t1", &basic_apps::Tee {});
        config::app(&mut c, "c_t2", &basic_apps::Tee {});
        config::link(&mut c, "a_io1.output -> b_t1.input");
        config::link(&mut c, "a_io1.output2 -> c_t2.input");
        config::link(&mut c, "b_t1.output -> a_io1.input");
        config::link(&mut c, "b_t1.output2 -> c_t2.input2");
        config::link(&mut c, "c_t2.output -> a_io1.input2");
        configure(&c);
        report_links();
        for name in &state().inhale { println!("pull {}", &name); }
        for name in &state().exhale { println!("push {}", &name); }
    }

    #[derive(Clone,Debug)]
    pub struct PseudoIO {}
    impl AppConfig for PseudoIO {
        fn new(&self) -> Box<dyn App> { Box::new(PseudoIOApp {}) }
    }
    pub struct PseudoIOApp {}
    impl App for PseudoIOApp {
        fn has_pull(&self) -> bool { true }
        fn has_push(&self) -> bool { true }
    }

}
