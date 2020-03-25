use super::engine;
use super::config;
use super::basic_apps;
use super::rawsocket_app;
use super::qos;
use super::offload;
use super::flow;

use std::env;
use std::process;

use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::io;
use std::collections::HashSet;

use regex::Regex;
use once_cell::sync::Lazy;

use serde::Serialize;
use serde::Deserialize;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use signal_hook::consts::signal::*;
use signal_hook::flag as signal_flag;


// Le program

pub fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 6 {
        println!("Invalid number of arguments.");
        print_usage(&args[0]);
        process::exit(1);
    }
    let outer_ifname = &args[1];
    let inner_ifname = &args[2];
    let specpath = &args[3];
    let ingress_profile = &args[4];
    let egress_profile = &args[5];

    loop {
        // Try to load and realize QoS spec
        if let Some(spec) = try_read_qos_spec(specpath) {
            let mut c = config::new();
            configure_synthetic_network(
                &mut c,
                outer_ifname, inner_ifname,
                ingress_profile, egress_profile,
                &spec
            );
            engine::configure(&c);
        }
        // Run engine until SIGHUP is received
        engine::main(Some(engine::Options {
            done: Some(signal_received(SIGHUP)),
            ..Default::default()
        }));
        engine::report_load();
    }
    
}

fn print_usage(exe: &str) {
    println!("Usage: {} <outer_ifname> <inner_ifname> <specpath> <ingress_profile> <egress_profile>", exe);
    let spec = SyntheticNetwork {
        default_link: SyntheticLink {
            ingress: QoS {
                rate: 10_000_000,
                loss: 0.0,
                latency: 0,
                jitter: 0,
                jitter_strength: 0.0,
                reorder_packets: false
            },
            egress: QoS {
                rate: 1_000_000,
                loss: 0.0,
                latency: 0,
                jitter: 0,
                jitter_strength: 0.0,
                reorder_packets: false
            }
        },
        flows: vec![
            SyntheticFlow {
                label: "http".to_string(),
                flow: Flow {
                    ip: 0,
                    protocol: 6,
                    port_min: 80,
                    port_max: 80
                },
                link: SyntheticLink {
                    ingress: QoS {
                        rate: 100_000_000,
                        loss: 0.0,
                        latency: 0,
                        jitter: 0,
                        jitter_strength: 0.0,
                        reorder_packets: false
                    },
                    egress: QoS {
                        rate: 100_000_000,
                        loss: 0.0,
                        latency: 0,
                        jitter: 0,
                        jitter_strength: 0.0,
                        reorder_packets: false
                    }
                }
            }
        ]
    };
    println!("Example config for <specpath>: {}",
             serde_json::to_string(&spec).unwrap());
}


// Translate JSON configuration (QoS spec) into app network config

fn configure_synthetic_network
    (config: &mut config::Config,
     outer_ifname: &str, inner_ifname: &str,
     ingress_profile: &str, egress_profile: &str,
     spec: &SyntheticNetwork)
{
    configure_interface(config, outer_ifname);
    configure_interface(config, inner_ifname);

    // Ingress path: outer → inner

    let outer_tsd = format!("{}_tsd", outer_ifname);
    configure_tsd(config, &outer_tsd, outer_ifname, 1400); // MSS: 1400

    let outer_offload = format!("{}_offload", outer_ifname);
    configure_offload(config, &outer_offload, &outer_tsd);

    let outer_rx = format!("{}.output", outer_offload);
    let inner_tx = format!("{}.input", inner_ifname);
    
    let outer_top = format!("{}_top", outer_ifname);
    configure_top(config, &outer_top, &inner_tx,
                  ingress_profile, flow::Dir::Src);

    let outer_split = format!("{}_split", outer_ifname);
    let outer_split_default = format!("{}.default", outer_split);
    configure_split(config, &outer_split, &outer_rx,
                    &spec.flows, flow::Dir::Src);

    let inner_join = format!("{}_join", inner_ifname);
    let inner_join_default = format!("{}.default", inner_join);
    configure_join(config, &inner_join, &outer_top);

    configure_qos(config, "ingress", &outer_split_default, &inner_join_default,
                  &spec.default_link.ingress);

    configure_flows(config, &outer_split, &inner_join,
                    &spec.flows, flow::Dir::Src);

    // Egress path: inner → outer

    let inner_tsd = format!("{}_tsd", inner_ifname);
    configure_tsd(config, &inner_tsd, inner_ifname, 1400); // MSS: 1400

    let inner_offload = format!("{}_offload", inner_ifname);
    configure_offload(config, &inner_offload, &inner_tsd);

    let inner_rx = format!("{}.output", inner_offload);
    let outer_tx = format!("{}.input", outer_ifname);
    
    let inner_top = format!("{}_top", inner_ifname);
    configure_top(config, &inner_top, &outer_tx,
                  egress_profile, flow::Dir::Dst);

    let inner_split = format!("{}_split", inner_ifname);
    let inner_split_default = format!("{}.default", inner_split);
    configure_split(config, &inner_split, &inner_rx,
                    &spec.flows, flow::Dir::Dst);

    let outer_join = format!("{}_join", outer_ifname);
    let outer_join_default = format!("{}.default", outer_join);
    configure_join(config, &outer_join, &inner_top);

    configure_qos(config, "egress", &inner_split_default, &outer_join_default,
                  &spec.default_link.egress);

    configure_flows(config, &inner_split, &outer_join,
                    &spec.flows, flow::Dir::Dst);
}

