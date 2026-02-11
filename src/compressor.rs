use crate::model::*;

const BLOCK_SIZE_SHIFT: usize = 13; // BLOCK_SIZE = 8kb
const BLOCK_SIZE: usize = 1 << BLOCK_SIZE_SHIFT;
const BLOCK_MASK: usize = BLOCK_SIZE - 1;

struct RangeEncoderState {
    output: Vec<u8>,

    low: u64,
    range: u32,

    cache: u32,
    cache_size: u32,

    is_first_byte: bool,

    bits_per_byte: Vec<f32>,
}

pub fn compress<F>(js_input: &[u8], wasm_input: &[u8], mut f: F) -> (Vec<u8>, Vec<f32>)
where
    F: FnMut(usize),
{
    use wasmparser::Payload::CodeSectionStart;
    use wasmparser::Parser;
    let mut code_section = 0u64..0u64;
    if !wasm_input.is_empty() {
        let parser = Parser::new(0);
        for payload in parser.parse_all(&wasm_input) {
            match payload.unwrap() {
                CodeSectionStart { range, .. } => {
                    code_section.start = range.start as u64;
                    code_section.end = range.end as u64;
                }
                _ => ()
            }
        }
        
        if code_section.start == 0 {
            println!("Warning: the provided WASM binary doesn't seem to have a code section");
        }
    }

    let mut input: Vec<u8> = vec![];
    input.extend(js_input);
    input.extend(wasm_input);
    code_section.start += js_input.len() as u64;
    code_section.end += js_input.len() as u64;

    let mut model = Model::new();
    let mut state = RangeEncoderState {
        output: Vec::new(),

        low: 0,
        range: 0xffffffff,

        cache: 0,
        cache_size: 0,

        is_first_byte: true,

        bits_per_byte: Vec::with_capacity(input.len()),
    };
    state.output.extend((code_section.start as u32).to_le_bytes());
    state.output.extend((code_section.end as u32).to_le_bytes());

    // leave output sizes empty for now
    state.output.extend(0u32.to_le_bytes());
    state.output.extend(0u32.to_le_bytes());

    let mut marker_bit_prob = 2048;

    f(0);

    let input_len = input.len();
    let mut byte_index = 0;
    while byte_index < input_len {
        let is_in_code_section = (byte_index as u64) >= code_section.start && (byte_index as u64) < code_section.end;

        // Each BLOCK_SIZE, we check if the next BLOCK_SIZE bytes are all 0x00 or not.
        //  If they are, we output a 0 marker bit, and nothing else.
        //  Otherwise, we output a 1 marker bit, and compress normally.
        //  This helps us skip over large portions of useless data common between sections and in (un)inialized data
        //  (some executables contain several megabytes' worth of such blocks!)
        // todo sag: check if we still need this for WASM
        if (byte_index & BLOCK_MASK) == 0 {
            let mut is_block_empty = input_len - byte_index >= BLOCK_SIZE;

            if is_block_empty {
                for i in 0..BLOCK_SIZE {
                    let byte_index = byte_index + i;
                    let byte = input[byte_index];
                    if byte != 0x00 {
                        is_block_empty = false;
                        break;
                    }
                }
            }

            let marker_bit = if is_block_empty { 0 } else { 1 };
            let marker_bit_bits = -((if marker_bit != 0 { marker_bit_prob } else { 4096 - marker_bit_prob } as f64) / 4096.0).log2();
            range_encode_bit(&mut state, marker_bit_prob, marker_bit);

            marker_bit_prob = (marker_bit_prob + if is_block_empty { 1 } else { 4095 }) >> 1;

            if is_block_empty {
                byte_index += BLOCK_SIZE;
                state
                    .bits_per_byte
                    .extend(&vec![(marker_bit_bits / ((BLOCK_SIZE * 8) as f64)) as f32; BLOCK_SIZE]);

                f(byte_index);

                continue;
            }
        }

        let byte = input[byte_index];
        let mut bits = 0.0;

        for bit_index in 0..8 {
            let bit = ((byte as u32) >> (7 - bit_index)) & 0x01;
            let prob = model.prob();
            bits += -((if bit != 0 { prob } else { 4096 - prob } as f64) / 4096.0).log2();
            range_encode_bit(&mut state, prob, bit);

            model.update(bit, is_in_code_section);
        }

        state.bits_per_byte.push(bits as _);

        byte_index += 1;

        if (byte_index & 0x03ff) == 0 {
            f(byte_index);
        }
    }

    for _ in 0..5 {
        range_encode_shift_low(&mut state);
    }

    f(input_len);

    state.output.as_mut_slice()[8..12].copy_from_slice(&(js_input.len() as u32).to_le_bytes());
    state.output.as_mut_slice()[12..16].copy_from_slice(&(wasm_input.len() as u32).to_le_bytes());

    (state.output, state.bits_per_byte)
}

