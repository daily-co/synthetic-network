use super::engine;
use super::packet;
use super::link;
use super::lib;

use std::cell::RefCell;
use std::ffi;
use std::mem;
use std::ptr;

// RAW socket app: interface with Linux network devices

#[derive(Clone,Debug)]
pub struct RawSocket { pub ifname: String }
impl engine::AppConfig for RawSocket {
    fn new(&self) -> Box<dyn engine::App> {
        Box::new(RawSocketApp {
            sock: open_raw_socket(&self.ifname),
            fdset: RefCell::new(FdSet::new())
        })
    }
}
pub struct RawSocketApp {
    sock: i32,
    fdset: RefCell<FdSet>
}
impl engine::App for RawSocketApp {
    fn has_pull(&self) -> bool { true }
    fn pull(&self, app: &engine::AppState) {
        if let Some(output) = app.output.get("output") {
            let mut output = output.borrow_mut();
            let mut limit = engine::PULL_NPACKETS;
            let mut fdset = self.fdset.borrow_mut();
            while limit > 0 && can_receive(self.sock, &mut fdset) {
                limit -= 1;
                link::transmit(&mut output, receive(self.sock));
            }
        }
    }
    fn has_push(&self) -> bool { true }
    fn push(&self, app: &engine::AppState) {
        if let Some(input) = app.input.get("input") {
            let mut input = input.borrow_mut();
            let mut fdset = self.fdset.borrow_mut();
            while !link::empty(&input) && can_transmit(self.sock, &mut fdset) {
                transmit(self.sock, link::receive(&mut input));
            }
        }
    }
    fn has_stop(&self) -> bool { true }
    fn stop(&self) { unsafe { libc::close(self.sock); } }
}

fn open_raw_socket(ifname: &str) -> i32 {
    let index = unsafe { libc::if_nametoindex(cstr(ifname).as_ptr()) };
    assert!(index != 0, "invalid ifname");
    let af_packet = libc::AF_PACKET;
    let sock_rawnoblock = libc::SOCK_RAW | libc::SOCK_NONBLOCK;
    let proto_eth = lib::htons(libc::ETH_P_ALL as u16) as i32;
    let sock = unsafe { libc::socket(af_packet, sock_rawnoblock, proto_eth) };
    assert!(sock != -1, "cannot create socket");
    unsafe {
        let addr = libc::sockaddr_ll {
            sll_family: af_packet as u16,
            sll_ifindex: index as i32,
            sll_protocol: proto_eth as u16,
            // Unset / zero
            sll_addr: [0; 8],
            sll_hatype: 0,
            sll_halen: 0,
            sll_pkttype: 0
        };
        let sa = &addr as *const libc::sockaddr_ll as *const libc::sockaddr;
        let addrlen = mem::size_of::<libc::sockaddr_ll>() as u32;
        if libc::bind(sock, sa, addrlen) == -1 {
            libc::close(sock);
            panic!("cannot bind to interface");
        }
    }
    sock
}

fn can_receive (sock: i32, fdset: &mut FdSet) -> bool {
    let fdmax = sock + 1;
    let readfds = fdset.as_mut_ptr();
    let writefds = ptr::null_mut();
    let exceptfds = ptr::null_mut();
    let timeout = &mut libc::timeval { tv_sec: 0, tv_usec: 0 };
    let mut ret = -1;
    let mut err = libc::EAGAIN;
    while ret == -1 && (err == libc::EAGAIN || err == libc::EINTR) {
        fdset.set(sock);
        ret = unsafe {
            libc::select(fdmax, readfds, writefds, exceptfds, timeout)
        };
        err = errno();
    }
    assert!(ret != -1, "cannot select(2) on raw socket");
    ret == 1
}

fn receive (sock: i32) -> Box<packet::Packet> {
    let mut p = packet::allocate();
    let read = unsafe {
        libc::read(sock, cptr(&mut p.data), packet::PAYLOAD_SIZE)
    };
    assert!(read > 0, "cannot read(2) packet");
    p.length = read as u16;
    p
}

fn can_transmit (sock: i32, fdset: &mut FdSet) -> bool {
    let fdmax = sock + 1;
    let readfds = ptr::null_mut();
    let writefds = fdset.as_mut_ptr();
    let exceptfds = ptr::null_mut();
    let timeout = &mut libc::timeval { tv_sec: 0, tv_usec: 0 };
    let mut ret = -1;
    let mut err = libc::EAGAIN;
    while ret == -1 && (err == libc::EAGAIN || err == libc::EINTR) {
        fdset.set(sock);
        ret = unsafe {
            libc::select(fdmax, readfds, writefds, exceptfds, timeout)
        };
        err = errno();
    }
    assert!(ret != -1, "cannot select(2) on raw socket");
    ret == 1
}

fn transmit (sock: i32, mut p: Box<packet::Packet>) {
    let written = unsafe {
        libc::write(sock, cptr(&mut p.data), p.length as usize)
    };
    assert!(written == p.length as isize, "cannot write(2) packet");
    packet::free(p);
}

fn cstr(s: &str) -> ffi::CString {
    ffi::CString::new(s).expect("cstr failed")
}

fn cptr<T>(ptr: &mut T) -> *mut ffi::c_void {
    ptr as *mut T as *mut ffi::c_void
}

fn errno() -> i32 {
    unsafe { *libc::__errno_location() }
}

struct FdSet(libc::fd_set);
impl FdSet {
    fn new() -> FdSet {
        unsafe {
            let mut raw_fd_set = mem::MaybeUninit::<libc::fd_set>::uninit();
            libc::FD_ZERO(raw_fd_set.as_mut_ptr());
            FdSet(raw_fd_set.assume_init())
        }
    }
    fn set(&mut self, fd: i32) {
        unsafe { libc::FD_SET(fd, &mut self.0) }
    }
    fn as_mut_ptr (&mut self) -> *mut libc::fd_set {
        &mut self.0
    }
        
}


#[cfg(test)]
mod selftest {
    use super::*;
    use crate::config;
    use crate::basic_apps;

    use std::time::Duration;

    #[test]
    fn rawsocket_sink() {
        if unsafe { libc::getuid() } != 0 {
            println!("Skipping test (need to be root)");
            return
        }
        let mut c = config::new();
        config::app(&mut c, "rawsocket", &RawSocket {
            ifname: "lo".to_string()
        });
        config::app(&mut c, "sink", &basic_apps::Sink {});
        config::link(&mut c, "rawsocket.output -> sink.input");
        engine::configure(&c);
        engine::main(Some(engine::Options {
            duration: Some(Duration::new(1, 0)), // 1 second
            report_links: true,
            ..Default::default()
        }));
    }
}