fn configure_interface
    (config: &mut config::Config,
     ifname: &str)
{
    config::app(config, ifname, &rawsocket_app::RawSocket {
        ifname: ifname.to_string()
    });
}

fn configure_tsd
    (config: &mut config::Config,
     name: &str, ifname: &str, mss: u16)
{
    let output_to_tsd = format!("{}.output -> {}.input", ifname, name);
    config::app(config, name, &offload::TSD {mss: mss});
    config::link(config, &output_to_tsd);
}

fn configure_top
    (config: &mut config::Config,
     name: &str, output: &str, path: &str, dir: flow::Dir)
{
    let top_to_input = format!("{}.output -> {}", name, output);
    config::app(config, name, &flow::Top {path: path.to_string(), dir: dir});
    config::link(config, &top_to_input);
}

fn configure_offload
    (config: &mut config::Config,
     name: &str, ifname: &str)
{
    let output_to_offload = format!("{}.output -> {}.input", ifname, name);
    config::app(config, name, &offload::Checksum {});
    config::link(config, &output_to_offload);
}

fn configure_split
    (config: &mut config::Config,
     name: &str, input: &str,
     synthetic_flows: &Vec<SyntheticFlow>, dir: flow::Dir)
{
    let mut flows = Vec::new();
    for synthetic_flow in synthetic_flows {
        flows.push(flow::Flow {
            label: synthetic_flow.label.to_string(),
            dir: dir,
            ip: synthetic_flow.flow.ip,
            protocol: synthetic_flow.flow.protocol,
            port_min: synthetic_flow.flow.port_min,
            port_max: synthetic_flow.flow.port_max
        });
    }
    let input_to_split = format!("{} -> {}.input", input, name);
    config::app(config, name, &flow::Split {flows: flows});
    config::link(config, &input_to_split);
}

fn configure_join
    (config: &mut config::Config,
     name: &str, output: &str)
{
    let join_to_output = format!("{}.output -> {}.input", name, output);
    config::app(config, name, &basic_apps::Join {});
    config::link(config, &join_to_output);
}

fn configure_flows
    (config: &mut config::Config,
     split: &str, join: &str,
     synthetic_flows: &Vec<SyntheticFlow>, dir: flow::Dir)
{
    let prefix = match dir {
        flow::Dir::Src => "ingress",
        flow::Dir::Dst => "egress"
    };
    for synthetic_flow in synthetic_flows {
        let input = format!("{}.{}", split, synthetic_flow.label);
        let output = format!("{}.{}", join, synthetic_flow.label);
        let app_label = format!("{}_{}", prefix, synthetic_flow.label);
        let qos = match dir {
            flow::Dir::Src => &synthetic_flow.link.ingress,
            flow::Dir::Dst => &synthetic_flow.link.egress
        };
        configure_qos(config, &app_label, &input, &output, qos);
    }
}

