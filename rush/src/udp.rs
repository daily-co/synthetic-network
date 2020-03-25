use super::lib;
use super::header;
use super::checksum;

// UDP
//
// This module contains a UDP header definition.
//
//   UDP - struct for UDP headers
//   Header<UDP>.src_port() -> u16 - get source port
//   Header<UDP>.set_src_port(u16) - set source port
//   Header<UDP>.dst_port() -> u16 - get destination port
//   Header<UDP>.set_dst_port(u16) - set destination port
//   Header<UDP>.len() -> u16 - get datagram length
//   Header<UDP>.set_len(u16) - set datagram length
//   Header<UDP>.checksum() -> u16 - get checksum
//   Header<UDP>.set_checksum(u16) - set checksum
//   Header<UDP>.checksum_compute(&[u8],u16,u16) - compute and set UDP checksum


#[repr(C, packed)]
#[derive(Default)]
pub struct UDP {
    src_port: u16,
    dst_port: u16,
    len: u16,
    checksum: u16
}

impl header::Header<UDP> {

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

    pub fn len(&self) -> u16 {
        lib::ntohs(self.header_ref().len)
    }

    pub fn set_len(&mut self, len: u16) {
        self.header_mut().len = lib::htons(len)
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
            self.header_slice(), header::size_of::<UDP>(), init
        );
        self.set_checksum(lib::htons(checksum::ipsum(
            payload, length as usize, !hsum
        )));
    }

}
