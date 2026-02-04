extern crate alloc;
use crate::dis_model::{DisModel, NUM_DIS_MODEL_STATES};
use alloc::alloc::{alloc_zeroed, dealloc};
use alloc::vec;
use alloc::vec::Vec;
use core::alloc::Layout;
use core::mem::size_of;
use core::simd::prelude::*;

const NUM_BASE_CONTEXT_MODELS: usize = 21;
const BYTE_MASKS: &[u8; NUM_BASE_CONTEXT_MODELS] = &[0, 1, 3, 17, 128, 2, 4, 5, 53, 76, 33, 7, 15, 6, 136, 193, 224, 243, 8, 9, 26];
const BIT_MASKS: &[u8; NUM_BASE_CONTEXT_MODELS] = &[
    127, 255, 255, 0, 131, 225, 191, 255, 127, 255, 207, 0, 127, 255, 223, 127, 63, 123, 255, 111, 191,
];

const HASH_MB: usize = 128;
const HASH_ENTRIES: usize = HASH_MB * 1024 * 1024 / 16;

const NUM_MIN_ACTIVE_CONTEXT_MODELS: usize = NUM_BASE_CONTEXT_MODELS;
const NUM_MAX_ACTIVE_CONTEXT_MODELS: usize = NUM_BASE_CONTEXT_MODELS * 2;
const NUM_CONST_MODELS: usize = 1;
const NUM_MATCH_MODELS: usize = 8;
const MIX_VECTOR_SIZE: usize = 8;
const NUM_MIN_MODEL_OUTPUTS: usize =
    (NUM_MIN_ACTIVE_CONTEXT_MODELS * 3 + NUM_CONST_MODELS + NUM_MATCH_MODELS + (MIX_VECTOR_SIZE - 1)) & !(MIX_VECTOR_SIZE - 1);
const NUM_MAX_MODEL_OUTPUTS: usize =
    (NUM_MAX_ACTIVE_CONTEXT_MODELS * 3 + NUM_CONST_MODELS + NUM_MATCH_MODELS + (MIX_VECTOR_SIZE - 1)) & !(MIX_VECTOR_SIZE - 1);

const APM_TAB_SIZE: usize = 16;
const APM_CONTEXT_SIZE: usize = 0x10000;
const NUM_APM_STAGES: usize = 3;

mod fnv {
    pub const DEFAULT: u32 = 2166136261;

    pub fn hash_byte(state: &mut u32, value: u8) {
        *state = ((*state).wrapping_mul(16777619)) ^ (value as u32);
    }
}

#[derive(Clone, Copy)]
struct HashTableEntry {
    checksum: u32,
    stationary_counts: u32,
    indirect_counts: u32,

    run_count: u16,
    run_symbol: u16,
}

impl HashTableEntry {
    fn new() -> HashTableEntry {
        HashTableEntry {
            checksum: 0,
            stationary_counts: 1 << 21,
            indirect_counts: 0,

            run_count: 0,
            run_symbol: 0,
        }
    }
}

const HISTORY_BUFFER_LEN: usize = 0x00100000;

const INDEX_BUFFER_LEN: usize = HISTORY_BUFFER_LEN / 4;

struct History {
    byte_history: Vec<u8>,
    byte_history_pos: usize,
}

impl History {
    pub fn new() -> History {
        History {
            byte_history: vec![0; HISTORY_BUFFER_LEN],
            byte_history_pos: 0,
        }
    }

    pub fn get(&self, index: usize) -> u8 {
        self.byte_history[index & (HISTORY_BUFFER_LEN - 1)]
    }

    pub fn hash(&self, byte_mask: u8) -> u32 {
        let mut state = fnv::DEFAULT;

        fnv::hash_byte(&mut state, byte_mask);

        for i in 0..8 {
            if ((byte_mask >> i) & 0x01) == 1 {
                fnv::hash_byte(&mut state, self.get((self.byte_history_pos as i32 - 1 - i) as usize));
            }
        }

        state
    }

