#![allow(unused, dead_code, internal_features)]
#![feature(core_intrinsics, portable_simd, variant_count)]

mod compressor;
mod dis_model;
mod model;

pub use compressor::{compress, decompress};

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::model::{Apm, History, Model, APM_CONTEXT_SIZE};
    use rand::prelude::StdRng;
    use rand::{Rng, RngCore, SeedableRng};
    use std::path::PathBuf;
    use std::{fs, println, slice};
    use wasmi::{Caller, Engine, Linker, Module, Store};

    #[test]
    fn roundtrip() {
        let src = include_bytes!("../test-data/test.wasm").to_vec();
        let (c, _) = compress(&src, |_| ());
        println!("From {} to {}", src.len(), c.len());
        let (out, _) = decompress(&c, |_| ());
        assert_eq!(src, out);
    }

    struct HostState;
    struct Test {
        instance: wasmi::Instance,
        store: Store<HostState>,
    }

    impl Test {
        fn new() -> Self {
            let wasm = include_str!("decompress.wat");
            let engine = Engine::default();
            let module = Module::new(&engine, wasm.as_bytes()).unwrap();

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
            linker.func_wrap("host", "l_x32", |caller: Caller<'_, HostState>, param: u32| {
                print!("{param:0X} // ");
            });
            let instance = linker.instantiate_and_start(&mut store, &module).unwrap();

            Self {
                instance,
                store,
            }
        }

        fn memory(&self) -> &[u8] {
            self.instance.get_memory(&self.store, "memory").unwrap().data(&self.store)
        }

        fn model_init(&mut self) {
            self.instance
                .get_typed_func::<(), ()>(&self.store, "model_init").unwrap()
                .call(&mut self.store, ()).unwrap();
        }

        fn history_get(&mut self, history_index: i32, index: i32) -> i32 {
            self.instance
                .get_typed_func::<(i32, i32), (i32)>(&self.store, "history_get").unwrap()
                .call(&mut self.store, (history_index, index)).unwrap()
        }

        fn history_hash(&mut self, history_index: i32, byte_mask: i32) -> i32 {
            self.instance
                .get_typed_func::<(i32, i32), (i32)>(&self.store, "history_hash").unwrap()
                .call(&mut self.store, (history_index, byte_mask)).unwrap()
        }

        fn history_update(&mut self, history_index: i32, byte: i32) {
            self.instance
                .get_typed_func::<(i32, i32), ()>(&self.store, "history_update").unwrap()
                .call(&mut self.store, (history_index, byte)).unwrap();
        }

        fn apm_stage_set_index(&mut self, apm_index: i32, context: i32, prob: i32) {
            self.instance
                .get_typed_func::<(i32, i32, i32), ()>(&self.store, "apm_stage_set_index").unwrap()
                .call(&mut self.store, (apm_index, context, prob)).unwrap();
        }

        fn apm_stage_prob(&mut self, apm_index: i32) -> i32 {
            self.instance
                .get_typed_func::<(i32), (i32)>(&self.store, "apm_stage_prob").unwrap()
                .call(&mut self.store, (apm_index)).unwrap()
        }

        fn apm_stage_update(&mut self, apm_index: i32, bit: i32) {
            self.instance
                .get_typed_func::<(i32, i32), ()>(&self.store, "apm_stage_update").unwrap()
                .call(&mut self.store, (apm_index, bit)).unwrap();
        }

        fn model_prob(&mut self) -> i32 {
            self.instance
                .get_typed_func::<(), (i32)>(&self.store, "model_prob").unwrap()
                .call(&mut self.store, ()).unwrap()
        }

        fn model_update(&mut self, bit: i32) {
            self.instance
                .get_typed_func::<(i32), ()>(&self.store, "model_update").unwrap()
                .call(&mut self.store, (bit)).unwrap();
        }
    }

    #[test]
    fn test_history() {
        let mut r = StdRng::seed_from_u64(42);
        let mut data = [0i32; 262144];
        r.fill(&mut data);

        let mut test = Test::new();
        test.model_init();

        let mut history = History::new();
        for (pos, b) in data.iter().enumerate() {
            let rust_hash = history.hash((*b & 0xff) as u8);
            let wasm_hash = test.history_hash(0, (*b & 0xff) as u8 as i32);
            assert_eq!(rust_hash, wasm_hash as u32, "Mismatch at {pos}");

            let new_byte = (r.next_u32() & 0xff) as u8;
            history.update(new_byte);
            test.history_update(0, new_byte as i32);

            /*for i in 0..HISTORY_BUFFER_LEN {
                assert_eq!(history.get(i), test.history_get(0, i as i32) as u8, "history_get mismatch at {i}, pos: {pos}");
            }*/
        }
    }

    #[test]
    fn test_apm() {
        let mut r = StdRng::seed_from_u64(42);
        let mut probs = [0i32; 262144];
        r.fill(&mut probs);

        let mut test = Test::new();
        test.model_init();

        let mut apm_stages = &mut [
            Apm::new(APM_CONTEXT_SIZE, 3),
            Apm::new(APM_CONTEXT_SIZE, 3),
            Apm::new(APM_CONTEXT_SIZE, 2)
        ];
        for b in &probs {
            let mut apm_index = b.rem_euclid(3);
            let apm_context = r.next_u32();
            let prob = -2047 + (b & 4095);
            test.apm_stage_set_index(apm_index, apm_context as i32, prob);
            apm_stages[apm_index as usize].set_index(apm_context, prob);
            assert_eq!(test.apm_stage_prob(apm_index), apm_stages[apm_index as usize].prob());

            let bit = (b & 1) as u32;
            test.apm_stage_update(apm_index, bit as i32);
            apm_stages[apm_index as usize].update(bit);
        }
    }

    #[test]
    fn test_model() {
        let mut r = StdRng::seed_from_u64(42);
        let mut bits = [0u8; 262144];
        r.fill_bytes(&mut bits);

        let mut test = Test::new();
        test.model_init();

        let mut model = Model::new();
        for (pos, bit) in bits.iter().enumerate() {
            let bit = if *bit < 128 { 0 } else { 1 };
            let rust_prob = model.prob() as i32;
            let wasm_prob = test.model_prob();
            assert_eq!(rust_prob, wasm_prob, "Mismatch at {pos}");

            model.update(bit as u32, false);
            test.model_update(bit);
        }
    }

    #[test]
    fn test_stretch() {
        let mut test = Test::new();
        test.model_init();

        let mut model = Model::new();
        let memory = test.memory()[0x00d0000..0x00d1000].as_ptr() as *const i32;
        let memory: &[i32] = unsafe { slice::from_raw_parts(memory, 4096) };
        assert_eq!(memory, &model.stretch_tab);
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

        let mut test = Test::new();
        {
            let memory = test.instance.get_memory(&test.store, "memory").unwrap().data_mut(&mut test.store);
            memory[..compressed.len()].copy_from_slice(&compressed);
        }

        println!();
        println!();
        println!();
        test.instance
            .get_typed_func::<(), ()>(&test.store, "decompress").unwrap()
            .call(&mut test.store, ()).unwrap();

        let memory = test.instance.get_memory(&test.store, "memory").unwrap().data(&mut test.store);

        /*for i in 0..high_level_decompressed.len() {
            if memory[1024 * 1024 + i] != high_level_decompressed[i] {
                panic!("Difference at offset {i}");
            }
        }*/
        assert_eq!(memory[1024 * 1024..1024 * 1024 + high_level_decompressed.len()], high_level_decompressed);
        //assert_eq!(memory[1024 * 1024..1024 * 1024 + high_level_decompressed.len()], include_bytes!("../test-data/test.wasm").to_vec());
        //assert_eq!(high_level_decompressed.len(), (fs::metadata(&dest).unwrap().len() as usize) - 16);
    }

    // rerun this is the squash_tab changes
    #[test]
    fn squash_tab_generator() {
        let squash_tab: [i32; 33] = [
            1, 2, 3, 6, 10, 16, 27, 45, 73, 120, 194, 310, 488, 747, 1101, 1546, 2047, 2549, 2994, 3348, 3607, 3785, 3901, 3975, 4022, 4050, 4068, 4079,
            4085, 4089, 4092, 4093, 4094,
        ];

        let mut current_line = String::new();

        for (i, &num) in squash_tab.iter().enumerate() {
            let bytes = num.to_le_bytes();
            let escaped: String = bytes.iter()
                .map(|b| format!("\\{:02x}", b))
                .collect();

            current_line.push_str(&escaped);

            if (i + 1) % 4 == 0 {
                println!("\"{}\"", current_line);
                current_line.clear();
            }
        }

        if !current_line.is_empty() {
            println!("\"{}\"", current_line);
        }
    }
}