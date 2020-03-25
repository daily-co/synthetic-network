use std::cmp;
use std::ptr;
use regex::Regex;
use once_cell::sync::Lazy;
use core::ffi;

pub fn fill(dst: &mut [u8], len: usize, val: u8) {
    unsafe {
        ptr::write_bytes(dst.as_mut_ptr(), val, cmp::min(len, dst.len()));
    }
}

pub fn copy(dst: &mut [u8], src: &[u8], len: usize) {
    unsafe {
        ptr::copy(src.as_ptr(), dst.as_mut_ptr(),
                  cmp::min(len, cmp::min(src.len(), dst.len())));
    }
}

// Increase value to be a multiple of size (if it is not already).
pub fn align(value: usize, size: usize) -> usize {
   if value % size == 0 {
       value
   } else {
       value + size - (value % size)
   }
}

#[cfg(target_endian = "little")] pub fn htonl(l: u32) -> u32 { l.swap_bytes() }
#[cfg(target_endian = "little")] pub fn ntohl(l: u32) -> u32 { l.swap_bytes() }
#[cfg(target_endian = "little")] pub fn htons(s: u16) -> u16 { s.swap_bytes() }
#[cfg(target_endian = "little")] pub fn ntohs(s: u16) -> u16 { s.swap_bytes() }
#[cfg(target_endian = "big"   )] pub fn htonl(l: u32) -> u32 { l }
#[cfg(target_endian = "big"   )] pub fn ntohl(l: u32) -> u32 { l }
#[cfg(target_endian = "big"   )] pub fn htons(s: u16) -> u16 { s }
#[cfg(target_endian = "big"   )] pub fn ntohs(s: u16) -> u16 { s }

pub fn comma_value(n: u64) -> String { // credit http://richard.warburton.it
    let s = format!("{}", n);
    if let Some(cap) = CVLEFTNUM.captures(&s) {
        let (left, num) = (&cap[1], &cap[2]);
        let rev = |s: &str| { s.chars().rev().collect::<String>() };
        let num = rev(&CVTHOUSANDS.replace_all(&rev(&num), "$1,").to_string());
        format!("{}{}", left, num)
    } else { s }
}
static CVLEFTNUM: Lazy<Regex> = Lazy::new
    (|| Regex::new(r"^(\d\d?\d?)(\d{3}*)$").unwrap());
static CVTHOUSANDS: Lazy<Regex> = Lazy::new
    (|| Regex::new(r"(\d{3})").unwrap());

// Fill slice with random bytes.
pub fn random_bytes(dst: &mut [u8], n: usize) {
    let n = cmp::min(n, dst.len());
    if unsafe {
        libc::getrandom(dst.as_mut_ptr() as *mut ffi::c_void, n, 0)
    } != n as isize { panic!("getrandom(2) failed"); }
}
