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
    use std::path::PathBuf;
    use std::{fs, println};

    #[test]
    fn roundtrip() {
        let src = include_bytes!("../test-data/test.wasm").to_vec();
        let (c, _) = compress(&src, |_| ());
        println!("From {} to {}", src.len(), c.len());
        let out = decompress(&c, |_| ());
        assert_eq!(src, out);
    }

    #[test]
    fn test_decompress() -> anyhow::Result<()> {
        use wasmi::*;

        let dest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/test.wasm.mashi");
        if !fs::exists(&dest)? {
            let src = include_bytes!("../test-data/test.wasm").to_vec();
            let (c, _) = compress(&src, |_| ());
            fs::write(&dest, &c)?;
        }
        let compressed = fs::read(&dest)?;
        let high_level_decompressed = decompress(&compressed, |_| ());

        let wasm = include_str!("decompress.wat");
        let engine = Engine::default();
        let module = Module::new(&engine, wasm.as_bytes())?;

        struct HostState;
        let mut store = Store::new(&engine, HostState);
        let mut linker = <Linker<HostState>>::new(&engine);
        linker.func_wrap("host", "log_i32", |caller: Caller<'_, HostState>, param: i32| {
            println!("       log_i32: {param}");
        });
        linker.func_wrap("host", "log_u32", |caller: Caller<'_, HostState>, param: u32| {
            println!("       log_u32: {param}");
        });
        let instance = linker.instantiate_and_start(&mut store, &module)?;

        {
            let memory = instance.get_memory(&store, "memory").unwrap().data_mut(&mut store);
            memory[..compressed.len()].copy_from_slice(&compressed);
        }

        instance
            .get_typed_func::<(), ()>(&store, "decompress")?
            .call(&mut store, ())?;

        let memory = instance.get_memory(&store, "memory").unwrap().data(&mut store);
        assert_eq!(memory[1024 * 1024..1024 * 1024 + high_level_decompressed.len()], high_level_decompressed);

        Ok(())
    }
}