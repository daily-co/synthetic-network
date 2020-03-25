// LINK STRUCT AND OPERATIONS
//
// This module defines a struct to represent unidirectional network links,
// implemented as circular ring buffers, and link operations.
//
//   Link - opaque link structure
//   LINK_MAX_PACKETS - capacity of a Link
//   new() -> Link - allocate a new empty Link
//   full(&Link) -> bool - predicate to test if Link is full
//   empty(&Link) -> bool - predicate to test if Link is empty
//   receive(&mut Link) -> Box<Packet> - dequeue a packet from the Link
//   transmit(&mut Link, Box<Packet>) - enqueue a packet on the Link

use super::packet;

// Size of the ring buffer.
const LINK_RING_SIZE: usize = 1024;

// Capacity of a Link.
pub const LINK_MAX_PACKETS: usize = LINK_RING_SIZE - 1;

pub struct Link {
    // this is a circular ring buffer, as described at:
    //   http://en.wikipedia.org/wiki/Circular_buffer
    packets: [*mut packet::Packet; LINK_RING_SIZE],
    // Two cursors:
    //   read:  the next element to be read
    //   write: the next element to be written
    read: i32, write: i32,
    // Link stats:
    pub txpackets: u64, pub txbytes: u64, pub txdrop: u64,
    pub rxpackets: u64, pub rxbytes: u64
}

const SIZE: i32 = LINK_RING_SIZE as i32; // shorthand

pub fn new() -> Link {
    Link { packets: [std::ptr::null_mut(); LINK_RING_SIZE],
           read: 0, write: 0,
           txpackets: 0, txbytes: 0, txdrop: 0,
           rxpackets: 0, rxbytes: 0 }
}

pub fn empty(r: &Link) -> bool { r.read == r.write }

pub fn full(r: &Link) -> bool { (r.write + 1) & (SIZE - 1) == r.read }

// NB: non-empty assertion commented out in original Snabb, but since we get a
// bunch of nice safety invariants from the Rust compiler, let’s maintain them.
// Box::from_raw will never alias because receive/transmit ensure any Packet is
// either on a single Link, or on no Link at all.
pub fn receive(r: &mut Link) -> Box<packet::Packet> {
    if empty(r) { panic!("Link underflow."); }
    let p = unsafe { Box::from_raw(r.packets[r.read as usize]) };
    r.read = (r.read + 1) & (SIZE - 1);
    r.rxpackets += 1;
    r.rxbytes += p.length as u64;
    p
}

#[inline(always)]
pub fn transmit(r: &mut Link, mut p: Box<packet::Packet>) {
    if full(r) {
        r.txdrop += 1;
        packet::free(p);
    } else {
        r.txpackets += 1;
        r.txbytes += p.length as u64;
        r.packets[r.write as usize] = &mut *p; std::mem::forget(p);
        r.write = (r.write + 1) & (SIZE - 1);
    }
}

// Ensure that Dropped Links are empty (otherwise Dropping a link would leak
// its remaining enqueued packets).
// NB: a non-empty Link going out of scope will trigger a panic.
impl Drop for Link {
    fn drop(&mut self) {
        while !empty(self) { packet::free(receive(self)); }
    }
}

#[cfg(test)]
mod selftest {
    use super::*;

    #[test]
    fn link() {
        let mut r = new();
        println!("Allocated a link of capacity {}", LINK_MAX_PACKETS);
        let to_transmit = 2000;
        if full(&r) { panic!("Link should be empty."); }
        for n in 1..=to_transmit {
            let mut p = packet::allocate();
            p.length = n;
            p.data[(n-1) as usize] = 42;
            // Why is &, &mut not automatically inferred?
            transmit(&mut r, p);
            //p.data[0] = 13 // Would cause compiler error.
            //transmit(&mut r, p); // Would cause compile error
        }
        println!("Transmitted {} packets", to_transmit);
        if empty(&r) || !full(&r) { panic!("Link should be full."); }
        let mut n = 0;
        while !empty(&r) {
            n += 1;
            let p = receive(&mut r);
            if p.length != n as u16 || p.data[n-1] != 42 { panic!("Corrupt packet!"); }
            packet::free(p);
        }
        //receive(&mut r); // Would cause link underflow panic.
        println!("Received {} packets", n);
        println!("link: rxpackets={} rxbytes={} txpackets={} txbytes={} txdrop={}",
                 r.rxpackets, r.rxbytes, r.txpackets, r.txbytes, r.txdrop);
        // Failing to drain the link would cause panic
    }

}
