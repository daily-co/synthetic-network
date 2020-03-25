#![allow(dead_code)]
#![feature(test)]
#![feature(asm)]

mod memory;
mod packet;
mod link;
mod engine;
mod config;
mod lib;
mod basic_apps;
mod header;
mod ethernet;
mod ipv4;
mod tcp;
mod udp;
mod checksum;
mod rawsocket_app;
mod qos;
mod offload;
mod flow;

mod synthetic_network;


fn main() {
    synthetic_network::main();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration,Instant};

    #[test]
    fn basic1 () {
        let npackets = match std::env::var("RUSH_BASIC1_NPACKETS") {
            Ok(val) => val.parse::<f64>().unwrap() as u64,
            _ => 1_000_000
        };
        let mut c = config::new();
        config::app(&mut c, "Source", &basic_apps::Source {size: 60});
        config::app(&mut c, "Tee", &basic_apps::Tee {});
        config::app(&mut c, "Sink", &basic_apps::Sink {});
        config::link(&mut c, "Source.tx -> Tee.rx");
        config::link(&mut c, "Tee.tx1 -> Sink.rx1");
        config::link(&mut c, "Tee.tx2 -> Sink.rx2");
        engine::configure(&c);
        let start = Instant::now();
        let output = engine::state().app_table
            .get("Source").unwrap()
            .output.get("tx").unwrap();
        let mut report = engine::throttle(Duration::new(1, 0));
        while output.borrow().txpackets < npackets {
            engine::main(Some(engine::Options{
                duration: Some(Duration::new(0, 10_000_000)), // 0.01s
                no_report: true,
                ..Default::default()
            }));
            if report() { engine::report_load(); }
        }
        let finish = Instant::now();
        let runtime = finish.duration_since(start).as_secs_f64();
        let packets = output.borrow().txpackets as f64;
        println!("Processed {:.1} million packets in {:.2} seconds (rate: {:.1} Mpps).",
                 packets / 1e6, runtime, packets / runtime / 1e6);
    }

}