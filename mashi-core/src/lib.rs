#![no_std]
#![allow(unused, dead_code, internal_features)]
#![feature(core_intrinsics, portable_simd, variant_count)]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

mod compressor;
mod dis_model;
mod model;

#[cfg(not(feature = "std"))]
#[panic_handler]
pub fn panic(_info: &::core::panic::PanicInfo) -> ! {
    #[allow(unused_unsafe)]
    unsafe {
        ::core::intrinsics::unreachable();
    }
}

#[cfg(not(feature = "std"))]
#[global_allocator]
static ALLOCATOR: lol_alloc::AssumeSingleThreaded<lol_alloc::LeakingPageAllocator> =
    unsafe { lol_alloc::AssumeSingleThreaded::new(lol_alloc::LeakingPageAllocator) };

#[cfg(feature = "std")]
pub use compressor::{compress, decompress};

#[cfg(all(feature = "tiny", target_arch = "wasm32"))]
#[unsafe(no_mangle)]
fn decompress(data: *const u8, len: usize) -> *const u8 {
    let data = unsafe {
        core::slice::from_raw_parts(data, len)
    };

    let result = compressor::decompress(&data, |_| ());
    result.as_ptr()
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::compressor::{compress, decompress};
    use std::println;

    #[test]
    fn roundtrip() {
        let src = include_bytes!("../test-data/test.wasm").to_vec();
        let (c, _) = compress(&src, |_| ());
        println!("From {} to {}", src.len(), c.len());
        let out = decompress(&c, |_| ());
        assert_eq!(src, out);
    }
}