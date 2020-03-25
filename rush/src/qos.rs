use super::packet;
use super::link;
use super::engine;

// QoS: quality of service regulating apps

use std::time::{Duration, Instant};
use std::collections::VecDeque;
use std::cell::RefCell;
use std::cmp::min;
use rand::Rng;


// Loss app: simulate probabilistic packet loss

#[derive(Clone,Debug)]
pub struct Loss {
    // ratio 0..1 of dropped packets (0.0 → 0%, 0.5 → 50%, 1.0 → 100%)
    pub ratio: f64
}
impl engine::AppConfig for Loss {
    fn new(&self) -> Box<dyn engine::App> {
        assert!(self.ratio >= 0.0 && self.ratio <= 1.0,
                "Ratio must be within 0.0 and 1.0");
        Box::new(LossApp {ratio: self.ratio})
    }
}
pub struct LossApp { ratio: f64 }
impl engine::App for LossApp {
    fn has_push(&self) -> bool { true }
    fn push(&self, app: &engine::AppState) {
        let mut input = app.input.get("input").unwrap().borrow_mut();
        let mut output = app.output.get("output").unwrap().borrow_mut();
        let mut rng = rand::thread_rng();
        while !link::empty(&input) {
            let p = link::receive(&mut input);
            if rng.gen::<f64>() >= self.ratio {
                link::transmit(&mut output, p);
            } else {
                packet::free(p);
            }
        }
    }
}


// Latency app: simulate constant latency

#[derive(Clone,Debug)]
pub struct Latency {
    pub ms: u64, // milliseconds of latency
    pub capacity: usize // delay queue capacity
}
impl engine::AppConfig for Latency {
    fn new(&self) -> Box<dyn engine::App> {
        Box::new(LatencyApp {
            ms: self.ms,
            queue: RefCell::new(DelayQueue::new(self.capacity))
        })
    }
}
pub struct LatencyApp {
    ms: u64,
    queue: RefCell<DelayQueue>
}
impl engine::App for LatencyApp {
    fn has_push(&self) -> bool { true }
    fn push(&self, app: &engine::AppState) {
        let mut input = app.input.get("input").unwrap().borrow_mut();
        let mut queue = self.queue.borrow_mut();
        // Enqueue delay
        if !link::empty(&input) && !queue.full() {
            let ttx = engine::now() + Duration::from_millis(self.ms);
            queue.enqueue_delay(ttx);
        }
        // Enqueue packet batch
        while !link::empty(&input) && !queue.full() {
            queue.enqueue_packet(link::receive(&mut input));
        }
    }
    fn has_pull(&self) -> bool { true }
    fn pull(&self, app: &engine::AppState) {
        let mut output = app.output.get("output").unwrap().borrow_mut();
        let mut queue = self.queue.borrow_mut();
        // Forward queued packets ready to transmit
        while !queue.empty() && queue.need_tx() {
            link::transmit(&mut output, queue.dequeue_packet());
        }
    }
}

// Jitter app: simulate random latency jitter
// XXX - jitter should probably be normally distributed?

#[derive(Clone,Debug)]
pub struct Jitter {
    pub ms: u64, // milliseconds of maximum jitter
    pub strength: f64, // jitter strength (0.0 → no jitter, 1.0 → very strong jitter)
    pub reorder: bool, // should jitter reorder packets?
    pub capacity: usize // delay queue capacity
}
impl engine::AppConfig for Jitter {
    fn new(&self) -> Box<dyn engine::App> {
        Box::new(JitterApp {
            us: self.ms as f64 * 1000.0,
            strength: self.strength,
            reorder: self.reorder,
            queue: RefCell::new(DelayQueue::new(self.capacity))
        })
    }
}
pub struct JitterApp {
    us: f64,
    strength: f64,
    reorder: bool,
    queue: RefCell<DelayQueue>
}
impl engine::App for JitterApp {
    fn has_push(&self) -> bool { true }
    fn push(&self, app: &engine::AppState) {
        let mut input = app.input.get("input").unwrap().borrow_mut();
        let mut output = app.output.get("output").unwrap().borrow_mut();
        let mut queue = self.queue.borrow_mut();
        let mut rng = rand::thread_rng();
        // Add jitter to incoming packets
        while !link::empty(&input) && !queue.full() {
            let add_jitter = rng.gen::<f64>() < self.strength;
            if add_jitter {
                let jitter = (self.us * rng.gen::<f64>()) as u64;
                let ttx = engine::now() + Duration::from_micros(jitter);
                queue.enqueue_delay(ttx);
            }
            if !add_jitter && self.reorder {
                // If reorder=true then forward packets without added jitter
                // immediately, effectively reordering them
                link::transmit(&mut output, link::receive(&mut input));
            } else if !queue.full() {
                queue.enqueue_packet(link::receive(&mut input));
            }
        }
    }
    fn has_pull(&self) -> bool { true }
    fn pull(&self, app: &engine::AppState) {
        let mut output = app.output.get("output").unwrap().borrow_mut();
        let mut queue = self.queue.borrow_mut();
        // Forward packets with jitter delay
        while !queue.empty() && queue.need_tx() {
            link::transmit(&mut output, queue.dequeue_packet());
        }
    }
}

