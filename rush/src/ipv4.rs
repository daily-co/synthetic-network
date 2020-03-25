use super::lib;
use super::header;
use super::checksum;

use std::mem;
use std::slice;
use std::net;
use std::str::FromStr;

// IPv4
//
// This module contains an IPv4 header definition, a type for IPv4 addresses,
// and some related utilities.
//
//   Address - u32 (in network byte order)
//   ntop(Address) -> String - return string representation of IPv4 address
//   pton(&str) -> Address - parse IPv4 address from string representation
//   IPv4 - struct for IPv4 headers
//   IPv4::new() -> Header<IPv4> - new header with defaults (version, IHL, ...)
//   Header<IPv4>.version() -> u16 - get 4-bit version (always 4)
//   Header<IPv4>.set_version(u16) - set 4-bit version (should always be 4)
//   Header<IPv4>.ihl() -> u16 - get 4-bit IHL (5 unless there are options)
//   Header<IPv4>.set_ihl(u16) - set 4-bit IHL (5 unless there are options)
//   Header<IPv4>.total_size() -> u16 - get IPv4 frame size including header
//   Header<IPv4>.set_total_size(u16) - set IPv4 frame size including header
//   Header<IPv4>.id() -> u16 - get flow identifier
//   Header<IPv4>.set_id(u16) - set flow identifier
//   Header<IPv4>.flags() -> u16 - get 3-bit fragment flags
//   Header<IPv4>.set_flags(u16) - set 3-bit fragment flags
//   Header<IPv4>.ttl() -> u8 - get Time-To-Live (max. hops)
//   Header<IPv4>.set_ttl(u8) - set Time-To-Live (max. hops)
//   Header<IPv4>.protocol() -> u8 - get protocol
//   Header<IPv4>.set_protocol(u8) - set protocol
//   Header<IPv4>.checksum() -> u16 - get header checksum
//   Header<IPv4>.set_checksum(u16) - set header checksum
//   Header<IPv4>.checksum_compute() - compute and set header checksum
//   Header<IPv4>.checksum_ok() -> bool - verify header checksum
//   Header<IPv4>.pseudo_checksum(u8,u16) -> u16 - comp. pseudo-header checksum
//   Header<IPv4>.src() -> Address - get source address
//   Header<IPv4>.set_src(Address) - set source address
//   Header<IPv4>.dst() -> Address - get destination address
//   Header<IPv4>.set_dst(Address) - set destination address
//   Header<IPv4>.swap() - swap source and destination addresses
//   PROTOCOL_TCP - const u8 identifier for protocol TCP
//   PROTOCOL_UDP - const u8 identifier for protocol UDP

pub type Address = u32;

pub fn ntop(address: Address) -> String {
    net::Ipv4Addr::from(lib::ntohl(address)).to_string()
}

pub fn pton(string: &str) -> Address {
    lib::htonl(u32::from(net::Ipv4Addr::from_str(string).unwrap()))
} 

#[repr(C, packed)]
#[derive(Default)]
pub struct IPv4 {
    ihl_v_tos: u16, // ihl:4, version:4, tos(dscp:6 + ecn:2)
    total_length: u16,
    id: u16,
    frag_off: u16, // flags:3, fragment_offset:13
    ttl: u8,
    protocol: u8,
    checksum: u16,
    src: Address,
    dst: Address
}
#[repr(C, packed)]
struct PseudoHeader {
    src: u32,
    dst: u32,
    ulp_zero: u8,
    ulp_protocol: u8,
    ulp_len: u16
}

impl IPv4 {
    pub fn new() -> header::Header<IPv4> {
        let mut h = header::new::<IPv4>();
        h.set_version(4);
        h.set_ihl((header::size_of::<IPv4>()/4) as u16);
        h.set_total_length(header::size_of::<IPv4>() as u16);
        h
    }
}

impl header::Header<IPv4> {

    pub fn version(&self) -> u16 {
        (lib::ntohs(self.header_ref().ihl_v_tos) >> 12) & 0xf
    }

    pub fn set_version(&mut self, version: u16) {
        let h = self.header_mut();
        h.ihl_v_tos &= lib::htons(0x0fff);
        h.ihl_v_tos |= lib::htons((version & 0xf) << 12);
    }

    pub fn ihl(&self) -> u16 {
        (lib::ntohs(self.header_ref().ihl_v_tos) >> 8) & 0xf
    }

    pub fn set_ihl(&mut self, ihl: u16) {
        let h = self.header_mut();
        h.ihl_v_tos &= lib::htons(0xf0ff);
        h.ihl_v_tos |= lib::htons((ihl & 0xf) << 8);
    }

    pub fn total_length(&self) -> u16 {
        lib::ntohs(self.header_ref().total_length)
    }