fn configure_qos
    (config: &mut config::Config,
     label: &str, input: &str, output: &str, qos: &QoS)
{
    // Capacity of queues used to delay packets
    // Hardcoded to a value we’re likely not to exceed, i.e:
    //  100,000 is good for delaying ~100K packets per second for 1 second
    //  (or ~1 Mpps for 100ms, etc.)
    // If this value is too small we’ll start dropping packets that would
    // overflow the queues, so re-evaluate once we have a good idea of our peak
    // pps, and pick a value that can generously handle that (like 3x or
    // something) for a feel-good margin and reasonable memory use.
    let delay_queue_capacity = 100_000;

    let rate = format!("rate_{}", label);
    let input_to_rate = format!("{} -> {}.input", input, rate);
    let loss = format!("loss_{}", label);
    let rate_to_loss = format!("{}.output -> {}.input", rate, loss);
    let latency = format!("latency_{}", label);
    let loss_to_latency = format!("{}.output -> {}.input", loss, latency);
    let jitter = format!("jitter_{}", label);
    let latency_to_jitter = format!("{}.output -> {}.input", latency, jitter);
    let jitter_to_output = format!("{}.output -> {}", jitter, output);


    config::link(config, &input_to_rate);
    config::app(config, &rate, &qos::RateLimiter {
        rate: qos.rate
    });
    config::link(config, &rate_to_loss);
    config::app(config, &loss, &qos::Loss {
        ratio: qos.loss.clamp(0.0, 1.0)
    });
    config::link(config, &loss_to_latency);
    config::app(config, &latency, &qos::Latency {
        ms: qos.latency,
        capacity: delay_queue_capacity
    });
    config::link(config, &latency_to_jitter);
    config::app(config, &jitter, &qos::Jitter {
        ms: qos.jitter,
        strength: qos.jitter_strength.clamp(0.0, 1.0),
        reorder: qos.reorder_packets,
        capacity: delay_queue_capacity
    });
    config::link(config, &jitter_to_output);
}


// This is our QoS spec / configuration format

#[derive(Serialize,Deserialize)]
struct SyntheticNetwork {
    default_link: SyntheticLink,
    flows: Vec<SyntheticFlow>
}
#[derive(Serialize,Deserialize)]
struct SyntheticLink {
    ingress: QoS,
    egress: QoS
}
#[derive(Serialize,Deserialize)]
struct QoS {
    rate: u64,
    loss: f64,
    latency: u64,
    jitter: u64,
    jitter_strength: f64,
    reorder_packets: bool
}
#[derive(Serialize,Deserialize)]
struct SyntheticFlow {
    label: String,
    flow: Flow,
    link: SyntheticLink
}
#[derive(Serialize,Deserialize)]
struct Flow {
    ip: u32,
    protocol: u8,
    port_min: u16,
    port_max: u16
}


// Parse a QoS spec from a JSON file

fn read_qos_spec(path: &str) -> Result<SyntheticNetwork, Box<dyn Error>> {
    let file = File::open(path)?;
    let spec = serde_json::from_reader(BufReader::new(file))?;
    sanitize_labels(&spec)?;
    Ok(spec)
}

fn try_read_qos_spec(path: &str) -> Option<SyntheticNetwork> {
    match read_qos_spec(path) {
        Ok(spec) => Some(spec),
        Err(error) => {
            println!("Warning: failed to read {} ({})", path, error);
            None
        }
    }
}

fn sanitize_labels(spec: &SyntheticNetwork) -> Result<(), Box<io::Error>> {
    let mut labels = HashSet::new();
    for synthetic_flow in &spec.flows {
        if synthetic_flow.label == "default" {
            return Err(Box::new(
                io::Error::new(io::ErrorKind::InvalidData,
                               "Flow label 'default' is reserved.")))
        }
        if !LABEL_SYNTAX.is_match(&synthetic_flow.label) {
            return Err(Box::new(
                io::Error::new(io::ErrorKind::InvalidData,
                               "Invalid characters in flow label.")))
        }
        if labels.contains(&synthetic_flow.label) {
            return Err(Box::new(
                io::Error::new(io::ErrorKind::InvalidData,
                               "Duplicate flow labels.")))
        }
        labels.insert(synthetic_flow.label.to_string());
    }
    Ok(())
}
static LABEL_SYNTAX: Lazy<Regex> = Lazy::new
    (|| Regex::new(r"^[\w_]+$").unwrap());


// Signal handling (for catching SIGHUP)

// See https://docs.rs/signal-hook/0.3.6/signal_hook/flag/index.html#examples
// “Reloading a configuration on SIGHUP (which is a common behaviour of many
// UNIX daemons, together with reopening the log file).”
fn signal_received(signal: i32) -> Box<dyn Fn() -> bool> {
    let flag = Arc::new(AtomicBool::new(false));
    signal_flag::register(signal, Arc::clone(&flag))
        .expect("Cannot register signal handler");
    // Return a closure () -> bool that returns true whenever we
    // receive `signal'
    Box::new(move || flag.swap(false, Ordering::Relaxed))
}
    