use super::lib;
use super::header;
use super::checksum;

use std::cmp;

// TCP
//
// This module contains a TCP header definition.
//
//   TCP - struct for TCP headers
//   Header<TCP>.src_port() -> u16 - get source port
//   Header<TCP>.set_src_port(u16) - set source port
//   Header<TCP>.dst_port() -> u16 - get destination port
//   Header<TCP>.set_dst_port(u16) - set destination port
//   Header<TCP>.checksum() -> u16 - get TCP checksum
//   Header<TCP>.set_checksum(u16) - set TCP checksum
//   Header<TCP>.checksum_compute(&[u8],u16,u16) - compute and set TCP checksum


#[repr(C, packed)]
#[derive(Default)]
pub struct TCP {
    src_port: u16,
    dst_port: u16,
    seq: u32,
    ack: u32,
    off_flags: u16, //data offset:4 reserved:3 NS:1 CWR:1 ECE:1 URG:1 ACK:1 PSH:1 RST:1 SYN:1 FIN:1
    window_size: u16,
    checksum: u16,
    urgent_pointer: u16
}

impl header::Header<TCP> {

    pub fn src_port(&self) -> u16 {
        lib::ntohs(self.header_ref().src_port)
    }

    pub fn set_src_port(&mut self, port: u16) {
        self.header_mut().src_port = lib::htons(port)
    }

    pub fn dst_port(&self) -> u16 {
        lib::ntohs(self.header_ref().dst_port)
    }

    pub fn set_dst_port(&mut self, port: u16) {
        self.header_mut().dst_port = lib::htons(port)
    }

    pub fn seq(&self) -> u32 {
        lib::ntohl(self.header_ref().seq)
    }

    pub fn set_seq(&mut self, seq: u32) {
        self.header_mut().seq = lib::htonl(seq);
    }

    pub fn data_offset(&self) -> u16 {
        (lib::ntohs(self.header_ref().off_flags) >> 12) & 0xf
    }

    pub fn set_data_offset(&mut self, offset: u16) {
        let h = self.header_mut();
        h.off_flags &= lib::htons(0x0fff);
        h.off_flags |= lib::htons((offset & 0xf) << 12);
    }

    pub fn size(&self) -> usize {
        cmp::max(5, self.data_offset() as usize) * 4
    }

    pub fn checksum(&self) -> u16 {
        self.header_ref().checksum
    }

    pub fn set_checksum(&mut self, checksum: u16) {
        self.header_mut().checksum = checksum
    }

    pub fn checksum_compute(&mut self, payload: &[u8], length: u16, init: u16)
    {
        self.set_checksum(0);
        let hsum = checksum::ipsum(
            self.header_slice(), header::size_of::<TCP>(), init
        );
        self.set_checksum(lib::htons(checksum::ipsum(
            payload, length as usize, !hsum
        )));
    }

}

#[cfg(test)]
mod selftest {
    use super::*;
    use crate::ethernet::Ethernet;
    use crate::ipv4::IPv4;

    #[test]
    fn checksum() {
        let ip_base      = header::size_of::<Ethernet>();
        let ip_hdr_size  = header::size_of::<IPv4>();
        let tcp_base     = ip_base + ip_hdr_size;
        let tcp_hdr_size = header::size_of::<TCP>();
        let payload_base = tcp_base + tcp_hdr_size;

        let mut p: [u8; 66] = [
            0x52, 0x54, 0x00, 0x02, 0x02, 0x02, 0x52, 0x54, 0x00, 0x01, 0x01, 0x01, 0x08, 0x00, 0x45, 0x00,
            0x00, 0x34, 0x59, 0x1a, 0x40, 0x00, 0x40, 0x06, 0x00, 0x00, 0xc0, 0xa8, 0x14, 0xa9, 0x6b, 0x15,
            0xf0, 0xb4, 0xde, 0x0b, 0x01, 0xbb, 0xe7, 0xdb, 0x57, 0xbc, 0x91, 0xcd, 0x18, 0x32, 0x80, 0x10,
            0x05, 0x9f, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01, 0x08, 0x0a, 0x06, 0x0c, 0x5c, 0xbd, 0xfa, 0x4a,
            0xe1, 0x65
        ];
        let ip = header::from_mem::<IPv4>(&mut p[ip_base..]);
        let mut tcp = header::from_mem::<TCP>(&mut p[tcp_base..]);
        let payload_length = p.len() - payload_base;
        tcp.checksum_compute(
            &p[payload_base..], payload_length as u16,
            !ip.pseudo_checksum(6, (tcp_hdr_size+payload_length) as u16)
        );
        assert!(tcp.checksum() == lib::htons(0x382a), "Wrong TCP checksum");

        assert!(tcp.data_offset() == 8);
        assert!(tcp.size() == 32);
        tcp.set_data_offset(0); // Invalid
        assert!(tcp.size() == 20);

        assert!(tcp.seq() == 3889911740);
        tcp.set_seq(42);
        assert!(tcp.seq() == 42);
    }

}