    pub fn set_total_length(&mut self, total_length: u16) {
        self.header_mut().total_length = lib::htons(total_length);
    }

    pub fn id(&self) -> u16 {
        lib::ntohs(self.header_ref().id)
    }

    pub fn set_id(&mut self, id: u16) {
        self.header_mut().id = lib::htons(id);
    }

    pub fn flags(&self) -> u16 {
        (lib::ntohs(self.header_ref().frag_off) >> 13) & 0x7
    }

    pub fn set_flags(&mut self, flags: u16) {
        let h = self.header_mut();
        h.frag_off &= lib::htons(0x1fff);
        h.frag_off |= lib::htons((flags & 0x7) << 13);
    }

    pub fn ttl(&self) -> u8 {
        self.header_ref().ttl
    }

    pub fn set_ttl(&mut self, ttl: u8) {
        self.header_mut().ttl = ttl;
    }

    pub fn protocol(&self) -> u8 {
        self.header_ref().protocol
    }

    pub fn set_protocol(&mut self, protocol: u8) {
        self.header_mut().protocol = protocol;
    }

    pub fn checksum(&self) -> u16 {
        self.header_ref().checksum
    }

    pub fn set_checksum(&mut self, checksum: u16) {
        self.header_mut().checksum = checksum;
    }    

    pub fn src(&self) -> Address {
        self.header_ref().src
    }

    pub fn set_src(&mut self, address: Address) {
        self.header_mut().src = address;
    }

    pub fn dst(&self) -> Address {
        self.header_ref().dst
    }

    pub fn set_dst(&mut self, address: Address) {
        self.header_mut().dst = address;
    }

    pub fn swap(&mut self) {
        let h = self.header_mut();
        let src = h.src;
        h.src = h.dst;
        h.dst = src;
    }

    pub fn checksum_compute(&mut self) {
        self.set_checksum(0);
        self.set_checksum(lib::htons(checksum::ipsum(
            self.header_slice(), header::size_of::<IPv4>(), 0)));
    }

    pub fn checksum_ok(&self) -> bool {
        0 == checksum::ipsum(self.header_slice(), header::size_of::<IPv4>(), 0)
    }

    pub fn pseudo_checksum(&self, protocol: u8, len: u16) -> u16 {
        let ph = PseudoHeader {
            src: self.src(),
            dst: self.dst(),
            ulp_zero: 0,
            ulp_protocol: protocol,
            ulp_len: lib::htons(len)
        };
        let ptr = &ph as *const PseudoHeader as *const u8;
        let size = mem::size_of::<PseudoHeader>();
        let s = unsafe { slice::from_raw_parts(ptr, size) };
        checksum::ipsum(s, size, 0)
    }

}

pub const PROTOCOL_TCP: u8 = 6;
pub const PROTOCOL_UDP: u8 = 17;

#[cfg(test)]
mod selftest {
    use super::*;

    #[test]
    fn ipv4() {
        let mut ip = IPv4::new();
        ip.set_src(pton("127.1.2.3"));
        ip.set_protocol(PROTOCOL_UDP);
        let mut mem: [u8; 20] = [1; 20];
        let mut ip2 = header::from_mem::<IPv4>(&mut mem);
        ip2.set_dst(pton("127.4.5.6"));
        ip2.set_protocol(PROTOCOL_TCP);
        ip.set_dst(ip2.dst());
        ip.swap();
        println!("ip  dst={} src={} ({})",
                 ntop(ip.dst()),
                 ntop(ip.src()),
                 ip.protocol());
        println!("ip2 dst={} src={} ({})",
                 ntop(ip2.dst()),
                 ntop(ip2.src()),
                 ip2.protocol());
        println!("size_of::<IPv4> {}", header::size_of::<IPv4>());
        println!("ihl_v_tos={:04x} version={} ihl={}",
                 lib::ntohs(ip.header_ref().ihl_v_tos),
                 ip.version(),
                 ip.ihl());
        println!("total_length={}", ip.total_length());
        let mut ip = IPv4::new();
        ip.set_total_length(60);
        ip.set_id(23757);
        ip.set_flags(0b010); // Don’t fragment
        ip.set_ttl(64);
        ip.set_protocol(PROTOCOL_TCP);
        ip.set_src(pton("127.0.0.1"));
        ip.set_dst(pton("127.0.0.1"));
        ip.checksum_compute();
        println!("ip={:x?}", ip.header_slice());
        println!("checksum={:x} (ok={})", ip.checksum(), ip.checksum_ok());
        println!("pseudo header (tcp, 40 bytes) checksum={:x}",
                 !ip.pseudo_checksum(PROTOCOL_TCP, 20+20));
    }

}
