use super::lib;

// IP CHECKSUM
//
// The checksum module provides an optimized ones-complement checksum
// routine.
//
//  ipsum(data: &[u8], length: usize, initial: u16) -> checksum: u16
//    return the ones-complement checksum for the given region of memory

// Reference implementation in Rust.
fn checksum_rust(data: &[u8], length: usize) -> u16 {
    let ptr: *const u8 = data.as_ptr();
    let mut csum: u64 = 0;
    let mut i = length;
    while i > 1 {
        let word = unsafe { *(ptr.offset((length-i) as isize) as *const u16) };
        csum += word as u64;
        i -= 2;
    }
    if i == 1 {
        csum += data[length-1] as u64;
    }
    loop {
        let carry = csum >> 16;
        if carry == 0 { break; }
        csum = (csum & 0xffff) + carry;
    }
    lib::ntohs(!csum as u16 & 0xffff)
}

// ipsum: return the ones-complement checksum for the given region of memory
//
// data is a byte slice to be checksummed.
// initial is an unsigned 16-bit number in host byte order which is used as
// the starting value of the accumulator. 
// The result is the IP checksum over the data in host byte order.
// 
// The 'initial' argument can be used to verify a checksum or to calculate the
// checksum in an incremental manner over chunks of memory. The synopsis to
// check whether the checksum over a block of data is equal to a given value is
// the following
//
//   if ipsum(data, len, value) == 0 {
//       checksum correct
//   } else {
//       checksum incorrect
//   }
//
// To chain the calculation of checksums over multiple blocks of data together
// to obtain the overall checksum, one needs to pass the one's complement of
// the checksum of one block as initial value to the call of ipsum() for the
// following block, e.g.
//
//   let sum1 = ipsum(data1, length1, 0);
//   let total_sum = ipsum(data2, length2, !sum1);
//
pub fn ipsum(data: &[u8], length: usize, initial: u16) -> u16 {
    unsafe { checksum(data, length, initial) }
}

#[cfg(target_arch="x86_64")]
unsafe fn checksum(data: &[u8], length: usize, initial: u16) -> u16 {
    let ptr = data.as_ptr();
    let size = length;
    let mut acc = initial as u64;
    core::arch::asm!("
# Accumulative sum.
xchg {acc:l}, {acc:h}          # Swap to convert to host-bytes order.
1:
cmp {size}, 32                 # If index is less than 32.
jl 2 f                         # Jump to branch '2'.
add {acc}, [{ptr}]             # Sum acc with qword[0].
adc {acc}, [{ptr} + 8]         # Sum with carry qword[1].
adc {acc}, [{ptr} + 16]        # Sum with carry qword[2].
adc {acc}, [{ptr} + 24]        # Sum with carry qword[3]
adc {acc}, 0                   # Sum carry-bit into acc.
sub {size}, 32                 # Decrease index by 8.
add {ptr}, 32                  # Jump two qwords.
jmp 1 b                        # Go to beginning of loop.
2:
cmp {size}, 16                 # If index is less than 16.
jl 3 f                         # Jump to branch '3'.
add {acc}, [{ptr}]             # Sum acc with qword[0].
adc {acc}, [{ptr} + 8]         # Sum with carry qword[1].
adc {acc}, 0                   # Sum carry-bit into acc.
sub {size}, 16                 # Decrease index by 8.
add {ptr}, 16                  # Jump two qwords.
3:
cmp {size}, 8                  # If index is less than 8.
jl 4 f                         # Jump to branch '4'.
add {acc}, [{ptr}]             # Sum acc with qword[0].
adc {acc}, 0                   # Sum carry-bit into acc.
sub {size}, 8                  # Decrease index by 8.
add {ptr}, 8                   # Next 64-bit.
4:
cmp {size}, 4                  # If index is less than 4.
jl 5 f                         # Jump to branch '5'.
mov {tmp:e}, dword ptr [{ptr}] # Fetch 32-bit into tmp.
add {acc}, {tmp}               # Sum acc with tmp. Accumulate carry.
adc {acc}, 0                   # Sum carry-bit into acc.
sub {size}, 4                  # Decrease index by 4.
add {ptr}, 4                   # Next 32-bit.
5:
cmp {size}, 2                  # If index is less than 2.
jl 6 f                         # Jump to branch '6'.
movzx {tmp}, word ptr [{ptr}]  # Fetch 16-bit into tmp.
add {acc}, {tmp}               # Sum acc with tmp. Accumulate carry.
adc {acc}, 0                   # Sum carry-bit into acc.
sub {size}, 2                  # Decrease index by 2.
add {ptr}, 2                   # Next 16-bit.
6:
cmp {size}, 1                  # If index is less than 1.
jl 7 f                         # Jump to branch '7'.
movzx {tmp}, byte ptr [{ptr}]  # Fetch 8-bit into tmp.
add {acc}, {tmp}               # Sum acc with tmp. Accumulate carry.
adc {acc}, 0                   # Sum carry-bit into acc.
# Fold 64-bit into 16-bit.
7:
mov {tmp}, {acc}               # Assign acc to tmp.
shr {tmp}, 32                  # Shift tmp 32-bit. Stores higher part of acc.
mov {acc:e}, {acc:e}           # Clear out higher-part of acc. Stores lower part of acc.
add {acc:e}, {tmp:e}           # 32-bit sum of acc and tmp.
adc {acc:e}, 0                 # Sum carry to acc.
mov {tmp:e}, {acc:e}           # Repeat for 16-bit.
shr {tmp:e}, 16
and {acc:e}, 0x0000ffff
add {acc:x}, {tmp:x}
adc {acc:x}, 0
# Ones' complement.
not {acc:e}                    # Ones' complement of dword acc.
and {acc:e}, 0xffff            # Clear out higher part of dword acc.
# Swap.
xchg {acc:l}, {acc:h}
",
         acc = inout(reg_abcd) acc,
         ptr = inout(reg) ptr => _,
         size = inout(reg) size => _,
         tmp = out(reg) _,
         options(nostack)
    );
    acc as u16
}

