#![allow(unused, dead_code, internal_features)]
#![feature(core_intrinsics, portable_simd, variant_count)]

mod compressor;
mod dis_model;
mod model;

pub use compressor::{compress, decompress};

const DEBUG_LOG: bool = false;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::dis_model::{DisModelState, NUM_DIS_MODEL_STATES};
    use crate::model::{Apm, History, MatchModel, Model, APM_CONTEXT_SIZE, BIT_MASKS, BYTE_MASKS, HISTORY_BUFFER_LEN, NUM_MATCH_MODELS};
    use log::LevelFilter;
    use log4rs::append::file::FileAppender;
    use log4rs::config::{Appender, Root};
    use log4rs::encode::pattern::PatternEncoder;
    use log4rs::Config;
    use num_traits::ToBytes;
    use rand::prelude::StdRng;
    use rand::{Rng, RngCore, SeedableRng};
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;
    use std::simd::i16x8;
    use std::sync::{Arc, Mutex};
    use std::{fs, mem, println, ptr, slice};
    use wasmi::{Caller, Engine, Linker, Module, Store};

    fn init_log() {
        if (DEBUG_LOG) {
            let logfile = FileAppender::builder()
                .append(false)
                .encoder(Box::new(PatternEncoder::new("{m}\n")))
                .build("log/rust.txt").unwrap();

            let config = Config::builder()
                .appender(Appender::builder().build("logfile", Box::new(logfile)))
                .build(Root::builder()
                    .appender("logfile")
                    .build(LevelFilter::Info)).unwrap();

            log4rs::init_config(config).unwrap();
        }
    }

    #[test]
    fn roundtrip() {
        init_log();

        // just a roundtrip of the pure Rust implementation
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
        file: Arc<Mutex<File>>,
    }

    impl Test {
        fn new() -> Self {
            let log_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("log/wasm.txt");
            let mut file = Arc::new(Mutex::new(File::create(&log_path).unwrap()));

            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/decompress.wat");
            let wasm = fs::read_to_string(&path).unwrap();
            let engine = Engine::default();
            let module = Module::new(&engine, wasm.as_bytes()).unwrap();

            let mut store = Store::new(&engine, HostState);
            let mut linker = <Linker<HostState>>::new(&engine);
            let f = file.clone();
            linker.func_wrap("host", "l_sep", move |caller: Caller<'_, HostState>| {
                if DEBUG_LOG { writeln!(f.lock().unwrap(), "============================================================================="); }
            });
            let f = file.clone();
            linker.func_wrap("host", "l_i32", move |caller: Caller<'_, HostState>, param: i32| {
                if DEBUG_LOG { writeln!(f.lock().unwrap(), "{param}"); }
            });
            let f = file.clone();
            linker.func_wrap("host", "l_u32", move |caller: Caller<'_, HostState>, param: u32| {
                if DEBUG_LOG { writeln!(f.lock().unwrap(), "{param}"); }
            });
            let f = file.clone();
            linker.func_wrap("host", "l_x32", move |caller: Caller<'_, HostState>, param: u32| {
                if DEBUG_LOG { writeln!(f.lock().unwrap(), "{param:0X}"); }
            });
            let f = file.clone();
            linker.func_wrap("host", "l_dm", move |caller: Caller<'_, HostState>, state: u32, opcode: i32, byte: i32, read_pos: u32, write_pos: u32| {
                let s: DisModelState = unsafe { mem::transmute(state as u8) };
                if DEBUG_LOG { writeln!(f.lock().unwrap(), "{:?} | self.opcode: {:0X} | incoming byte: {:0X} (r: {}, w: {})", s, opcode, byte, read_pos, write_pos); }
            });
            let instance = linker.instantiate_and_start(&mut store, &module).unwrap();

            Self {
                instance,
                store,
                file,
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

        fn match_model_prob(&mut self, match_model_index: i32) -> i32 {
            self.instance
                .get_typed_func::<(i32), (i32)>(&self.store, "match_model_prob").unwrap()
                .call(&mut self.store, (match_model_index)).unwrap()
        }

        fn match_model_update_bit(&mut self, match_model_index: i32, bit: i32) {
            self.instance
                .get_typed_func::<(i32, i32), ()>(&self.store, "match_model_update_bit").unwrap()
                .call(&mut self.store, (match_model_index, bit)).unwrap()
        }

        fn match_model_update_byte(&mut self, match_model_index: i32, byte_mask: i32) {
            self.instance
                .get_typed_func::<(i32, i32), ()>(&self.store, "match_model_update_byte").unwrap()
                .call(&mut self.store, (match_model_index, byte_mask)).unwrap()
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

        fn model_update(&mut self, bit: i32, is_in_code_section: bool) {
            self.instance
                .get_typed_func::<(i32, i32), ()>(&self.store, "model_update").unwrap()
                .call(&mut self.store, (bit, if is_in_code_section { 1 } else { 0 })).unwrap();
        }
    }

    #[test]
    fn test_history() {
        let mut r = StdRng::seed_from_u64(42);
        let mut data = [0i32; 64];
        r.fill(&mut data);

        let mut test = Test::new();
        test.model_init();

        let mut histories = vec![];
        for _ in 0..22 {
            histories.push(History::new());
        }
        for (pos, b) in data.iter().enumerate() {
            for i in 0..22 {
                let rust_hash = histories[i].hash((*b & 0xff) as u8);
                let wasm_hash = test.history_hash(i as i32, (*b & 0xff) as u8 as i32);
                assert_eq!(rust_hash, wasm_hash as u32, "Mismatch at {pos}");

                let new_byte = (r.next_u32() & 0xff) as u8;
                histories[i].update(new_byte);
                test.history_update(i as i32, new_byte as i32);

                for x in 0..HISTORY_BUFFER_LEN {
                    //assert_eq!(histories[i].get(x), test.history_get(i as i32, x as i32) as u8, "history_get mismatch at {x}, pos: {pos}");
                }
            }
        }
    }

    #[test]
    fn test_match_model() {
        let mut r = StdRng::seed_from_u64(42);
        let mut data = [0i32; 262144];
        r.fill(&mut data);

        let mut test = Test::new();
        test.model_init();

        let mut history = History::new();
        let mut match_models = vec![];
        for i in 0..NUM_MATCH_MODELS {
            match_models.push(MatchModel::new());
        }
        for (pos, b) in data.iter().enumerate() {
            for i in 0..NUM_MATCH_MODELS {
                let rust_prob = match_models[i].prob(&history);
                let wasm_prob = test.match_model_prob(i as i32);
                assert_eq!(rust_prob, wasm_prob as u32, "Mismatch at {pos}, match_model_index {i}");
            }

            let new_byte = (r.next_u32() & 0xff) as u8;

            for bit in 0..8 {
                for i in 0..NUM_MATCH_MODELS {
                    let val = ((new_byte >> bit) & 1) as u32;
                    match_models[i].update_bit(val);
                    test.match_model_update_bit(i as i32, val as i32);
                }
            }

            history.update(new_byte);
            test.history_update(0, new_byte as i32);
            for i in 0..NUM_MATCH_MODELS {
                let byte_mask = ((1 << (i + 1)) - 1) as _;
                match_models[i].update_byte(&history, byte_mask);
                test.match_model_update_byte(i as i32, byte_mask as i32);
            }
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
        let mut min_seed = (u64::MAX, 0);

        // 19,59
        // 73,52

        for seed in 2..=2 {
            let mut r = StdRng::seed_from_u64(seed);
            let mut bits = [0u8; 8];
            r.fill_bytes(&mut bits);

            let mut test = Test::new();
            test.model_init();

            let mut model = Model::new();
            for (pos, bit) in bits.iter().enumerate() {
                let bit = if *bit < 128 { 0 } else { 1 };
                let rust_prob = model.prob() as i32;
                println!();
                let wasm_prob = test.model_prob();
                println!();
                if rust_prob != wasm_prob {
                    if min_seed.0 > seed {
                        min_seed = (seed, pos);
                        //println!("Broken seed: {:?}", min_seed);
                    }
                    break;
                }
                //assert_eq!(rust_prob, wasm_prob, "Mismatch at {pos}");

                model.update(bit as u32, false);
                test.model_update(bit, false);
            }
        }

        println!("min_seed: {:?}", min_seed);
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
    fn test_mix_and_train() {
        let mut r = StdRng::seed_from_u64(42);

        let mut test = Test::new();
        let probs = vec![
            i16x8::from_slice(&[1111, 2222, 3333, 4444, 5555, 6666, 7777, 8888]),
            i16x8::from_slice(&[555, 1555, 2555, 3555, 4555, 5555, 6555, 7555]),
        ];
        let mut weights = vec![
            i16x8::from_slice(&[55, 66, 77, 88, 99, 1010, 1111, 1212]),
            i16x8::from_slice(&[88, 99, 1010, 1111, 1212, 1313, 1414, 1515]),
        ];

        let memory = test.instance.get_memory(&test.store, "memory").unwrap().data_mut(&mut test.store);
        unsafe {
            ptr::copy(probs.as_ptr() as *const _, memory[0..].as_mut_ptr(), 32);
            ptr::copy(weights.as_ptr() as *const _, memory[1024..].as_mut_ptr(), 32);
        }

        // mix and train a while
        for _ in 0..5000 {
            assert_eq!(test.instance
                           .get_typed_func::<(i32, i32, u32), (i32)>(&test.store, "mix").unwrap()
                           .call(&mut test.store, (0, 1024, 2)).unwrap(), model::mix(&probs, &weights, 2));

            loop {
                let bit = (r.next_u32() & 1) as i32;
                let current_prob = -2047 + (r.next_u32() % 4097) as i32;

                let prediction_error = ((bit << 12) - current_prob) * 7;
                if prediction_error < -32768 || prediction_error >= 32768 { continue; }

                model::train(&probs, &mut weights, 2, bit, current_prob);
                test.instance
                    .get_typed_func::<(i32, i32, u32, i32, i32), ()>(&test.store, "train").unwrap()
                    .call(&mut test.store, (0, 1024, 2, bit, current_prob)).unwrap();
                break;
            }
        }
    }

    #[test]
    fn test_indirect_probs() {
        let mut test = Test::new();
        test.model_init();

        let mut model = Model::new();
        for i in 0..NUM_DIS_MODEL_STATES + 1 {
            const LENGTH_IN_BYTES: usize = 0x540000;
            let start = 0x1ac00000 + i * LENGTH_IN_BYTES;
            let memory = test.memory()[start..start + LENGTH_IN_BYTES].as_ptr() as *const u16;
            let memory: &[u16] = unsafe { slice::from_raw_parts(memory, LENGTH_IN_BYTES / size_of::<u16>()) };
            assert_eq!(memory.len(), model.dis_model_contexts[0].indirect_probs.len());
            assert_eq!(memory, model.dis_model_contexts[0].indirect_probs);
        }
    }

    fn wasm_roundtrip(src: &[u8]) {
        let (compressed, _) = compress(&src, |_| ());

        init_log();

        let (high_level_decompressed, model) = decompress(&compressed, |_| ());
        assert_eq!(&high_level_decompressed, &src);

        let mut test = Test::new();
        {
            let memory = test.instance.get_memory(&test.store, "memory").unwrap().data_mut(&mut test.store);
            memory[..compressed.len()].copy_from_slice(&compressed);
        }

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
    }

    #[test]
    fn test_wasm_tiny_roundtrip() {
        wasm_roundtrip(include_bytes!("../test-data/add.wasm"));
    }

    #[test]
    fn test_wasm_full_roundtrip() {
        wasm_roundtrip(include_bytes!("../test-data/test.wasm"));
    }

    fn print_tab<T>(tab: &[T], items_per_line: usize) where T: ToBytes {
        let mut current_line = String::new();

        for (i, num) in tab.into_iter().enumerate() {
            let bytes = num.to_le_bytes();
            let escaped: String = bytes.as_ref().iter()
                .map(|b| format!("\\{:02x}", b))
                .collect();

            current_line.push_str(&escaped);

            if (i + 1) % items_per_line == 0 {
                println!("\"{}\"", current_line);
                current_line.clear();
            }
        }

        if !current_line.is_empty() {
            println!("\"{}\"", current_line);
        }
    }

    // rerun if table changes
    #[test]
    fn squash_tab_generator() {
        let squash_tab: [i32; 33] = [
            1, 2, 3, 6, 10, 16, 27, 45, 73, 120, 194, 310, 488, 747, 1101, 1546, 2047, 2549, 2994, 3348, 3607, 3785, 3901, 3975, 4022, 4050, 4068, 4079,
            4085, 4089, 4092, 4093, 4094,
        ];
        print_tab(&squash_tab, 4);
    }

    // rerun if table changes
    #[test]
    fn bit_masks_tab_generator() {
        print_tab(BIT_MASKS, 16);
    }

    // rerun if table changes
    #[test]
    fn byte_masks_tab_generator() {
        print_tab(BYTE_MASKS, 16);
    }
}