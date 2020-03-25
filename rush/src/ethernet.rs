use super::lib;
use super::header;

use std::mem;

// ETHERNET
//
// This module contains an Ethernet header definition, a type for Ethernet
// (MAC) addresses, and some related utilities.
//
//   MacAddress - six bytes
//   ntop(&MacAddress) -> String - return string representation of MAC address
//   pton(&str) -> MacAddress - parse MAC address from string representation
//   Ethernet - struct for Ethernet headers
//   Header<Ethernet>.dst() -> &MacAddress - get destination address
//   Header<Ethernet>.set_dst(&MacAddress) - set destination address
//   Header<Ethernet>.src() -> &MacAddress - get source address
//   Header<Ethernet>.set_src(&MacAddress) - set source address
//   Header<Ethernet>.ethertype() -> u16 - get ethertype
//   Header<Ethernet>.set_ethertype(u16) - set ethertype
//   Header<Ethernet>.swap() - swap source and destination addresses
//   TYPE_IPV4 - const u16 identifier for ethertype IPv4

pub type MacAddress = [u8; 6];

pub fn ntop(address: &MacAddress) -> String {
    format!("{:x}:{:x}:{:x}:{:x}:{:x}:{:x}",
            address[0], address[1], address[2],
            address[3], address[4], address[5])
}

pub fn pton(string: &str) -> MacAddress {
    let mut address: MacAddress = [0; 6];
    address[0] = u8::from_str_radix(&string[0..2], 16).unwrap();
    address[1] = u8::from_str_radix(&string[3..5], 16).unwrap();
    address[2] = u8::from_str_radix(&string[6..8], 16).unwrap();
    address[3] = u8::from_str_radix(&string[9..11], 16).unwrap();
    address[4] = u8::from_str_radix(&string[12..14], 16).unwrap();
    address[5] = u8::from_str_radix(&string[15..17], 16).unwrap();
    address
} 

#[repr(C, packed)]
#[derive(Default)]
pub struct Ethernet {
    dst: MacAddress,
    src: MacAddress,
    ethertype: u16
}

impl header::Header<Ethernet> {

    pub fn dst(&self) -> &MacAddress {
        &self.header_ref().dst
    }

    pub fn set_dst(&mut self, address: &MacAddress) {
        let h = self.header_mut();
        lib::copy(&mut h.dst, address, mem::size_of::<MacAddress>());
    }

    pub fn src(&self) -> &MacAddress {
        &self.header_ref().src
    }

    pub fn set_src(&mut self, address: &MacAddress) {
        let h = self.header_mut();
        lib::copy(&mut h.src, address, mem::size_of::<MacAddress>());
    }

    pub fn ethertype(&self) -> u16 {
        lib::ntohs(self.header_ref().ethertype)
    }

    pub fn set_ethertype(&mut self, ethertype: u16) {
        self.header_mut().ethertype = lib::htons(ethertype);
    }

    pub fn swap(&mut self) {
        let h = self.header_mut();
        let mut tmp: MacAddress = [0; 6];
        lib::copy(&mut tmp, &h.dst, 6);
        lib::copy(&mut h.dst, &h.src, 6);
        lib::copy(&mut h.src, &tmp, 6);
    }

}

pub const TYPE_IPV4: u16 = 0x0800;

#[cfg(test)]
mod selftest {
    use super::*;

    #[test]
    fn ethernet() {
        let mut eth = header::new::<Ethernet>();
        eth.set_src(&pton("42:42:42:42:42:42"));
        eth.set_ethertype(TYPE_IPV4);
        let mut mem: [u8; 20] = [1; 20];
        let mut eth2 = header::from_mem::<Ethernet>(&mut mem);
        eth2.set_dst(&pton("01:02:03:04:05:06"));
        eth2.set_ethertype(TYPE_IPV4);
        eth.set_dst(eth2.dst());
        eth.swap();
        println!("eth  dst = to {} from {} ({:x})",
                 ntop(eth.dst()),
                 ntop(eth.src()),
                 eth.ethertype());
        println!("eth2 dst = to {} from {} ({:x})",
                 ntop(eth2.dst()),
                 ntop(eth2.src()),
                 eth2.ethertype());
        println!("size_of::<Ethernet> {}", header::size_of::<Ethernet>());
    }

}
