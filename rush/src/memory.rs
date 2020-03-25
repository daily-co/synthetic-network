// For more information about huge pages checkout:
// * HugeTLB - Large Page Support in the Linux kernel
//   http://linuxgazette.net/155/krishnakumar.html
// * linux/Documentation/vm/hugetlbpage.txt
//   https://www.kernel.org/doc/Documentation/vm/hugetlbpage.txt

use super::lib;

use std::ffi;
use regex::Regex;
use once_cell::unsync::Lazy;

// Serve small allocations from hugepage "chunks"

// List of all allocated huge pages: {pointer, size, used}
// The last element is used to service new DMA allocations.
struct Chunk {
    pointer: u64,
    size: usize,
    used: usize
}
static mut CHUNKS: Lazy<Vec<Chunk>> = Lazy::new(|| Vec::new());

// Allocate DMA-friendly memory. Return virtual memory pointer.
pub fn dma_alloc(bytes: usize,  align: usize) -> *mut u8 {
    assert!(bytes <= huge_page_size());
    // Get current chunk of memory to allocate from
    if unsafe { CHUNKS.len() } == 0 { allocate_next_chunk() }
    let mut chunk = unsafe { CHUNKS.last_mut().unwrap() };
    // Skip allocation forward pointer to suit alignment
    chunk.used = lib::align(chunk.used, align);
    // Need a new chunk to service this allocation?
    if chunk.used + bytes > chunk.size {
        allocate_next_chunk();
        chunk = unsafe { CHUNKS.last_mut().unwrap() };
    }
    // Slice out the memory we need
    let offset = chunk.used;
    chunk.used = chunk.used + bytes;
    (chunk.pointer + (offset as u64)) as *mut u8
}

// Add a new chunk.
fn allocate_next_chunk() {
    let ptr = allocate_hugetlb_chunk();
    let chunk = Chunk { pointer: ptr as u64,
                        size: huge_page_size(),
                        used: 0 };
    unsafe { CHUNKS.push(chunk); }
}

// HugeTLB: Allocate contiguous memory in bulk from Linux

fn allocate_hugetlb_chunk() -> *mut ffi::c_void {
    if let Ok(ptr) = std::panic::catch_unwind(|| {
        allocate_huge_page(huge_page_size())
    }) { ptr } else { panic!("Failed to allocate a huge page for DMA"); }
}

// Huge page size in bytes
static mut HUGE_PAGE_SIZE: Option<usize> = None;
fn huge_page_size () -> usize {
    match unsafe { HUGE_PAGE_SIZE } {
        Some(size) => size,
        None => unsafe { HUGE_PAGE_SIZE = Some(get_huge_page_size());
                         HUGE_PAGE_SIZE.unwrap() }
    }
}

fn get_huge_page_size () -> usize {
    let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap();
    let re = Regex::new(r"Hugepagesize: +([0-9]+) kB").unwrap();
    if let Some(cap) = re.captures(&meminfo) {
        (&cap[1]).parse::<usize>().unwrap() * 1024
    } else { panic!("Failed to get hugepage size"); }
}

// Physical memory allocation
//
// Allocate HugeTLB memory pages for DMA. HugeTLB memory is always
// mapped to a virtual address with a specific scheme:
//
//   virtual_address = physical_address | 0x500000000000ULL
//
// This makes it possible to resolve physical addresses directly from
// virtual addresses (remove the tag bits) and to test addresses for
// validity (check the tag bits).

// Tag applied to physical addresses to calculate virtual address.
const TAG: u64 = 0x500000000000;

// virtual_to_physical(ptr) -> u64
//
// Return the physical address of specially mapped DMA memory.
pub fn virtual_to_physical(virt_addr: *const u8) -> u64 {
    let virt_addr = virt_addr as u64;
    assert!(virt_addr & 0x500000000000 == 0x500000000000,
            "Invalid DMA address: 0x{:x}\nDMA address tag check failed",
            virt_addr);
    virt_addr ^ 0x500000000000
}

