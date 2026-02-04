#![allow(unused, dead_code, internal_features)]
#![feature(core_intrinsics, portable_simd, variant_count)]

mod compressor;
mod dis_model;
mod model;

pub use compressor::{compress, decompress};

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::{fs, println, slice};

    #[test]
    fn roundtrip() {
        let src = include_bytes!("../test-data/test.wasm").to_vec();
        let (c, _) = compress(&src, |_| ());
        println!("From {} to {}", src.len(), c.len());
        let (out, _) = decompress(&c, |_| ());
        assert_eq!(src, out);
    }

    #[test]
    fn test_decompress() {
        use wasmi::*;

        let dest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/test.wasm.mashi");
        if !fs::exists(&dest).unwrap() {
            let src = include_bytes!("../test-data/test.wasm").to_vec();
            let (c, _) = compress(&src, |_| ());
            fs::write(&dest, &c).unwrap();
        }
        let compressed = fs::read(&dest).unwrap();
        let (high_level_decompressed, model) = decompress(&compressed, |_| ());

        let wasm = include_str!("decompress.wat");
        let engine = Engine::default();
        let module = Module::new(&engine, wasm.as_bytes()).unwrap();

        struct HostState;
        let mut store = Store::new(&engine, HostState);
        let mut linker = <Linker<HostState>>::new(&engine);
        linker.func_wrap("host", "log_i32", |caller: Caller<'_, HostState>, param: i32| {
            println!("       log_i32: {param}");
        });
        linker.func_wrap("host", "log_u32", |caller: Caller<'_, HostState>, param: u32| {
            println!("       log_u32: {param}");
        });
        linker.func_wrap("host", "l_u32", |caller: Caller<'_, HostState>, param: u32| {
            print!("{param} // ");
        });
        let instance = linker.instantiate_and_start(&mut store, &module).unwrap();

        {
            let memory = instance.get_memory(&store, "memory").unwrap().data_mut(&mut store);
            memory[..compressed.len()].copy_from_slice(&compressed);
        }

        println!();
        instance
            .get_typed_func::<(), ()>(&store, "decompress").unwrap()
            .call(&mut store, ()).unwrap();

        let memory = instance.get_memory(&store, "memory").unwrap().data(&mut store);

        // check stretch_tab
        {
            let memory = memory[0x00d0000..0x00d1000].as_ptr() as *const i32;
            let memory: &[i32] = unsafe { slice::from_raw_parts(memory, 4096) };
            assert_eq!(memory, &model.stretch_tab);
        }

        /*for i in 0..high_level_decompressed.len() {
            if memory[1024 * 1024 + i] != high_level_decompressed[i] {
                panic!("Difference at offset {i}");
            }
        }*/
        assert_eq!(memory[1024 * 1024..1024 * 1024 + high_level_decompressed.len()], high_level_decompressed);
        //assert_eq!(memory[1024 * 1024..1024 * 1024 + high_level_decompressed.len()], include_bytes!("../test-data/test.wasm").to_vec());
        //assert_eq!(high_level_decompressed.len(), (fs::metadata(&dest).unwrap().len() as usize) - 16);
    }

    #[test]
    fn squash_tab_generator() {
        let squash_tab: [i32; 33] = [
            1, 2, 3, 6, 10, 16, 27, 45, 73, 120, 194, 310, 488, 747, 1101, 1546, 2047, 2549, 2994, 3348, 3607, 3785, 3901, 3975, 4022, 4050, 4068, 4079,
            4085, 4089, 4092, 4093, 4094,
        ];

        // We will accumulate the string here
        let mut current_line = String::new();

        // Iterate through the array
        for (i, &num) in squash_tab.iter().enumerate() {
            // 1. Convert integer to 4 bytes (Little Endian)
            let bytes = num.to_le_bytes();

            // 2. Format as WAT hex string (e.g. \01\00\00\00)
            let escaped: String = bytes.iter()
                .map(|b| format!("\\{:02x}", b))
                .collect();

            current_line.push_str(&escaped);

            // Optional: Break lines every 4 integers for readability
            if (i + 1) % 4 == 0 {
                println!("\"{}\"", current_line);
                current_line.clear();
            }
        }

        // Print any remaining items
        if !current_line.is_empty() {
            println!("\"{}\"", current_line);
        }
    }
}