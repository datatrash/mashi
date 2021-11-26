use crate::Predictor;
use alloc::alloc::{alloc_zeroed, Layout};

// This arithmetic coder is based on the one in Crinkler, see https://github.com/runestubbe/Crinkler for details!

#[cfg(feature = "encoder")]
pub struct Encoder<'a> {
    predictor: Predictor<'a>,
    dest_bit: i32,
    interval_size: u32,
    interval_min: u32,
    output: alloc::vec::Vec<u8>,
}

#[cfg(feature = "encoder")]
impl<'a> Encoder<'a> {
    pub fn new() -> Self {
        Self {
            predictor: Predictor::new(),
            dest_bit: -1,
            interval_size: 0x80000000,
            interval_min: 0,
            output: alloc::vec![],
        }
    }

    pub fn encode(&mut self, input: &[u8]) -> alloc::vec::Vec<u8> {
        for byte in input {
            for i in (0..8).rev() {
                let y = (byte >> i) & 1;
                self.encode_bit(y as u32);
            }
        }

        if self.interval_min != 0 {
            let should_carry = self.interval_min.wrapping_add(self.interval_size);
            if should_carry == 0 || should_carry >= self.interval_min {
                // Not carry
                self.dest_bit += 1;
            }
            self.put_bit();
        }

        let mut with_header = alloc::vec![];
        with_header.extend((input.len() as u32).to_le_bytes());
        with_header.extend(self.output.clone());
        with_header
    }

    fn encode_bit(&mut self, y: u32) {
        let mut prob = 4095 - self.predictor.p();
        if prob == 0 {
            prob = 1
        }
        //println!("Encoding bit {} with probability: {:?}", bit, prob);
        debug_assert!(prob > 0 && prob < 4096);
        let threshold = ((self.interval_size as u64 * prob as u64) >> 12) as u32;
        self.predictor.update(y);
        if y != 0 {
            let old_interval_min = self.interval_min;
            self.interval_min = self.interval_min.wrapping_add(threshold);
            if self.interval_min < old_interval_min {
                // Carry
                self.put_bit();
            }

            self.interval_size -= threshold;
        } else {
            self.interval_size = threshold;
        }

        while self.interval_size < 0x80000000 {
            self.dest_bit += 1;

            if self.interval_min & 0x80000000 != 0 {
                self.put_bit();
            }
            self.interval_min <<= 1;
            self.interval_size <<= 1;
        }
    }

    fn put_bit(&mut self) {
        let mut dest_bit = self.dest_bit;
        loop {
            dest_bit -= 1;
            if dest_bit < 0 {
                return;
            }
            let msk = 1u8 << (dest_bit & 7);

            let dest_byte = (dest_bit >> 3) as usize;
            if dest_byte >= self.output.len() {
                self.output.resize(dest_byte + 1, 0);
            }
            let v = self.output[dest_byte];
            self.output[dest_byte] = v ^ msk;

            if v & msk == 0 {
                return;
            }
        }
    }
}

pub struct Decoder<'a> {
    predictor: Predictor<'a>,
    input_pos: usize,
    input_bit: usize,
    interval_size: u32,
    data: u32,
}

impl<'a> Decoder<'a> {
    pub fn new() -> Self {
        Self {
            predictor: Predictor::new(),
            input_pos: 0,
            input_bit: 0,
            interval_size: 1,
            data: 0,
        }
    }

    pub fn decode(&mut self, input: &[u8]) -> &[u8] {
        let expected_len = unsafe { *(input.as_ptr() as *const u32) } as usize;
        let input = &input[4..];

        let ptr = unsafe { alloc_zeroed(Layout::from_size_align_unchecked(expected_len, 1)) };
        let output = unsafe { core::slice::from_raw_parts_mut(ptr, expected_len) };

        let mut pos = 0;
        while pos != expected_len {
            let mut byte = 0;
            for _ in 0..8 {
                byte += byte + self.decode_bit(input);
            }
            output[pos] = byte as u8;
            pos += 1;
        }

        output
    }

    fn decode_bit(&mut self, input: &[u8]) -> u32 {
        while self.interval_size < 0x80000000 {
            self.data <<= 1u32;
            if self.next_bit(input) {
                self.data += 1;
            }
            self.interval_size <<= 1u32;
        }

        let mut prob = 4095 - self.predictor.p();
        if prob == 0 {
            prob = 1
        }

        let threshold = ((self.interval_size as u64 * prob as u64) >> 12) as u32;

        let bit = if self.data >= threshold {
            self.data -= threshold;
            self.interval_size -= threshold;
            1
        } else {
            self.interval_size = threshold;
            0
        };
        //println!("Decoded bit {} with probability: {:?}", bit, prob);

        self.predictor.update(bit);
        bit
    }

    fn next_bit(&mut self, input: &[u8]) -> bool {
        if self.input_pos >= input.len() {
            return false;
        }
        let bit = (input[self.input_pos] >> self.input_bit) & 1 == 1;
        self.input_bit += 1;
        if self.input_bit == 8 {
            self.input_bit = 0;
            self.input_pos += 1;
        }
        bit
    }
}