struct DelayQueue {
    packets: VecDeque<DelayedPacket>,
    capacity: usize
}
enum DelayedPacket {
    Delay(Instant),
    Packet(Box<packet::Packet>)
}
impl DelayQueue {
    fn new(capacity: usize) -> DelayQueue {
        DelayQueue {
            packets: VecDeque::with_capacity(capacity),
            capacity: capacity
        }
    }
    fn full(&self) -> bool {
        self.packets.len() >= self.capacity
    }
    fn empty(&self) -> bool {
        self.packets.is_empty()
    }
    fn peek(&mut self) -> &DelayedPacket {
        match self.packets.front() {
            Some(entry) => entry,
            None => panic!("Queue underflow.")
        }
    }
    fn enqueue_delay(&mut self, ttx: Instant) {
        if self.full() { panic!("Queue overflow.") }
        self.packets.push_back(DelayedPacket::Delay(ttx));
    }
    fn enqueue_packet(&mut self, p: Box<packet::Packet>) {
        if self.full() { panic!("Queue overflow.") }
        self.packets.push_back(DelayedPacket::Packet(p));
    }
    fn need_tx(&mut self) -> bool {
        match self.peek() {
            DelayedPacket::Packet(_) => true,
            DelayedPacket::Delay(ttx) => {
                if engine::now() >= *ttx {
                    self.packets.pop_front();
                    true
                } else {
                    false
                }
            }
        }
    }
    fn dequeue_packet(&mut self) -> Box<packet::Packet> {
        match self.packets.pop_front() {
            Some(DelayedPacket::Packet(p)) => p,
            Some(DelayedPacket::Delay(_)) => panic!("Expected packet."),
            None => panic!("Queue underflow.")
        }
    }
}
impl Drop for DelayQueue {
    fn drop(&mut self) {
        while !self.empty() {
            match self.peek() {
                DelayedPacket::Packet(_) => packet::free(self.dequeue_packet()),
                DelayedPacket::Delay(_) => { self.packets.pop_front(); () }
            }
        }
    }
}


// RateLimiter app: limit throughput to bitrate

