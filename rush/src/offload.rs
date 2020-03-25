use super::packet;
use super::link;
use super::engine;
use super::lib;
use super::header as hdr;
use super::ethernet;
use super::ethernet::Ethernet;
use super::ipv4;
use super::ipv4::IPv4;
use super::tcp::TCP;
use super::udp::UDP;

use std::cmp;

// Checksum app: offload checksum computation
//
// Receives packets on the input link and forwards them to output link.
//
// Opportunistically fills in missing TCP and UDP checksums for incoming
// packets with checksum set to the ones’ complement of IP pseudo header
// checksum—which is Linux’ canonical way of signaling that the checksum
// computation is to be offloaded.
//
// NYI: IPv4 Options, IPv6 (non-matching packets are forwarded as-is)

#[derive(Clone,Debug)]
pub struct Checksum {}
impl engine::AppConfig for Checksum {
    fn new(&self) -> Box<dyn engine::App> {
        Box::new(ChecksumApp {})
    }
}
pub struct ChecksumApp {}
impl engine::App for ChecksumApp {
    fn has_push(&self) -> bool { true }
    fn push(&self, app: &engine::AppState) {
        let mut input = app.input.get("input").unwrap().borrow_mut();
        let mut output = app.output.get("output").unwrap().borrow_mut();
        while !link::empty(&input) {
            let mut p = link::receive(&mut input);
            maybe_fill_in_checksum(&mut p);
            // Forward
            link::transmit(&mut output, p);
        }
    }
}

fn maybe_fill_in_checksum(p: &mut packet::Packet) {
    let eth = hdr::from_mem::<Ethernet>(&mut p.data);
    if eth.ethertype() == ethernet::TYPE_IPV4 {
        // It’s is an IPv4 packet!
        let ip_ofs = hdr::size_of::<Ethernet>();
        let ip = hdr::from_mem::<IPv4>(&mut p.data[ip_ofs..]);
        if ip.ihl() > 5 { return } // NYI: IP Options

        let proto_ofs = hdr::size_of::<Ethernet>() + hdr::size_of::<IPv4>();
        let proto_length = p.length - proto_ofs as u16;

        if ip.protocol() == ipv4::PROTOCOL_TCP {
            // It’s is a TCP packet!
            let mut tcp = hdr::from_mem::<TCP>(&mut p.data[proto_ofs..]);
            // For offloaded TCP checksums, Linux leaves the checksum value set
            // to the seed value (ones’ complement of IP pseudo header
            // checksum) going into the TCP checksum calculation.
            let pseudo_csum = ip.pseudo_checksum(
                ipv4::PROTOCOL_TCP, proto_length
            );
            // Checksum omitted?
            if lib::ntohs(tcp.checksum()) == !pseudo_csum {
                // Compute and fill in TCP checksum
                let payload_ofs = proto_ofs + hdr::size_of::<TCP>();
                let payload_length = p.length - payload_ofs as u16;
                tcp.checksum_compute(
                    &p.data[payload_ofs..], payload_length, !pseudo_csum
                );
            }

        } else if ip.protocol() == ipv4::PROTOCOL_UDP {
            // It’s is a UDP packet!
            let mut udp = hdr::from_mem::<UDP>(&mut p.data[proto_ofs..]);
            // (Same-same as for TCP...)
            let pseudo_csum = ip.pseudo_checksum(
                ipv4::PROTOCOL_UDP, proto_length
            );
            // Checksum omitted?
            if lib::ntohs(udp.checksum()) == !pseudo_csum {
                // Compute and fill in UDP checksum
                let payload_ofs = proto_ofs + hdr::size_of::<UDP>();
                let payload_length = p.length - payload_ofs as u16;
                udp.checksum_compute(
                    &p.data[payload_ofs..], payload_length, !pseudo_csum
                );
            }
        }
    }
}

// TSD app: TCP Segment Deoptimization
//
// Split up TCP segments to fit MSS in order to counteract TSO as commonly
// performed by operating system network stacks.
// (MSS = Maximum segment size)
// (TSO = TCP segmentation offloading/optimization)
//
// Forwards packets from input to output. Does *not* compute checksums of
// emitted TCP segments but fills in ones’ complement of pseudo header
// checksum instead (see Checksum app above).
//
#[derive(Clone,Debug)]
pub struct TSD {
    pub mss: u16
}
impl engine::AppConfig for TSD {
    fn new(&self) -> Box<dyn engine::App> {
        assert!(self.mss > 0, "Invalid MSS");
        Box::new(TSDApp {mss: self.mss})
    }
}
pub struct TSDApp {
    mss: u16
}
impl engine::App for TSDApp {
    fn has_push(&self) -> bool { true }
    fn push(&self, app: &engine::AppState) {
        let mut input = app.input.get("input").unwrap().borrow_mut();
        let mut output = app.output.get("output").unwrap().borrow_mut();
        while !link::empty(&input) {
            forward_tcp_segments(
                &mut output, link::receive(&mut input), self.mss
            );
        }
    }
}

fn forward_tcp_segments
  (output: &mut link::Link, mut p: Box<packet::Packet>, mss: u16) {
    // Try to split up the packet into TCP segments and forward those, or give
    // up and forward the packet as-is if it is not a segmentable TCP packet
    let eth = hdr::from_mem::<Ethernet>(&mut p.data);
    if eth.ethertype() != ethernet::TYPE_IPV4 { // NYI: IPv6
        link::transmit(output, p);
        return
    }

    let ip_ofs = hdr::size_of::<Ethernet>();
    let mut ip = hdr::from_mem::<IPv4>(&mut p.data[ip_ofs..]);
    if ip.ihl() > 5 { // NYI: IP Options
        link::transmit(output, p);
        return
    }
    if ip.protocol() != ipv4::PROTOCOL_TCP { // Not TCP
        link::transmit(output, p);
        return
    }

    let tcp_ofs = hdr::size_of::<Ethernet>() + hdr::size_of::<IPv4>();
    let mut tcp = hdr::from_mem::<TCP>(&mut p.data[tcp_ofs..]);

    let payload_ofs = cmp::min(tcp_ofs + tcp.size(), p.length as usize);
    let payload_length = p.length as usize - payload_ofs;

    if payload_length <= mss as usize { // Packet fits MSS, forward as is
        link::transmit(output, p);
        return
    }

    // Segment packet, forward segments
    let mut data_ofs = payload_ofs;
    let mut data_length = payload_length;
    while data_length > 0 {
        let mut s = packet::allocate();
        let slen = cmp::min(mss as usize, data_length);
        s.length = (payload_ofs + slen) as u16;
        ip.set_total_length(s.length - ip_ofs as u16);
        ip.checksum_compute();
        let pseudo_csum = ip.pseudo_checksum(
            ipv4::PROTOCOL_TCP, s.length - tcp_ofs as u16
        );
        tcp.set_checksum(lib::htons(!pseudo_csum));
        lib::copy(&mut s.data, &p.data[..payload_ofs], payload_ofs);
        lib::copy(&mut s.data[payload_ofs..], &p.data[data_ofs..], slen);
        link::transmit(output, s);
        data_ofs += slen as usize;
        data_length -= slen;
        tcp.set_seq(tcp.seq() + slen as u32);
    }
    packet::free(p);
}