#[cfg(target_arch="aarch64")]
unsafe fn checksum(data: &[u8], length: usize, initial: u16) -> u16 {
    let ptr = data.as_ptr();
    let size = length;
    let mut acc = initial as u64;
    // Accumulative sum
    core::arch::asm!("
ands {mod32}, {size}, ~31
rev16 {acc:w}, {acc:w}          // Swap initial to convert to host-bytes order.
b.eq 2f                         // Skip 32 bytes at once block, carry flag cleared (ands)

1:
ldp {tmp1}, {tmp2}, [{ptr}], 16 // Load dword[0..1] and advance input
adds {acc}, {acc}, {tmp1}       // Sum acc with dword[0].
adcs {acc}, {acc}, {tmp2}       // Sum with carry dword[1].
ldp {tmp1}, {tmp2}, [{ptr}], 16 // Load dword[2..3] and advance input
adcs {acc}, {acc}, {tmp1}       // Sum with carry dword[2].
adcs {acc}, {acc}, {tmp2}       // Sum with carry dword[3].
adc {acc}, {acc}, xzr           // Sum carry-bit into acc.
subs {mod32}, {mod32}, 32       // Consume four dwords.
b.gt 1b
tst {mod32}, 32                 // Clear carry flag (set by subs for b.gt)

2:
tbz {size}, 4, 3f               // skip 16 bytes at once block
ldp {tmp1}, {tmp2}, [{ptr}], 16 // Load dword[0..1] and advance
adds {acc}, {acc}, {tmp1}       // Sum with carry dword[0].
adcs {acc}, {acc}, {tmp2}       // Sum with carry dword[1].

3:
tbz {size}, 3, 4f               // skip 8 bytes at once block
ldr {tmp2}, [{ptr}], 8          // Load dword and advance
adcs {acc}, {acc}, {tmp2}       // Sum acc with dword[0]. Accumulate carry.

4:
tbz {size}, 2, 5f               // skip 4 bytes at once block
ldr {tmp1:w}, [{ptr}], 4        // Load word and advance
adcs {acc}, {acc}, {tmp1}       // Sum acc with word[0]. Accumulate carry.

5:
tbz {size}, 1, 6f               // skip 2 bytes at once block
ldrh {tmp1:w}, [{ptr}], 2       // Load hword and advance
adcs {acc}, {acc}, {tmp1}       // Sum acc with hword[0]. Accumulate carry.

6:
tbz {size}, 0, 7f               // If size is less than 1.
ldrb {tmp1:w}, [{ptr}]          // Load byte.
adcs {acc}, {acc}, {tmp1}       // Sum acc with byte. Accumulate carry.

// Fold 64-bit into 16-bit.
7:
lsr {tmp1}, {acc}, 32           // Store high 32 bit of acc in tmp1.
adcs {acc:w}, {acc:w}, {tmp1:w} // 32-bit sum of acc and r1. Accumulate carry.
adc {acc:w}, {acc:w}, wzr       // Sum carry to acc.
uxth {tmp2:w}, {acc:w}          // Repeat for 16-bit.
add {acc:w}, {tmp2:w}, {acc:w}, lsr 16
add {acc:w}, {acc:w}, {acc:w}, lsr 16  // (This sums the carry, if any, into acc.)
// One's complement.
mvn {acc:w}, {acc:w}
// Swap.
rev16 {acc:w}, {acc:w}
",
         acc = inout(reg) acc,
         ptr =  inout(reg) ptr => _,
         size = inout(reg) size => _,
         tmp1 = out(reg) _, tmp2 = out(reg) _,
         mod32 = out(reg) _,
         options(nostack)
    );
    acc as u16
}