// uses http://en.wikipedia.org/wiki/Token_bucket algorithm
// single bucket, drop non-conformant packets
#[derive(Clone,Debug)]
pub struct RateLimiter {
    pub rate: u64 // bits per second (bps)
}
impl engine::AppConfig for RateLimiter {
    fn new(&self) -> Box<dyn engine::App> {
        // Late limiting with a single token bucket is not an excact science
        // (imagine bursty traffic, and limitations of time-keeping in context
        // of the implementation)
        //
        // We do two things here to behave reasonable:
        //   - avoid IEEE floating point math by scaling our integer values
        //   - operate on discrete ticks of time (100 us per tick)
        //   - choose bucket capacity and initial token values to hopefully
        //     cover our operational range
        //
        // The result should be good enough to shape bandwidths between ~50 Kbps
        // and 10 Gbps within 10% accuracy over a 100 ms time window.
        // Below ~50 Kbps accuracy decreases significantly.
        //
        // `scale' is set to the number of microseconds in a second.
        // NB: if you change this value you have to change how tokens are
        // replenished in push() accordingly.
        //
        // `capacity' is set to the scaled rate over 1 second, and directly
        // affects the permitted burstiness of traffic. I.e., RateLimiter will
        // allow bursts of up to `rate` bits without throttling.
        //
        // `initial_tokens` is choosen to cover bandwidth expected between two
        // ticks. Roughly speaking, if you set this to higher values, the rate
        // limit will take longer to take effect (i.e., larger initial bursts).
        //
        let scale = 1_000_000;
        let tick = 100; // us
        let capacity = self.rate*scale;
        let initial_tokens = self.rate*scale / (1_000_000 / tick);
        Box::new(RateLimiterApp {
            rate: self.rate,
            scale: scale,
            tick: tick,
            bucket: RefCell::new(BitrateBucket {
                capacity: capacity,
                tokens: initial_tokens,
                last_time: None
            })
        })
    }
}
pub struct RateLimiterApp {
    rate: u64,
    scale: u64,
    tick: u64,
    bucket: RefCell<BitrateBucket>
}
struct BitrateBucket {
    capacity: u64,
    tokens: u64,
    last_time: Option<Instant>
}
impl engine::App for RateLimiterApp {
    fn has_push(&self) -> bool { true }
    fn push(&self, app: &engine::AppState) {
        let mut input = app.input.get("input").unwrap().borrow_mut();
        let mut output = app.output.get("output").unwrap().borrow_mut();
        let mut bucket = self.bucket.borrow_mut();

        // Replenish bucket tokens (once every tick at most)
        let now = engine::now();
        if let Some(last_time) = bucket.last_time {
            let us_elapsed = (now - last_time).as_micros() as u64;
            if us_elapsed >= self.tick {
                bucket.last_time = Some(engine::now());
                bucket.tokens = min(
                    bucket.tokens + (self.rate * us_elapsed),
                    bucket.capacity
                );
            }
        } else {
            bucket.last_time = Some(engine::now());
        }

        // Forward packets, consuming bucket tokens
        while !link::empty(&input) {
            let p = link::receive(&mut input);
            let tokens = packet::bitlength(&p) * self.scale;
            if tokens <= bucket.tokens {
                bucket.tokens -= tokens;
                link::transmit(&mut output, p);
            } else {
                // Out of tokens: drop packet
                packet::free(p);
            }
        }
    }
}


#[cfg(test)]
mod selftest {
    use super::*;
    use crate::config;
    use crate::basic_apps;

    #[test]
    fn loss() {
        packet::preallocate(2000);
        let mut c = config::new();
        let loss_rate = 0.1;
        config::app(&mut c, "source", &basic_apps::Source {size: 60});
        config::app(&mut c, "loss", &Loss {ratio: loss_rate});
        config::app(&mut c, "sink", &basic_apps::Sink {});
        config::link(&mut c, "source.output -> loss.input");
        config::link(&mut c, "loss.output -> sink.input");
        engine::configure(&c);
        engine::main(Some(engine::Options {
            duration: Some(Duration::new(0, 10_000_000)), // 0.01s
            report_links: true,
            ..Default::default()
        }));
        let input = engine::state().link_table
            .get("source.output -> loss.input").unwrap();
        let output = engine::state().link_table
            .get("loss.output -> sink.input").unwrap();
        let sent = input.borrow().txpackets as f64;
        let received = output.borrow().rxpackets as f64;
        let loss = 1.0 - received/sent;
        println!("Loss = {:.1}%", loss * 100.0);
        let tolerance = 0.001;
        println!("expected={} lost={:.4} tolerance={}",
                 loss_rate, loss, tolerance);
        assert!((loss - loss_rate).abs() < tolerance);
    }

