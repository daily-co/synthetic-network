use super::packet;
use super::link;
use super::engine;
use super::header as hdr;
use super::ethernet;
use super::ethernet::Ethernet;
use super::ipv4;
use super::ipv4::IPv4;
use super::tcp::TCP;
use super::udp::UDP;

use std::ffi;
use std::mem;


// Split app: match incoming packets against flows and forward them to
// associated outputs; packets not mathcing any flow are forwarded on the
// "default" output
//
// NYI: IPv6, prefixes, protocols that use ports other than TCP/UDP

#[derive(Clone,Debug)]
pub struct Flow {
    pub label: String,     // name of the output link
    pub dir: Dir,          // look at source or destination address/port tuple?
    pub ip: ipv4::Address, // zero is interpreted as “any address”
    pub protocol: u8,      // zero is interpreted as “any protocol”
    pub port_min: u16,     // port range (NB: not all protocols use ports)
    pub port_max: u16
}

#[derive(Clone,Debug,Copy)]
pub enum Dir { Src, Dst }

#[derive(Clone,Debug)]
pub struct Split {
    pub flows: Vec<Flow>
}
impl engine::AppConfig for Split {
    fn new(&self) -> Box<dyn engine::App> {
        Box::new(SplitApp {flows: self.flows.to_vec()})
    }
}
pub struct SplitApp {
    flows: Vec<Flow>
}
impl engine::App for SplitApp {
    fn has_push(&self) -> bool { true }
    fn push(&self, app: &engine::AppState) {
        let mut input = app.input.get("input").unwrap().borrow_mut();
        let default = app.output.get("default").unwrap();
        while !link::empty(&input) {
            let mut p = link::receive(&mut input);
            let mut output = default.borrow_mut();
            for flow in &self.flows {
                if flow_match(&mut p, flow) {
                    output = app.output.get(&flow.label).unwrap().borrow_mut();
                    break
                }
            }
            link::transmit(&mut output, p);
        }
    }
}

fn flow_match(p: &mut packet::Packet, flow: &Flow) -> bool {
    let eth = hdr::from_mem::<Ethernet>(&mut p.data);
    if eth.ethertype() != ethernet::TYPE_IPV4 { return false } // NYI: IPv6

    let ip_ofs = hdr::size_of::<Ethernet>();
    let ip = hdr::from_mem::<IPv4>(&mut p.data[ip_ofs..]);
    if ip.ihl() > 5 { return false } // NYI: IP Options

    let addr = match flow.dir {
        Dir::Src => ip.src(),
        Dir::Dst => ip.dst()
    };
    if flow.ip > 0 && addr != flow.ip { return false }
    if flow.protocol > 0 && ip.protocol() != flow.protocol { return false }

    let proto_ofs = hdr::size_of::<Ethernet>() + hdr::size_of::<IPv4>();

    if flow.protocol == ipv4::PROTOCOL_TCP {
        let tcp = hdr::from_mem::<TCP>(&mut p.data[proto_ofs..]);
        let port = match flow.dir {
            Dir::Src => tcp.src_port(),
            Dir::Dst => tcp.dst_port()
        };
        if port < flow.port_min || port > flow.port_max { return false }

    } else if flow.protocol == ipv4::PROTOCOL_UDP {
        let udp = hdr::from_mem::<UDP>(&mut p.data[proto_ofs..]);
        let port = match flow.dir {
            Dir::Src => udp.src_port(),
            Dir::Dst => udp.dst_port()
        };
        if port < flow.port_min || port > flow.port_max { return false }
    }

    true
}


// Top app: profile flows (packets are forwarded from input to output
// unchanged)
//
// Accepts a path to a file that will be created if it does not already exist,
// and mapped into memory using mmap(2). We suggest to use a path on an
// in-memory filesystem such as /var/run/...
//
// The file’s layout is an array of 2048 (FLOWTOP_NSLOTS) slots. Each slot
// consists of a 64-bit packet counter, a 64-bit bits counter, and a 64-bit
// flow ID. The ID consists of the flow tuple encoded in a little-endian
// 64-bit word like so:
//
//    Bits   | 63..48  39..32    31..0
//    Fields | port    protocol  ipv4addr
//
// For each packet received on the input port, its flow tuple is extracted and
// hashed to select a slot in the array. The slot’s packet counter is incremented
// by one, the bits counter is incremented by the bit length of the packet on the
// wire (i.e., including Ethernet overhead), the and flow ID is set according to
// the packet’s flow tuple. I.e., the slot’s flow ID is set to reflect the
// flow tuple of the last packet counted.
//
// NYI: IPv6, protocols that use ports other than TCP/UDP