    pub fn update(&mut self, byte: u8) {
        self.byte_history[self.byte_history_pos] = byte;
        self.byte_history_pos = (self.byte_history_pos + 1) & (HISTORY_BUFFER_LEN - 1);
    }
}

struct MatchModel {
    index_buffer: Vec<usize>,

    bit_position: u32,

    offset: usize,
    length: usize,

    history_hash: u32,
    predicted_bit: u32,
}

impl MatchModel {
    fn new() -> MatchModel {
        MatchModel {
            index_buffer: vec![0; INDEX_BUFFER_LEN],

            bit_position: 0,

            offset: 0,
            length: 0,

            history_hash: 0,
            predicted_bit: 0,
        }
    }

    // Returns p(1) in [0, 4096)
    pub fn prob(&mut self, history: &History) -> u32 {
        if self.length == 0 {
            2048
        } else {
            self.predicted_bit = ((history.get(history.byte_history_pos - self.offset) >> (7 - self.bit_position)) & 0x01) as _;
            (((2048 / (self.length as i32)) * ((self.predicted_bit as i32) * -2 + 1)) & 0x0fff) as _
        }
    }

    // update assumes prob has been called _once_ for the current bit _before_ update has been called!
    pub fn update_bit(&mut self, bit: u32) {
        if self.predicted_bit != bit {
            // Mismatch; clear length (and thus current match we're tracking)
            self.length = 0;
        }

        self.bit_position += 1;
    }

    pub fn update_byte(&mut self, history: &History, byte_mask: u8) {
        self.bit_position = 0;

        let history_pos = history.byte_history_pos - 1;

        if self.length == 0 {
            // We don't have a match currently; let's look for a new one
            self.offset = history_pos - self.index_buffer[self.history_hash as usize];
            if (self.offset & (HISTORY_BUFFER_LEN - 1)) > 0 {
                while self.length < 255
                    && history.get((history_pos as i32 - self.length as i32) as usize)
                    == history.get((history_pos as i32 - self.length as i32 - self.offset as i32) as usize)
                {
                    self.length += 1;
                }
            }
        } else if self.length < 255 {
            // We're already tracking a match; let's increment its length if it's not already saturated
            self.length += 1;
        }

        // Update index buffer for current history hash to point to the match we're tracking
        self.index_buffer[self.history_hash as usize] = history_pos;

        self.history_hash = history.hash(byte_mask) & ((INDEX_BUFFER_LEN - 1) as u32);
    }
}

pub struct Apm {
    pub tab: Vec<i16>,
    adjust_rate: i32,

    index: usize,
    weight: i32,
}

impl Apm {
    // adjust_rate should default to ~7 (smaller = faster)
    pub fn new(num_contexts: usize, adjust_rate: i32) -> Apm {
        let tab_size = num_contexts * (APM_TAB_SIZE + 1);
        let mut tab = vec![0; tab_size];
        for i in 0..num_contexts {
            for x in 0..(APM_TAB_SIZE + 1) {
                tab[i * (APM_TAB_SIZE + 1) + x] = squash((x as i32 * (4096 / APM_TAB_SIZE) as i32 - 2047)) as i16;
            }
        }

        Apm {
            tab: tab,
            adjust_rate: adjust_rate,

            index: 0,
            weight: 0,
        }
    }

    // Assumes prob is stretched
    pub fn set_index(&mut self, mut context: u32, mut prob: i32) {
        context &= (APM_CONTEXT_SIZE as u32) - 1;

        prob += 2047;
        if prob < 0 {
            prob = 0;
        }
        if prob > 4095 {
            prob = 4095;
        }

        self.index = (context as usize) * (APM_TAB_SIZE + 1) + ((prob >> 8) as usize);
        self.weight = prob & 0xff;
        //print!("{} // {} // {} // ", prob, self.index, self.weight);
    }

    // Assumes set_index has been called already for the current bit
    pub fn prob(&self) -> i32 {
        let a = self.tab[self.index] as i32;
        let b = self.tab[self.index + 1] as i32;
        //print!("{} // {a} // {b} // ", self.index);
        a + (((b - a) * self.weight) >> 8)
    }