   #[test]
    fn latency() {
        packet::preallocate(10_000);
        let mut c = config::new();
        let delay = 100; // ms
        let capacity = 3000;
        config::app(&mut c, "source", &basic_apps::Source {size: 60});
        config::app(&mut c, "latency", &Latency {ms: delay, capacity: capacity});
        config::app(&mut c, "sink", &basic_apps::Sink {});
        config::link(&mut c, "source.output -> latency.input");
        config::link(&mut c, "latency.output -> sink.input");
        engine::configure(&c);
        let start = Instant::now();
        let output = engine::state().app_table
            .get("latency").unwrap()
            .output.get("output").unwrap();
        while output.borrow().txpackets == 0 {
            engine::main(Some(engine::Options{
                duration: Some(Duration::from_millis(1)),
                no_report: true,
                ..Default::default()
            }));
        }
        let finish = Instant::now();
        let latency_p1 = finish.duration_since(start).as_millis();
        let tolerance = 2; // 2 ms tolerance
        assert!((delay as i64 - latency_p1 as i64).abs() < tolerance);
        println!("Latency of first packet: {:?} ms", latency_p1);
        // Reset engine state
        engine::configure(&config::new());
        engine::configure(&c);
        let runtime = 250; // 250 ms
        engine::main(Some(engine::Options{
            duration: Some(Duration::from_millis(runtime)),
            report_links: true,
            ..Default::default()
        }));
        let output = engine::state().app_table
            .get("latency").unwrap()
            .output.get("output").unwrap();
        let sent = output.borrow().txpackets;
        let expected = capacity as u64 * (runtime/delay);
        let tolerance = 100;
        println!("expected(approx.)={} sent={}", expected, sent);
        assert!((expected as i64 - sent as i64).abs() < tolerance);
    }

    #[test]
    fn ratelimit() {
        packet::preallocate(2000);
        let mut c = config::new();
        let rate = 1_000_000; // 1 Mbps
        let packet_size = 60;
        let duration_ms = 100;
        config::app(&mut c, "source", &basic_apps::Source {size: packet_size});
        config::app(&mut c, "limit", &RateLimiter {rate: rate});
        config::app(&mut c, "sink", &basic_apps::Sink {});
        config::link(&mut c, "source.output -> limit.input");
        config::link(&mut c, "limit.output -> sink.input");
        engine::configure(&c);
        engine::main(Some(engine::Options{
            duration: Some(Duration::from_millis(duration_ms)),
            report_links: true,
            ..Default::default()
        }));
        let output = engine::state().app_table
            .get("limit").unwrap()
            .output.get("output").unwrap();
        let sent = output.borrow().txpackets;
        let mut p = packet::allocate();
        p.length = packet_size;
        let bits = sent * packet::bitlength(&p);
        packet::free(p);
        println!("Rate: {:.2}/{:.2} Mbps",
                 bits as f64 * (1000.0 / duration_ms as f64) / 1_000_000.0,
                 rate as f64 / 1_000_000.0);
        let expected = rate as f64 * (duration_ms as f64/ 1000.0);
        let tolerance = rate as f64 * (duration_ms as f64 / 1000.0) * 0.02; // 2%
        println!("expected={:.0} received={} tolerance={:.0}",
                 expected, bits, tolerance);
        assert!((expected - bits as f64).abs() < tolerance);
    }

    #[test]
    fn jitter() {
        // This is really just a basic “don’t crash” test
        let mut c = config::new();
        config::app(&mut c, "source", &basic_apps::Source {size: 60});
        config::app(&mut c, "jitter", &Jitter {
            ms: 10,
            strength: 0.1,
            reorder: true,
            capacity: 10_000
        });
        config::app(&mut c, "sink", &basic_apps::Sink {});
        config::link(&mut c, "source.output -> jitter.input");
        config::link(&mut c, "jitter.output -> sink.input");
        engine::configure(&c);
        engine::main(Some(engine::Options {
            done: Some(Box::new(|| true)), // single breath
            report_links: true,
            ..Default::default()
        }));
        // Stop sending new packets
        config::app(&mut c, "source", &basic_apps::Sink {});
        engine::configure(&c);
        engine::main(Some(engine::Options {
            duration: Some(Duration::from_millis(20)),
            report_links: true,
            ..Default::default()
        }));
        let input = engine::state().link_table
            .get("source.output -> jitter.input").unwrap();
        let output = engine::state().link_table
            .get("jitter.output -> sink.input").unwrap();
        let sent = input.borrow().txpackets as f64;
        let received = output.borrow().rxpackets as f64;
        assert!(sent == received);
    }
}