#[derive(Clone,Debug)]
pub struct Top {
    pub path: String,
    pub dir: Dir
}
impl engine::AppConfig for Top {
    fn new(&self) -> Box<dyn engine::App> {
        Box::new(TopApp {map: open_flowtop_map(&self.path), dir: self.dir})
    }
}
pub struct TopApp {
    map: *mut FlowTop,
    dir: Dir
}
impl engine::App for TopApp {
    fn has_stop(&self) -> bool { true }
    fn stop(&self) { close_flowtop_map(self.map); }

    fn has_push(&self) -> bool { true }
    fn push(&self, app: &engine::AppState) {
        let mut input = app.input.get("input").unwrap().borrow_mut();
        let mut output = app.output.get("output").unwrap().borrow_mut();
        while !link::empty(&input) {
            let mut p = link::receive(&mut input);
            flow_count(&mut p, self.dir, self.map);
            link::transmit(&mut output, p);
        }
    }
}

fn flow_count(p: &mut Box<packet::Packet>, dir: Dir, map: *mut FlowTop) {
    let mut addr: u32 = 0;
    let mut protocol: u8 = 0;
    let mut port: u16 = 0;

    let eth = hdr::from_mem::<Ethernet>(&mut p.data);
    if eth.ethertype() == ethernet::TYPE_IPV4 { // NYI: IPv6
        
        let ip_ofs = hdr::size_of::<Ethernet>();
        let ip = hdr::from_mem::<IPv4>(&mut p.data[ip_ofs..]);

        addr = match dir {
            Dir::Src => ip.src(),
            Dir::Dst => ip.dst()
        };
        protocol = ip.protocol();

        if ip.ihl() == 5 { // NYI: IP Options
            let proto_ofs = hdr::size_of::<Ethernet>() + hdr::size_of::<IPv4>();

            if ip.protocol() == ipv4::PROTOCOL_TCP {
                let tcp = hdr::from_mem::<TCP>(&mut p.data[proto_ofs..]);
                port = match dir {
                    Dir::Src => tcp.src_port(),
                    Dir::Dst => tcp.dst_port()
                };

            } else if ip.protocol() == ipv4::PROTOCOL_UDP {
                let udp = hdr::from_mem::<UDP>(&mut p.data[proto_ofs..]);
                port = match dir {
                    Dir::Src => udp.src_port(),
                    Dir::Dst => udp.dst_port()
                };
            }
        }
    }

    flowtop_inc(map, addr, protocol, port, packet::bitlength(p));
}

fn open_flowtop_map(path: &str) -> *mut FlowTop {
    unsafe {
        let fd = libc::open(cstr(path).as_ptr(),
                            libc::O_CREAT|libc::O_RDWR, 0o600);
        assert!(fd >= 0, "open");
        let size = mem::size_of::<FlowCtr>() * FLOWTOP_NSLOTS;
        assert!(libc::ftruncate(fd, size as i64) == 0, "ftruncate");
        let ptr = libc::mmap(std::ptr::null_mut(), size,
                             libc::PROT_READ | libc::PROT_WRITE,
                             libc::MAP_SHARED, fd, 0);
        assert!(ptr != libc::MAP_FAILED, "mmap");
        libc::close(fd);
        ptr as *mut FlowTop
    }
}

fn close_flowtop_map(ptr: *mut FlowTop) {
    let size = mem::size_of::<FlowCtr>() * FLOWTOP_NSLOTS;
    unsafe { libc::munmap(ptr as *mut ffi::c_void, size) };
}

fn cstr(s: &str) -> ffi::CString {
    ffi::CString::new(s).expect("cstr failed")
}

const FLOWTOP_NSLOTS: usize = 2048; // MUST be a power of two!
const FLOWTOP_SLOTMASK: usize = FLOWTOP_NSLOTS - 1;

#[repr(C, packed)]
#[derive(Clone,Copy)]
struct FlowCtr {
    packets: u64,
    bits: u64,
    id: u64
}
#[repr(C, packed)]
struct FlowTop {
    slots: [FlowCtr; FLOWTOP_NSLOTS]
}

fn flowtop_inc(map: *mut FlowTop, ip: u32, protocol: u8, port: u16, bits: u64) {
    let id = flow_id(ip, protocol, port);
    let mut slot = unsafe { &mut (*map).slots[flow_slot(id)] };
    slot.id = id;
    slot.packets += 1;
    slot.bits += bits;
}

fn flow_id(ip: u32, protocol: u8, port: u16) -> u64 {
    ((port as u64) << 48) | ((protocol as u64) << 32) | ((ip as u64) << 0)
}

fn flow_slot(flow: u64) -> usize {
    murmurhash64_mix64(flow) as usize & FLOWTOP_SLOTMASK
}