    // Assumes set_index has been called already for the current bit
    pub fn update(&mut self, bit: u32) {
        let index = self.index;
        self.update_entry(index, bit);
        self.update_entry(index + 1, bit);
    }

    fn update_entry(&mut self, index: usize, bit: u32) {
        let scaled_bit = (bit << 12) as i32;
        let mut entry = self.tab[index] as i32;
        entry += (scaled_bit - entry) >> self.adjust_rate;
        //print!("{index} // {entry} // ");
        self.tab[index] = entry as _;
    }
}

pub struct DisModelContext {
    stage_1_weights: Vec<i16x8>,
    stage_2_weights: Vec<i16x8>,
    indirect_probs: Vec<u16>,
    pub apm_stages: Vec<Apm>,
}

impl DisModelContext {
    pub fn new() -> DisModelContext {
        let mut indirect_probs = Vec::new();
        for _ in 0..NUM_MAX_ACTIVE_CONTEXT_MODELS {
            for _last_bits in 0..16 {
                for counts_one in 0..64 {
                    for counts_zero in 0..64 {
                        let epsilon = 1 << 1;
                        let shifted_counts_zero = counts_zero << 4;
                        let shifted_counts_one = counts_one << 4;

                        let prob = (shifted_counts_one + epsilon / 2) * (4096 << 4) / (shifted_counts_zero + shifted_counts_one + epsilon);

                        indirect_probs.push(prob as u16);
                    }
                }
            }
        }

        let mut apm_stages = Vec::new();
        apm_stages.push(Apm::new(APM_CONTEXT_SIZE, 3));
        apm_stages.push(Apm::new(APM_CONTEXT_SIZE, 3));
        apm_stages.push(Apm::new(APM_CONTEXT_SIZE, 2));

        DisModelContext {
            stage_1_weights: vec![i16x8::splat(0); NUM_MAX_MODEL_OUTPUTS * 256 * 8 / MIX_VECTOR_SIZE],
            stage_2_weights: vec![i16x8::splat(0); 8 / MIX_VECTOR_SIZE],
            indirect_probs,
            apm_stages,
        }
    }
}

pub struct Model {
    histories: Vec<History>,

    dis_model: DisModel,
    dis_model_state: u32,

    hash_table: *mut HashTableEntry,
    hash_table_layout: Layout,

    pub dis_model_contexts: Vec<DisModelContext>,

    context_model_byte_hashes: Vec<u32>,
    context_model_hashes: Vec<u32>,
    context_model_indirect_prob_indices: Vec<usize>,

    stage_1_probs: Vec<i16x8>,
    stage_1_weight_contexts: Vec<i32>,
    stage_2_probs: Vec<i16x8>,
    stage_2_prob: i32,

    pub stretch_tab: [i32; 4096],

    apm_mix_weights: Vec<u8>,

    bit_history: u8,
    bit_index: u32,
    bit_history_hash: u8,
    num_active_context_models: usize,
    num_model_outputs: usize,

    match_models: Vec<MatchModel>,
}

impl Drop for Model {
    fn drop(&mut self) {
        unsafe {
            dealloc(self.hash_table as *mut _, self.hash_table_layout);
        }
    }
}

