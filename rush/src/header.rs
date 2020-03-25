use super::lib;

use std::mem;
use std::slice;

// PROTOCOL HEADERS
//
// Synthesizing and parsing network protocol headers.
//
//   Header - generic header box (can point to a head allocated object or
//                                into arbitrary memory)
//   new<T>() -> Header<T> - create a new heap allocated header of type T
//   from_mem<T>(&mut [u8]) -> Header<T> - cast byte slice into a Header<T>
//   size_of<T>() -> usize - return byte size of header of type T
//   Header<T>.copy(&mut [u8]) - copy header into a byte slice
//   Header<T>.header_ref(&self) -> &T - get reference to header
//   Header<T>.header_mut(&mut self) -> &mut T - get mutable reference to header
//   Header<T>.header_slice(&self) -> &[u8] - header as byte slice

pub struct Header<T> {
    pub ptr: *mut T,
    _backing: Option<Box<T>>
}

pub fn new<T: Default>() -> Header<T> {
    let mut h = Box::new(T::default());
    Header { ptr: &mut *h, _backing: Some(h) }
}

pub fn from_mem<T>(ptr: &mut [u8]) -> Header<T> {
    assert!(ptr.len() >= mem::size_of::<T>());
    Header { ptr: ptr as *mut [u8] as *mut T, _backing: None }
}

pub fn size_of<T>() -> usize { mem::size_of::<T>() }

impl<T> Header<T> {

    pub fn copy(&self, dst: &mut [u8]) {
        lib::copy(dst, self.header_slice(), size_of::<T>());
    }

    pub fn header_slice(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.ptr as *const u8, size_of::<T>()) }
    }

    pub fn header_ref(&self) -> &T {
        unsafe { self.ptr.as_ref().unwrap() }
    }

    pub fn header_mut(&mut self) -> &mut T {
        unsafe { self.ptr.as_mut().unwrap() }
    }

}
