use super::engine;
use super::memory;
use super::lib;

use std::cmp;
use std::mem;

// PACKET STRUCT AND FREELIST
//
// This module defines a struct to represent packets of network data, and
// implements a global freelist from which packets can be allocated.
//
//   Packet - packet structure with length and data fields
//   PAYLOAD_SIZE - size of packet’s data field
//   preallocate(usize) - preallocate a minimum amount of packets
//   allocate() -> Box<Packet> - take a packet off the freelist for use
//   free(Box<Packet>) - return a packet to the freelist
//   clone(Box<Packet>) -> Box<Packet> - return a copy of packet
//   bitlength(Box<Packet>) -> usize - return bit length of packet on-the-wire

// The maximum amount of payload in any given packet.
// NB: for synthetic_network we cranked this way up to fit the maximum
// packet size supported by the Linux kernel, in order to handle reassembled
// TCP segments produced via TSO.
pub const PAYLOAD_SIZE: usize = 65535;

// Packet of network data, with associated metadata.
// XXX - should be #[repr(C, packed)], however that would require unsafe{} to
// access members. Is the memory layout in repr(rust) equivalent?
pub struct Packet {
    pub length: u16, // data payload length
    pub data: [u8; PAYLOAD_SIZE]
}

// A packet may never go out of scope. It is either on the freelist, a link, or
// in active use (in-scope).
// XXX - Could free() packets automatically in Drop, and obsolete manual free.
impl Drop for Packet { fn drop(&mut self) { panic!("Packet leaked"); } }

// Allocate a packet struct on the heap (initialized all-zero).
// NB: Box is how we heap-allocate in Rust.
fn new_packet() -> Box<Packet> {
    let base = memory::dma_alloc(mem::size_of::<Packet>(),
                                 mem::align_of::<Packet>());
    let mut p = unsafe { Box::from_raw(base as *mut Packet) };
    p.length = 0;
    p
}
fn new_packet_noroot() -> Box<Packet> {
    Box::new(Packet { length: 0, data: [0; PAYLOAD_SIZE] })
}

// Maximum number of packets on the freelist.
const MAX_PACKETS: usize = 1_000_000;

// Freelist consists of an array of mutable raw pointers to Packet,
// and a fill counter.
struct Freelist {
    list: [*mut Packet; MAX_PACKETS],
    nfree: usize
}

// FL: global freelist (initially empty, populated with null ptrs).
static mut FL: Freelist = Freelist {
    list: [std::ptr::null_mut(); MAX_PACKETS],
    nfree: 0
};

// Preallocate at least n packets.
pub fn preallocate(n: usize) {
    while unsafe { PACKETS_ALLOCATED } < n {
        preallocate_step();
    }
}

// Fill up FL with freshly allocated packets.
// NB: using FL is unsafe because it is a mutable static (we have to ensure
// thread safety).
// NB: use DMA allocator if run as root, regular heap allocator otherwise.
static mut PACKETS_ALLOCATED: usize = 0;
static mut PACKET_ALLOCATION_STEP: usize = 1000;
fn preallocate_step () {
    let new_packet = match 1 /* unsafe { libc::getuid() } */ {
        0 => new_packet,
        _ => new_packet_noroot
    };
    unsafe {
        assert!(PACKETS_ALLOCATED + PACKET_ALLOCATION_STEP <= MAX_PACKETS,
                "Packet allocation overflow");

        for _ in 0..PACKET_ALLOCATION_STEP {
            free_internal(new_packet());
        }
        PACKETS_ALLOCATED += PACKET_ALLOCATION_STEP;
        PACKET_ALLOCATION_STEP *= 2;
    }
}

// Allocate an empty Boxed Packet from FL.
// NB: we can use Box::from_raw safely on the packets "leaked" onto
// the static FL. We can also be sure that the Box does not alias another
// packet (see free).
#[inline(always)]
pub fn allocate() -> Box<Packet> {
    if unsafe { FL.nfree == 0 } {
        preallocate_step();
    }
    unsafe { FL.nfree -= 1; }
    unsafe { Box::from_raw(FL.list[FL.nfree]) }
}

// Return Boxed Packet to FL.
// NB: because p is mutable and Box does not implement the Copy trait free
// effectively consumes the Box. Once a packet is freed it can no longer be
// referenced, and hence can not me mutated once it has been returned to the
// freelist.
// NB: we can cast a mutable reference of the boxed packet (&mut *p) to a raw
// pointer.
// NB: we std::mem::forget the Box p to inhibit Dropping of the packet once it
// is on the freelist. (I.e., we intentionally leak up to MAX_PACKETS packets
// onto the static FL.) If a packet goes out of scope without being freed, the
// attempt to Drop it will trigger a panic (see Packet). Hence we ensure that
// all allocated packets are eventually freed.
fn free_internal(mut p: Box<Packet>) {
    if unsafe { FL.nfree } == MAX_PACKETS { panic!("Packet freelist overflow"); }
    p.length = 0;
    unsafe { FL.list[FL.nfree] = &mut *p; } mem::forget(p);
    unsafe { FL.nfree += 1; }
}
pub fn free (p: Box<Packet>) {
    engine::add_frees();
    engine::add_freebytes(p.length as u64);
    engine::add_freebits(bitlength(&p));
    free_internal(p);
}

// Clone a packet
pub fn clone (p: &Box<Packet>) -> Box<Packet> {
    let mut copy = allocate();
    lib::copy(&mut copy.data, &p.data, p.length as usize);
    copy.length = p.length;
    copy
}

pub fn bitlength(p: &Box<Packet>) -> u64 {
    // Calculate bits of physical capacity required for packet on 10GbE
    // Account for minimum data size and overhead of Ethernet preamble, CRC,
    // and inter-packet gap
    // https://netoptimizer.blogspot.com/2014/05/the-calculations-10gbits-wirespeed.html
    (12 + 8 + cmp::max(p.length as u64, 60) + 4) * 8
}

// pub fn debug() {
//    unsafe {
//        println!("FL.nfree: {}", FL.nfree);
//        println!("FL.list[FL.nfree].data[0]: {}",
//                 FL.list[FL.nfree-1].as_mut().unwrap().data[0]);
//    }
// }

#[cfg(test)]
mod selftest {
    use super::*;

    #[test]
    fn alloc() {
        let mut p = allocate();
        println!("Allocated a packet of length {}", p.length);
        p.length = 1;
        p.data[0] = 42;
        //p.data[100000] = 99; // Would cause compile error
        println!("Mutating packet (length = {}, data[0] = {})",
                 p.length, p.data[0]);
        let len = p.length;
        free(p); // Not freeing would cause panic
        println!("Freed a packet of length {}", len);
        //p.length = 2; // Would cause compile error
    }

}