impl Model {
    pub fn new() -> Model {
        let mut histories = Vec::new();
        for _ in 0..(NUM_DIS_MODEL_STATES + 1) {
            histories.push(History::new());
        }

        let hash_table_layout = Layout::from_size_align(size_of::<HashTableEntry>() * HASH_ENTRIES, 64).unwrap();

        let mut dis_model_contexts = Vec::new();
        for _ in 0..(NUM_DIS_MODEL_STATES + 1) {
            dis_model_contexts.push(DisModelContext::new());
        }

        let mut stretch_tab = [0; 4096];
        let mut pi = 0;
        for x in -2047..2049 {
            let i = squash(x);
            for j in pi..i + 1 {
                stretch_tab[j as usize] = x;
            }
            pi = i + 1;
        }

        let mut match_models = Vec::new();
        for _ in 0..NUM_MATCH_MODELS {
            match_models.push(MatchModel::new());
        }

        Model {
            histories,

            dis_model: DisModel::new(),
            dis_model_state: 0,

            hash_table: unsafe { alloc_zeroed(hash_table_layout.clone()) } as *mut _,
            hash_table_layout,

            dis_model_contexts,

            context_model_byte_hashes: vec![0; NUM_MAX_ACTIVE_CONTEXT_MODELS],
            context_model_hashes: vec![0; NUM_MAX_ACTIVE_CONTEXT_MODELS],
            context_model_indirect_prob_indices: vec![0; NUM_MAX_ACTIVE_CONTEXT_MODELS],

            stage_1_probs: vec![i16x8::splat(0); NUM_MAX_MODEL_OUTPUTS / MIX_VECTOR_SIZE],
            stage_1_weight_contexts: vec![0; 8],
            stage_2_probs: vec![i16x8::splat(0); 8 / MIX_VECTOR_SIZE],
            stage_2_prob: 0,

            stretch_tab,

            apm_mix_weights: vec![2, 2, 1],

            bit_history: 0,
            bit_index: 0,
            bit_history_hash: 0,
            num_active_context_models: NUM_MIN_ACTIVE_CONTEXT_MODELS,
            num_model_outputs: NUM_MIN_MODEL_OUTPUTS,

            match_models,
        }
    }