fn range_encode_bit(state: &mut RangeEncoderState, prob: u32, bit: u32) {
    let new_bound = (state.range >> 12) * prob;
    if bit == 1 {
        state.range = new_bound;
    } else {
        state.low += new_bound as u64;
        state.range -= new_bound;
    }

    while state.range < 0x01000000 {
        state.range <<= 8;
        range_encode_shift_low(state);
    }
}

fn range_encode_shift_low(state: &mut RangeEncoderState) {
    let carry = (state.low >> 32) as u32;
    if state.low < 0xff000000 || carry == 1 {
        if !state.is_first_byte {
            state.output.push((state.cache + carry) as u8);
        } else {
            state.is_first_byte = false;
        }

        while state.cache_size > 0 {
            state.output.push((0xff + carry) as u8);
            state.cache_size -= 1;
        }

        state.cache = ((state.low >> 24) & 0xff) as u32;
    } else {
        state.cache_size += 1;
    }

    state.low = (state.low << 8) & 0xffffffff;
}

struct RangeDecoderState<'a, I: Iterator<Item=&'a u8>> {
    input: I,

    code: u32,
    range: u32,
}

pub fn decompress<F>(input: &[u8], mut f: F) -> (Vec<u8>, Model)
where
    F: FnMut(usize),
{
    let mut arr = [0u8; 4];
    arr.copy_from_slice(&input[0..4]);
    let code_section_start = u32::from_le_bytes(arr) as u64;
    arr.copy_from_slice(&input[4..8]);
    let code_section_end = u32::from_le_bytes(arr) as u64;
    arr.copy_from_slice(&input[8..12]);
    let js_output_size = u32::from_le_bytes(arr) as usize;
    arr.copy_from_slice(&input[12..16]);
    let wasm_output_size = u32::from_le_bytes(arr) as usize;
    let code_section = code_section_start..code_section_end;

    let output_size = js_output_size + wasm_output_size;

    let input = &input[16..];

    let mut model = Model::new();
    let mut state = RangeDecoderState {
        input: input.into_iter(),

        code: 0,
        range: 0xffffffff,
    };

    let mut output = Vec::new();

    for _ in 0..4 {
        state.code <<= 8;
        state.code |= *state.input.next().unwrap() as u32;
    }

    let mut marker_bit_prob = 2048;

    f(0);

    let mut byte_index = 0;
    while byte_index < output_size {
        if (byte_index & BLOCK_MASK) == 0 {
            let marker_bit = range_decode_bit(&mut state, marker_bit_prob);

            marker_bit_prob = (marker_bit_prob + if marker_bit == 0 { 1 } else { 4095 }) >> 1;

            if marker_bit == 0 {
                for _ in 0..BLOCK_SIZE {
                    output.push(0x00);
                }

                byte_index += BLOCK_SIZE;

                f(byte_index);

                continue;
            }
        }

        let mut byte: u8 = 0;

        let is_in_code_section = (byte_index as u64) >= code_section.start && (byte_index as u64) < code_section.end;
        for _ in 0..8 {
            let bit = range_decode_bit(&mut state, model.prob());

            model.update(bit, is_in_code_section);

            byte = (byte << 1) | (bit as u8);
        }

        output.push(byte);

        byte_index += 1;

        if (byte_index & 0x03ff) == 0 {
            f(byte_index);
        }
    }

    f(output_size);

    (output, model)
}

fn range_decode_bit<'a, I: Iterator<Item=&'a u8>>(state: &mut RangeDecoderState<'a, I>, prob: u32) -> u32 {
    let bound = (state.range >> 12) * prob;
    let bit = if state.code < bound {
        state.range = bound;
        1
    } else {
        state.code -= bound;
        state.range -= bound;
        0
    };

    while state.range < 0x01000000 {
        state.code = (state.code << 8) | *state.input.next().unwrap() as u32;
        state.range <<= 8;
    }

    bit
}