// Non-cryptographic 64-bit hash (Murmur3 fmix64)
// https://github.com/aappleby/smhasher/blob/master/src/MurmurHash3.cpp#L81
fn murmurhash64_mix64(mut k: u64) -> u64 {
    k ^= k >> 33;
    k = k.wrapping_mul(0xff51afd7ed558ccd);
    k ^= k >> 33;
    k = k.wrapping_mul(0xc4ceb9fe1a85ec53);
    k ^= k >> 33;
    k
}


#[cfg(test)]
mod selftest {
    use super::*;
    use crate::lib;
    use crate::config;
    use crate::basic_apps;
    use std::cell::RefCell;
    use std::fs;

    #[test]
    fn split() {
        let packets = vec![
            // TCP 192.168.0.123:200 -> 10.10.0.42:80
            vec![
                /*Dst MAC*/ 0x52, 0x54, 0x00, 0x02, 0x02, 0x02,
                /*Src MAC*/ 0x52, 0x54, 0x00, 0x01, 0x01, 0x01,
                /*Ethertype*/ 0x08, 0x00,
                /*IPv4 version, IHL*/ 0x45, /*TOS*/ 0x00,
                /*Total length*/ 0x00, 0x34, /*ID*/ 0x59, 0x1a,
                /*Flags, frag. offset*/ 0x40, 0x00, /*TTL*/ 0x40,
                /*Protocol*/ 0x06, /*Checksum*/ 0x00, 0x00,
                /*Src addr*/ 192, 168, 0, 123,
                /*Dst addr*/ 10, 10, 0, 42,
                /*Src port*/ 0, 200, /*Dst port*/ 0, 80],

            // TCP 192.168.178.12:123 -> 10.10.0.42:80
            vec![
                /*Dst MAC*/ 0x52, 0x54, 0x00, 0x02, 0x02, 0x02,
                /*Src MAC*/ 0x52, 0x54, 0x00, 0x01, 0x01, 0x01,
                /*Ethertype*/ 0x08, 0x00,
                /*IPv4 version, IHL*/ 0x45, /*TOS*/ 0x00,
                /*Total length*/ 0x00, 0x34, /*ID*/ 0x59, 0x1a,
                /*Flags, frag. offset*/ 0x40, 0x00, /*TTL*/ 0x40,
                /*Protocol*/ 0x06, /*Checksum*/ 0x00, 0x00,
                /*Src addr*/ 192, 168, 178, 12,
                /*Dst addr*/ 10, 10, 0, 42,
                /*Src port*/ 0, 123, /*Dst port*/ 0, 80],

            // IPv6
            vec![
                /*Dst MAC*/ 0x52, 0x54, 0x00, 0x02, 0x02, 0x02,
                /*Src MAC*/ 0x52, 0x54, 0x00, 0x01, 0x01, 0x01,
                /*Ethertype*/ 0x86, 0xdd]
        ];

        engine::configure(&config::new());
        let mut c = config::new();
        config::app(&mut c, "source", &PacketGen {packets: packets});
        config::app(&mut c, "split", &Split {flows: vec![
            Flow {
                label: "src_addr".to_string(),
                dir: Dir::Src,
                ip: ipv4::pton("192.168.0.123"),
                protocol: 0,
                port_min: 0,
                port_max: 0
            },
            Flow {
                label: "dst_tcp80".to_string(),
                dir: Dir::Dst,
                ip: 0,
                protocol: ipv4::PROTOCOL_TCP,
                port_min: 80,
                port_max: 80
            }
        ]});
        config::app(&mut c, "sink", &basic_apps::Sink {});
        config::link(&mut c, "source.output -> split.input");
        config::link(&mut c, "split.src_addr -> sink.src_addr");
        config::link(&mut c, "split.dst_tcp80 -> sink.dst_tcp80");
        config::link(&mut c, "split.default -> sink.default");
        engine::configure(&c);
        engine::main(Some(engine::Options {
            done: Some(Box::new(|| true)), // single breath
            report_links: true,
            ..Default::default()
        }));

        let src_addr_out = engine::state().link_table
            .get("split.src_addr -> sink.src_addr").unwrap();
        assert!(src_addr_out.borrow().txpackets == 1);
        let dst_tcp80_out = engine::state().link_table
            .get("split.dst_tcp80 -> sink.dst_tcp80").unwrap();
        assert!(dst_tcp80_out.borrow().txpackets == 1);
        let default_out = engine::state().link_table
            .get("split.default -> sink.default").unwrap();
        assert!(default_out.borrow().txpackets == 1);
    }

    #[test]
    fn flowtop() {
        let map = open_flowtop_map("flowtop.map");
        for id in 1..=10 {
            println!("hash {}={:x} {:x}", id,
                     murmurhash64_mix64(id as u64),
                     FLOWTOP_SLOTMASK);
            for _ in 1..=100 {
                flowtop_inc(map, id, 0, 0, 42);
            }
        }
        unsafe {
            for slot in &(*map).slots {
                if slot.packets > 0 {
                    println!("flow: {:x}, packets: {}, bits: {}",
                             slot.id, slot.packets, slot.bits);
                }
            }
        }
        // Cleanup
        close_flowtop_map(map);
        let _ = fs::remove_file("flowtop.map");
    }