    // Returns p(1) in [0, 4096)
    pub fn prob(&mut self) -> u32 {
        let dis_model_context = &mut self.dis_model_contexts[self.dis_model_state as usize];

        self.bit_history_hash = (1 << self.bit_index) | self.bit_history;
        /*
                let mut probs_ptr: *mut i16 = self.stage_1_probs.as_ptr() as *mut _;
                for i in 0..self.num_active_context_models {
                    let mut hash = self.context_model_byte_hashes[i];
                    hash ^= ((1 << self.bit_index) | (self.bit_history & BIT_MASKS[i % NUM_BASE_CONTEXT_MODELS])) as u32;

                    let checksum = hash;

                    const BUCKET_SIZE: u32 = 4;
                    hash = (hash & ((HASH_ENTRIES as u32) / BUCKET_SIZE - 1)) * BUCKET_SIZE;

                    // Check checksums in bucket, starting with the first entry (LRU)
                    let mut bucket_index = 0;
                    while bucket_index < BUCKET_SIZE && unsafe { (*self.hash_table.offset(hash as _)).checksum } != checksum {
                        hash += 1;
                        bucket_index += 1;
                    }
                    // If no checksums match in bucket, adjust hash/bucket_index to point to last entry and replace it with a new entry
                    if bucket_index == BUCKET_SIZE {
                        hash -= 1;
                        bucket_index -= 1;
                        unsafe { *self.hash_table.offset(hash as _) = HashTableEntry::new() };
                    }
                    // Swap entries and adjust hash/bucket_index until current entry is the first in the bucket (LRU)
                    while bucket_index > 0 {
                        let swap_entry = unsafe { *self.hash_table.offset(hash as _) };
                        unsafe { *self.hash_table.offset(hash as _) = *self.hash_table.offset((hash - 1) as _) };
                        hash -= 1;
                        bucket_index -= 1;
                        unsafe { *self.hash_table.offset(hash as _) = swap_entry };
                    }

                    let entry = unsafe { &mut *self.hash_table.offset(hash as _) };
                    entry.checksum = checksum;

                    self.context_model_hashes[i] = hash;

                    // Indirect model
                    let counts = entry.indirect_counts;
                    let indirect_prob_index = (i << 16) | (counts as usize);
                    self.context_model_indirect_prob_indices[i] = indirect_prob_index;
                    let prediction = stretch((dis_model_context.indirect_probs[indirect_prob_index] as u32) >> 4, &self.stretch_tab);

                    unsafe {
                        *probs_ptr = prediction as _;
                        probs_ptr = probs_ptr.offset(1);
                    }

                    // Stationary model
                    let counts = entry.stationary_counts;
                    let prob = counts & 0x003fffff;
                    let prediction = stretch(prob >> 10, &self.stretch_tab);

                    unsafe {
                        *probs_ptr = prediction as _;
                        probs_ptr = probs_ptr.offset(1);
                    }

                    // Run model
                    let count = entry.run_count as i32;
                    let symbol = entry.run_symbol as i32;

                    let prediction = stretch((((2048 / (count + 1)) * (symbol * -2 + 1)) & 0x0fff) as _, &self.stretch_tab);

                    unsafe {
                        *probs_ptr = prediction as _;
                        probs_ptr = probs_ptr.offset(1);
                    }
                }

                for _ in 0..NUM_CONST_MODELS {
                    let prediction = 1024;

                    unsafe {
                        *probs_ptr = prediction;
                        probs_ptr = probs_ptr.offset(1);
                    }
                }

                for i in 0..NUM_MATCH_MODELS {
                    let prob = self.match_models[i].prob(&self.histories[0]);
                    let prediction = stretch(prob, &self.stretch_tab);

                    unsafe {
                        *probs_ptr = prediction as _;
                        probs_ptr = probs_ptr.offset(1);
                    }
                }

                for i in 0..4 {
                    self.stage_1_weight_contexts[i] = self.histories[0].get((self.histories[0].byte_history_pos as i32 - 1 - i as i32) as usize) as _;
                }
                self.stage_1_weight_contexts[4] = (self.histories[0].hash(0xff) & 0xff) as _;
                self.stage_1_weight_contexts[5] = self.bit_history_hash as _;
                self.stage_1_weight_contexts[6] = ((squash(unsafe { *(self.stage_1_probs.as_ptr() as *const i16).offset((self.num_model_outputs - 1) as _) } as i32) as i32) >> 6) // Last match model prob (i.e. some function of the [likely] longest match length/prediction)
                    | if self.histories[0].get((self.histories[0].byte_history_pos as i32 - 1) as usize) == self.histories[0].get((self.histories[0].byte_history_pos as i32 - 2) as usize) { 1 << 6 } else { 0 }
                    | if self.histories[0].get((self.histories[0].byte_history_pos as i32 - 2) as usize) == self.histories[0].get((self.histories[0].byte_history_pos as i32 - 3) as usize) { 1 << 7 } else { 0 };
                let mut probs_ptr: *mut i16 = self.stage_2_probs.as_ptr() as *mut _;
                for i in 0..8 {
                    let prediction = mix(
                        &self.stage_1_probs,
                        &dis_model_context.stage_1_weights
                            [(i * 256 + (self.stage_1_weight_contexts[i] as usize)) * NUM_MAX_MODEL_OUTPUTS / MIX_VECTOR_SIZE..],
                        self.num_model_outputs / MIX_VECTOR_SIZE,
                    );

                    unsafe {
                        *probs_ptr = prediction as _;
                        probs_ptr = probs_ptr.offset(1);
                    }
                }
                self.stage_2_prob = mix(&self.stage_2_probs, &dis_model_context.stage_2_weights, 8 / MIX_VECTOR_SIZE);
        */
        self.stage_2_prob = 0;

        let mut prob = self.stage_2_prob;
        for i in 0..NUM_APM_STAGES {
            let mut apm_context = self.histories[0].hash((1 << i) - 1);
            fnv::hash_byte(&mut apm_context, self.bit_history_hash);
            dis_model_context.apm_stages[i].set_index(apm_context, prob);
            prob += ((stretch(dis_model_context.apm_stages[i].prob() as _, &self.stretch_tab) - prob) * (self.apm_mix_weights[i] as i32)) >> 4;
        }
        prob = squash(prob) as _;

        let prob_margin = 1;

        if prob < prob_margin {
            prob = prob_margin;
        }
        if prob > 4096 - prob_margin {
            prob = 4096 - prob_margin;
        }

        prob as _
    }