// Map a new HugeTLB page to an appropriate virtual address.
//
// The page is allocated via the hugetlbfs filesystem
// /var/run/rush/hugetlbfs that is mounted automatically.
// The page has to be file-backed because the Linux kernel seems to
// not support remap() on anonymous pages.
//
// Further reading:
//   https://www.kernel.org/doc/Documentation/vm/hugetlbpage.txt
//   http://stackoverflow.com/questions/27997934/mremap2-with-hugetlb-to-change-virtual-address
fn allocate_huge_page(size: usize) -> *mut ffi::c_void {
    ensure_hugetlbfs();
    unsafe {
        let tmpfile = cstr(&format!("/var/run/rush/hugetlbfs/alloc.{}",
                                    libc::getpid()));
        let fd = libc::open(tmpfile.as_ptr(), libc::O_CREAT|libc::O_RDWR, 0o700);
        assert!(fd >= 0, "create hugetlb");
        assert!(libc::ftruncate(fd, size as i64) == 0, "ftruncate");
        let tmpptr = libc::mmap(std::ptr::null_mut(), size,
                                libc::PROT_READ | libc::PROT_WRITE,
                                libc::MAP_SHARED, fd, 0);
        assert!(tmpptr != libc::MAP_FAILED, "mmap hugetlb");
        assert!(libc::mlock(tmpptr, size) == 0, "mlock");
        let phys = resolve_physical(tmpptr);
        let virt = phys | TAG;
        let ptr = libc::mmap(virt as *mut ffi::c_void, size,
                             libc::PROT_READ | libc::PROT_WRITE,
                             libc::MAP_SHARED | libc::MAP_FIXED, fd, 0);
        libc::unlink(tmpfile.as_ptr());
        libc::munmap(tmpptr, size);
        libc::close(fd);
        ptr
    }
}

// Make sure that /var/run/rush/hugetlbfs is mounted.
fn ensure_hugetlbfs() {
    let target = cstr("/var/run/rush/hugetlbfs");
    let source = cstr("none");
    let fstype = cstr("hugetlbfs");
    let flags = // XXX: RW?
        libc::MS_NOSUID|libc::MS_NODEV|libc::MS_NOEXEC|libc::MS_RELATIME;
    unsafe {
        libc::mkdir(cstr("/var/run/rush").as_ptr(), 0o755);
        libc::mkdir(target.as_ptr(), 0o755);
        if libc::mount(source.as_ptr(), target.as_ptr(), fstype.as_ptr(),
                       flags | libc::MS_REMOUNT, std::ptr::null_mut()) != 0 {
            println!("[mounting /var/run/rush/hugetlbfs]");
            assert!(libc::mount(source.as_ptr(), target.as_ptr(), fstype.as_ptr(),
                                flags, std::ptr::null_mut()) == 0,
                    "failed to (re)mount /var/run/rush/hugetlbfs");
        }
    }
}

// resolve_physical(ptr) => uint64_t
//
// Resolve the physical address of the given pointer via the kernel.
fn resolve_physical(ptr: *const ffi::c_void) -> u64 {
    unsafe {
        let pagesize = 4096;
        let virtpage = ptr as u64 / pagesize;
        let pagemap = cstr("/proc/self/pagemap");
        let pagemapfd = libc::open(pagemap.as_ptr(), libc::O_RDONLY);
        assert!(pagemapfd >= 0, "Failed to open /proc/self/pagemap");
        let mut data: [u64; 1] = [0];
        assert!(libc::pread(pagemapfd, cptr(&mut data), 8, virtpage as i64 * 8) == 8);
        libc::close(pagemapfd);
        assert!(data[0] & (1<<63) != 0, "page not present");
        let physpage = data[0] & 0xFFFFFFFFFFFFF;
        physpage * pagesize
    }
}

fn cstr(s: &str) -> ffi::CString {
    ffi::CString::new(s).expect("cstr failed")
}

fn cptr<T>(ptr: &mut T) -> *mut ffi::c_void {
    ptr as *mut T as *mut ffi::c_void
}