    #[test]
    fn top() {
        let packets = vec![
            // TCP 192.168.0.123:200 -> 10.10.0.42:80
            vec![
                /*Dst MAC*/ 0x52, 0x54, 0x00, 0x02, 0x02, 0x02,
                /*Src MAC*/ 0x52, 0x54, 0x00, 0x01, 0x01, 0x01,
                /*Ethertype*/ 0x08, 0x00,
                /*IPv4 version, IHL*/ 0x45, /*TOS*/ 0x00,
                /*Total length*/ 0x00, 0x34, /*ID*/ 0x59, 0x1a,
                /*Flags, frag. offset*/ 0x40, 0x00, /*TTL*/ 0x40,
                /*Protocol*/ 0x06, /*Checksum*/ 0x00, 0x00,
                /*Src addr*/ 192, 168, 0, 123,
                /*Dst addr*/ 10, 10, 0, 42,
                /*Src port*/ 0, 200, /*Dst port*/ 0, 80],

            // TCP 192.168.178.12:123 -> 10.10.0.42:80
            vec![
                /*Dst MAC*/ 0x52, 0x54, 0x00, 0x02, 0x02, 0x02,
                /*Src MAC*/ 0x52, 0x54, 0x00, 0x01, 0x01, 0x01,
                /*Ethertype*/ 0x08, 0x00,
                /*IPv4 version, IHL*/ 0x45, /*TOS*/ 0x00,
                /*Total length*/ 0x00, 0x34, /*ID*/ 0x59, 0x1a,
                /*Flags, frag. offset*/ 0x40, 0x00, /*TTL*/ 0x40,
                /*Protocol*/ 0x06, /*Checksum*/ 0x00, 0x00,
                /*Src addr*/ 192, 168, 178, 12,
                /*Dst addr*/ 10, 10, 0, 42,
                /*Src port*/ 0, 123, /*Dst port*/ 0, 80],

            // IPv6
            vec![
                /*Dst MAC*/ 0x52, 0x54, 0x00, 0x02, 0x02, 0x02,
                /*Src MAC*/ 0x52, 0x54, 0x00, 0x01, 0x01, 0x01,
                /*Ethertype*/ 0x86, 0xdd]
        ];

        engine::configure(&config::new());
        let mut c = config::new();
        config::app(&mut c, "source", &PacketGen {packets: packets});
        config::app(&mut c, "top", &Top {
            path: "flowtop.map".to_string(),
            dir: Dir::Src
        });
        config::app(&mut c, "sink", &basic_apps::Sink {});
        config::link(&mut c, "source.output -> top.input");
        config::link(&mut c, "top.output -> sink.input");
        engine::configure(&c);
        engine::main(Some(engine::Options {
            done: Some(Box::new(|| true)), // single breath
            report_links: true,
            ..Default::default()
        }));

        let input = engine::state().link_table
            .get("source.output -> top.input").unwrap();
        let output = engine::state().link_table
            .get("top.output -> sink.input").unwrap();
        assert!(input.borrow().rxpackets == output.borrow().txpackets);

        // Test stop()
        engine::configure(&config::new());

        let map = open_flowtop_map("flowtop.map");
        unsafe {
            for slot in &(*map).slots {
                if slot.packets > 0 {
                    println!("flow: {:x}, packets: {}, bits: {}",
                             slot.id, slot.packets, slot.bits);
                }
            }
        }

        // Cleanup
        let _ = fs::remove_file("flowtop.map");
    }

    #[derive(Clone,Debug)]
    pub struct PacketGen { packets: Vec<Vec<u8>> }
    impl engine::AppConfig for PacketGen {
        fn new(&self) -> Box<dyn engine::App> {
            Box::new(PacketGenApp {
                packets: RefCell::new(self.packets.to_vec())
            })
        }
    }
    pub struct PacketGenApp { packets: RefCell<Vec<Vec<u8>>> }
    impl engine::App for PacketGenApp {
        fn has_pull(&self) -> bool { true }
        fn pull(&self, app: &engine::AppState) {
            if let Some(output) = app.output.get("output") {
                let mut output = output.borrow_mut();
                let mut packets = self.packets.borrow_mut();
                while !link::full(&output) {
                    match packets.pop() {
                        Some(data) => {
                            let mut p = packet::allocate();
                            lib::copy(&mut p.data, &data, data.len());
                            p.length = data.len() as u16;
                            link::transmit(&mut output, p);
                        }
                        None => break
                    }
                }
            }
        }
    }

}