    // update assumes prob has been called _once_ for the current bit _before_ update has been called!
    pub fn update(&mut self, bit: u32, is_in_code_section: bool) {
        let dis_model_context = &mut self.dis_model_contexts[self.dis_model_state as usize];

        // Update context models
        /*for i in 0..self.num_active_context_models {
            // Indirect model
            // Update indirect prob
            let indirect_prob_index = self.context_model_indirect_prob_indices[i];
            let mut indirect_prob = dis_model_context.indirect_probs[indirect_prob_index] as i32;
            indirect_prob += (((bit as i32) << 16) - indirect_prob) >> 6;
            dis_model_context.indirect_probs[indirect_prob_index] = indirect_prob as u16;

            // Update counts
            let hash = self.context_model_hashes[i];
            let entry = unsafe { &mut *self.hash_table.offset(hash as _) };
            let counts = entry.indirect_counts;
            let mut counts_zero = counts & 0x3f;
            let mut counts_one = (counts >> 6) & 0x3f;
            let last_bits = counts >> 12;

            if bit == 0 {
                if counts_zero < 63 {
                    counts_zero += 1;
                }

                if counts_one > 9 {
                    counts_one = 9;
                }
            } else {
                if counts_one < 63 {
                    counts_one += 1;
                }

                if counts_zero > 9 {
                    counts_zero = 9;
                }
            }

            entry.indirect_counts = ((last_bits << 13) | (bit << 12) | (counts_one << 6) | counts_zero) & 0xffff;

            // Stationary model
            let counts = entry.stationary_counts as i32;

            let mut prob = counts & 0x003fffff;
            let mut count = counts >> 22;

            let max = 1 << 22;
            let delta = max >> 12;
            prob += ((((bit as i32) << 22) - prob) << 9) / (count + delta);

            if count < 256 {
                count += 1;
            }

            entry.stationary_counts = ((count << 22) | prob) as u32;

            // Run model
            let mut count = entry.run_count as i32;
            let symbol = entry.run_symbol as u32;

            if bit != symbol as _ {
                count = 0;
            }

            if count < 1024 {
                count += 1;
            }

            entry.run_count = count as _;
            entry.run_symbol = bit as _;
        }

        // Update match models
        for i in 0..NUM_MATCH_MODELS {
            self.match_models[i].update_bit(bit);
        }

        // Update model weights
        let mut probs_ptr: *mut i16 = self.stage_2_probs.as_ptr() as *mut _;
        for i in 0..8 {
            let prediction = unsafe {
                let prediction = *probs_ptr;
                probs_ptr = probs_ptr.offset(1);
                prediction
            };

            train(
                &self.stage_1_probs,
                &mut dis_model_context.stage_1_weights
                    [(i * 256 + (self.stage_1_weight_contexts[i] as usize)) * NUM_MAX_MODEL_OUTPUTS / MIX_VECTOR_SIZE..],
                self.num_model_outputs / MIX_VECTOR_SIZE,
                bit as _,
                squash(prediction as _) as _,
            );
        }
        train(
            &self.stage_2_probs,
            &mut dis_model_context.stage_2_weights,
            8 / MIX_VECTOR_SIZE,
            bit as _,
            squash(self.stage_2_prob) as _,
        );*/

        for i in 0..NUM_APM_STAGES {
            dis_model_context.apm_stages[i].update(bit);
        }

        self.bit_history = (self.bit_history << 1) | (bit as u8);
        self.bit_index += 1;

        if self.bit_index == 8 {
            let byte = self.bit_history;

            self.histories[0].update(byte);

            /*if self.dis_model_state > 0 {
                self.histories[self.dis_model_state as usize].update(byte);
            }

            if is_in_code_section {
                self.dis_model.update(byte);
            }*/

            self.bit_history = 0;
            self.bit_index = 0;

            /*self.dis_model_state = if is_in_code_section { 1 + (self.dis_model.val()) } else { 0 };
            self.num_active_context_models = if is_in_code_section {
                NUM_MAX_ACTIVE_CONTEXT_MODELS
            } else {
                NUM_MIN_ACTIVE_CONTEXT_MODELS
            };
            self.num_model_outputs = if is_in_code_section {
                NUM_MAX_MODEL_OUTPUTS
            } else {
                NUM_MIN_MODEL_OUTPUTS
            };

            for i in 0..self.num_active_context_models {
                let history_index = if i < NUM_MIN_ACTIVE_CONTEXT_MODELS { 0 } else { self.dis_model_state };
                let mut hash = self.histories[history_index as usize].hash(BYTE_MASKS[i % NUM_BASE_CONTEXT_MODELS]);
                fnv::hash_byte(&mut hash, self.dis_model_state as _);
                fnv::hash_byte(&mut hash, 0x00); // Hash 0 byte to make room for bit history bits
                self.context_model_byte_hashes[i] = hash;
            }

            for i in 0..NUM_MATCH_MODELS {
                self.match_models[i].update_byte(&self.histories[0], ((1 << (i + 1)) - 1) as _);
            }*/
        }
    }
}