#[cfg(test)]
mod selftest {
    use super::*;
    extern crate test;

    #[test]
    fn checksum() {
        let cases: Vec<&[u8]> = vec![
            &[0xffu8, 0xff, 0xff, 0xff, 0xff],
            &[0u8, 0, 0, 0, 0],
            &[42u8, 41, 40, 39, 38, 37, 36, 35, 34, 33, 32, 31, 30, 29, 28],
            &[],
            &[01u8, 02, 03, 04, 05, 06, 07, 08, 09, 10, 11, 12, 13, 14, 15, 16,
              01u8, 02, 03, 04, 05, 06, 07, 08, 09, 10, 11, 12, 13, 14, 15, 16,
              01u8, 02, 03, 04, 05, 06, 07, 08, 09, 10, 11, 12, 13, 14, 15, 16,
              01u8, 02, 03, 04, 05, 06, 07, 08, 09, 10, 11, 12, 13, 14, 15]
        ];
        for case in cases {
            for l in 0..=case.len() {
                let n = checksum_rust(&case, l);
                println!("{:?} {} {}", &case, l, n);
                assert_eq!(ipsum(&case, l, 0), n);
            }
        }
    }

    #[test]
    fn checksum_carry() {
        for l in 2..=63 {
            let mut case = vec![0u8; l];
            for i in 0..=l-2 { case[i] = 0xff; }
            case[l-1] = 0x01;
            let n = checksum_rust(&case, l);
            println!("{:?} {} {}", &case, l, n);
            assert_eq!(ipsum(&case, l, 0), n);
        }
    }

    #[test]
    fn checksum_random() {
        let mut progress = 1;
        for i in 1..=32 { // Crank this up to run more random test cases
            if i >= progress {
                println!("{}", progress);
                progress *= 2;
            }
            for l in 0..=1500 { // Tune this down (to e.g. 63) for faster cases
                let mut case = vec![0u8; l];
                lib::random_bytes(&mut case, l);
                let r = checksum_rust(&case, l);
                let n = ipsum(&case, l, 0);
                if r != n {
                    println!("{:?} len={} ref={} asm={}", &case, l, r, n);
                    panic!("mismatch");
                }
            }
        }
    }

    #[test]
    fn checksum_bench() {
        let nchunks = match std::env::var("RUSH_CHECKSUM_NCHUNKS") {
            Ok(val) => val.parse::<f64>().unwrap() as usize,
            _ => 1_000_000
        };
        let chunksize = match std::env::var("RUSH_CHECKSUM_CHUNKSIZE") {
            Ok(val) => val.parse::<usize>().unwrap(),
            _ => 60
        };
        let mut case = vec![0u8; nchunks];
        lib::random_bytes(&mut case, chunksize);
        let mut i = 0;
        while i < nchunks {
            test::black_box(ipsum(&case, chunksize, 0));
            test::black_box(i += 1);
        }
        println!("Checksummed {} * {} byte chunks", i, chunksize);
    }

    #[test]
    fn checksum_rampool_bench() {
        let nchunks = match std::env::var("RUSH_CHECKSUM_NCHUNKS") {
            Ok(val) => val.parse::<f64>().unwrap() as usize,
            _ => 1_000_000
        };
        let chunksize = match std::env::var("RUSH_CHECKSUM_CHUNKSIZE") {
            Ok(val) => val.parse::<usize>().unwrap(),
            _ => 60
        };
	let poolsize = match std::env::var("RUSH_CHECKSUM_POOLSIZE") {
	    Ok(val) => val.parse::<usize>().unwrap(),
	    _ => 512 * 1024 // Typical ARM L2 cache size
	};
        assert!(poolsize & (poolsize - 1) == 0,
                "poolsize must be a power of two");
	let mut pool = vec![0u8; poolsize+chunksize];
	lib::random_bytes(&mut pool, poolsize+chunksize);
	let mut i = 0;
	while i < nchunks {
	    // Pick a slice with pseudo-random offset
            let x = i as u32 * 0x85ebca6b;
	    let case = &pool[x as usize & (poolsize-1)..];
	    test::black_box(ipsum(&case, chunksize, 0));
	    test::black_box(i += 1);
        }
        println!("Checksummed {} * {} byte chunks (RAM pool size {})",
                 i, chunksize, poolsize);
    }

}
