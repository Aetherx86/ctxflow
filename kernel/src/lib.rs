//! ctxflow kernel: every decision that costs no tokens, in one portable binary.
//! JSON in, JSON out, no I/O. Python, Node, Electron and browsers load the same .wasm.
#![cfg_attr(target_arch = "wasm32", no_std)]
extern crate alloc;

pub mod compress;
pub mod council;
pub mod gate;
pub mod json;

use alloc::{string::{String, ToString}, vec, vec::Vec};
use json::{obj, parse, st, V};

include!(concat!(env!("OUT_DIR"), "/pool.rs"));

pub fn prompt(i: usize) -> String {
    let mix = |mut z: u32| {
        z = z.wrapping_add(0x9e37_79b9);
        z = (z ^ (z >> 16)).wrapping_mul(0x85eb_ca6b);
        z = (z ^ (z >> 13)).wrapping_mul(0xc2b2_ae35);
        z ^ (z >> 16)
    };
    let b: Vec<u8> = POOL[i].iter().enumerate().map(|(k, x)| x ^ mix(SALT.wrapping_add(k as u32)) as u8).collect();
    String::from_utf8(b).unwrap_or_default()
}

pub fn dispatch(input: &str) -> String {
    let Some(i) = parse(input) else {
        return obj(vec![("error", st("kernel: invalid JSON input"))]).dump();
    };
    let text = i.get("text").s();
    let out = match i.get("op").s() {
        "version" => obj(vec![("version", st(env!("CARGO_PKG_VERSION")))]),
        "estimate" => obj(vec![("tokens", V::Num(compress::estimate(text) as f64))]),
        "compress" => {
            let c = compress::compress(text, i.get("code_max").n(12.0) as usize, i.get("log_max").n(6.0) as usize);
            let (before, after) = (compress::estimate(text), compress::estimate(&c.text));
            obj(vec![("text", st(&c.text)), ("stubs", V::Arr(c.stubs)), ("before", V::Num(before as f64)), ("after", V::Num(after as f64))])
        }
        "anchors" => obj(vec![("anchors", V::Arr(compress::anchors(text, i.get("max").n(40.0) as usize).iter().map(|a| st(a)).collect()))]),
        "verify_summary" => {
            let (s, m) = compress::verify_summary(text, i.get("anchors").arr());
            obj(vec![("text", st(&s)), ("missing", V::Arr(m.iter().map(|a| st(a)).collect()))])
        }
        "error_sig" => obj(vec![("sig", st(&compress::error_sig(text)))]),
        "redact" => {
            let (r, m) = compress::redact(text);
            obj(vec![("text", st(&r)), ("map", V::Arr(m))])
        }
        "gate" => gate::gate(&i),
        "brief" => gate::brief(&i),
        "advisor_system" => obj(vec![("text", st(&prompt(3)))]),
        "council_step" => council::step(&i),
        other => obj(vec![("error", st(&("kernel: unknown op ".to_string() + other)))]),
    };
    out.dump()
}

#[cfg(target_arch = "wasm32")]
mod abi {
    use core::alloc::{GlobalAlloc, Layout};
    use core::ptr::addr_of_mut;

    struct Bump;
    static mut NEXT: usize = 0;
    static mut BASE: usize = 0;
    static mut LAST: usize = 0;
    const PAGE: usize = 65536;

    unsafe impl GlobalAlloc for Bump {
        unsafe fn alloc(&self, l: Layout) -> *mut u8 {
            if *addr_of_mut!(BASE) == 0 {
                let start = core::arch::wasm32::memory_size(0) * PAGE;
                *addr_of_mut!(BASE) = start;
                *addr_of_mut!(NEXT) = start;
            }
            let at = (*addr_of_mut!(NEXT) + l.align() - 1) & !(l.align() - 1);
            let end = at + l.size();
            let have = core::arch::wasm32::memory_size(0) * PAGE;
            if end > have {
                let want = (end - have).div_ceil(PAGE);
                if core::arch::wasm32::memory_grow(0, want) == usize::MAX {
                    return core::ptr::null_mut();
                }
            }
            *addr_of_mut!(NEXT) = end;
            *addr_of_mut!(LAST) = at;
            at as *mut u8
        }
        unsafe fn dealloc(&self, _: *mut u8, _: Layout) {}
        unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
            // Growing strings are the common case; extend in place when this was the last block.
            if p as usize == *addr_of_mut!(LAST) && new >= l.size() {
                let end = p as usize + new;
                let have = core::arch::wasm32::memory_size(0) * PAGE;
                if end > have && core::arch::wasm32::memory_grow(0, (end - have).div_ceil(PAGE)) == usize::MAX {
                    return core::ptr::null_mut();
                }
                *addr_of_mut!(NEXT) = end;
                return p;
            }
            let q = self.alloc(Layout::from_size_align_unchecked(new, l.align()));
            if !q.is_null() {
                core::ptr::copy_nonoverlapping(p, q, l.size().min(new));
            }
            q
        }
    }
    #[global_allocator]
    static A: Bump = Bump;

    #[panic_handler]
    fn panic(_: &core::panic::PanicInfo) -> ! {
        core::arch::wasm32::unreachable()
    }

    #[no_mangle]
    pub unsafe extern "C" fn reset() {
        *addr_of_mut!(NEXT) = *addr_of_mut!(BASE);
        *addr_of_mut!(LAST) = 0;
    }
    #[no_mangle]
    pub unsafe extern "C" fn alloc(n: usize) -> *mut u8 {
        A.alloc(Layout::from_size_align_unchecked(n.max(1), 1))
    }
    /// Returns (ptr << 32) | len.
    #[no_mangle]
    pub unsafe extern "C" fn call(p: *const u8, n: usize) -> u64 {
        let s = core::str::from_utf8(core::slice::from_raw_parts(p, n)).unwrap_or("");
        let out = super::dispatch(s);
        let r = ((out.as_ptr() as u64) << 32) | out.len() as u64;
        core::mem::forget(out);
        r
    }
}

#[cfg(test)]
mod tests;