// return p = 1/(1 + exp(-d)), d scaled by 8 bits, p scaled by 12 bits
pub fn squash(d: i32) -> u32 {
    if d > 2047 {
        return 4095;
    }
    if d < -2047 {
        return 0;
    }
    let squash_tab = [
        1, 2, 3, 6, 10, 16, 27, 45, 73, 120, 194, 310, 488, 747, 1101, 1546, 2047, 2549, 2994, 3348, 3607, 3785, 3901, 3975, 4022, 4050, 4068, 4079,
        4085, 4089, 4092, 4093, 4094,
    ];
    let w = d & 0x7f;
    let d = (d >> 7) + 16;
    (squash_tab[d as usize] + (((squash_tab[(d + 1) as usize] - squash_tab[d as usize]) * w + 64) >> 7)) as u32
}

// Inverse of squash. d = ln(p/(1-p)), d scaled by 8 bits, p by 12 bits.
// d has range -2047 to 2047 representing -8 to 8.  p has range 0 to 4095.
fn stretch(p: u32, stretch_tab: &[i32]) -> i32 {
    stretch_tab[p as usize]
}

fn mix(probs: &[i16x8], weights: &[i16x8], count: usize) -> i32 {
    unsafe {
        let mut acc = i16x8::splat(0);
        // Vertical sums
        for i in 0..count {
            acc += probs[i].mul_hi(weights[i]);
        }
        // Horizontal sums
        acc = horizontal_pair_add(acc, acc);
        acc = horizontal_pair_add(acc, acc);
        acc = horizontal_pair_add(acc, acc);
        acc[0] as i32
    }
}

fn train(probs: &[i16x8], weights: &mut [i16x8], count: usize, bit: i32, current_prob: i32) {
    let prediction_error = ((bit << 12) - current_prob) * 7;
    if prediction_error < -32768 || prediction_error >= 32768 {
        panic!("Prediction error fail: {}", prediction_error);
    }
    let prediction_error = i16x8::splat(prediction_error as _);

    unsafe {
        for i in 0..count {
            let weight_adjusts = (((probs[i] << 1).mul_hi(prediction_error)) + i16x8::splat(1)) >> 1;
            weights[i] = weights[i].saturating_add(weight_adjusts);
        }
    }
}

trait SimdMulHi {
    fn mul_hi(self, other: Self) -> Self;
}

impl SimdMulHi for i16x8 {
    #[inline(always)]
    fn mul_hi(self, other: Self) -> Self {
        let multiplied: i32x8 = self.cast::<i32>() * other.cast::<i32>();
        (multiplied >> i32x8::splat(16)).cast::<i16>()
    }
}

fn horizontal_pair_add(a: i16x8, b: i16x8) -> i16x8 {
    let evens = simd_swizzle!(a, b, [0, 2, 4, 6, 8, 10, 12, 14]);
    let odds = simd_swizzle!(a, b, [1, 3, 5, 7, 9, 11, 13, 15]);

    evens + odds
}