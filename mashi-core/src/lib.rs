#![cfg_attr(not(feature = "std"), no_std)]

// The code in this file is heavily inspired and modified from the PAQ range of compressors.
// Go to http://mattmahoney.net/dc/ to learn more!

extern crate alloc;

use alloc::alloc::{alloc_zeroed, Layout};
use core::mem::MaybeUninit;

pub mod codec;

#[rustfmt::skip]
const STATE_TABLE: [[u8; 2]; 256] = [
    [  1,  2],[  3,  5],[  4,  6],[  7, 10],[  8, 12],[  9, 13],[ 11, 14], // 0
    [ 15, 19],[ 16, 23],[ 17, 24],[ 18, 25],[ 20, 27],[ 21, 28],[ 22, 29], // 7
    [ 26, 30],[ 31, 33],[ 32, 35],[ 32, 35],[ 32, 35],[ 32, 35],[ 34, 37], // 14
    [ 34, 37],[ 34, 37],[ 34, 37],[ 34, 37],[ 34, 37],[ 36, 39],[ 36, 39], // 21
    [ 36, 39],[ 36, 39],[ 38, 40],[ 41, 43],[ 42, 45],[ 42, 45],[ 44, 47], // 28
    [ 44, 47],[ 46, 49],[ 46, 49],[ 48, 51],[ 48, 51],[ 50, 52],[ 53, 43], // 35
    [ 54, 57],[ 54, 57],[ 56, 59],[ 56, 59],[ 58, 61],[ 58, 61],[ 60, 63], // 42
    [ 60, 63],[ 62, 65],[ 62, 65],[ 50, 66],[ 67, 55],[ 68, 57],[ 68, 57], // 49
    [ 70, 73],[ 70, 73],[ 72, 75],[ 72, 75],[ 74, 77],[ 74, 77],[ 76, 79], // 56
    [ 76, 79],[ 62, 81],[ 62, 81],[ 64, 82],[ 83, 69],[ 84, 71],[ 84, 71], // 63
    [ 86, 73],[ 86, 73],[ 44, 59],[ 44, 59],[ 58, 61],[ 58, 61],[ 60, 49], // 70
    [ 60, 49],[ 76, 89],[ 76, 89],[ 78, 91],[ 78, 91],[ 80, 92],[ 93, 69], // 77
    [ 94, 87],[ 94, 87],[ 96, 45],[ 96, 45],[ 48, 99],[ 48, 99],[ 88,101], // 84
    [ 88,101],[ 80,102],[103, 69],[104, 87],[104, 87],[106, 57],[106, 57], // 91
    [ 62,109],[ 62,109],[ 88,111],[ 88,111],[ 80,112],[113, 85],[114, 87], // 98
    [114, 87],[116, 57],[116, 57],[ 62,119],[ 62,119],[ 88,121],[ 88,121], // 105
    [ 90,122],[123, 85],[124, 97],[124, 97],[126, 57],[126, 57],[ 62,129], // 112
    [ 62,129],[ 98,131],[ 98,131],[ 90,132],[133, 85],[134, 97],[134, 97], // 119
    [136, 57],[136, 57],[ 62,139],[ 62,139],[ 98,141],[ 98,141],[ 90,142], // 126
    [143, 95],[144, 97],[144, 97],[ 68, 57],[ 68, 57],[ 62, 81],[ 62, 81], // 133
    [ 98,147],[ 98,147],[100,148],[149, 95],[150,107],[150,107],[108,151], // 140
    [108,151],[100,152],[153, 95],[154,107],[108,155],[100,156],[157, 95], // 147
    [158,107],[108,159],[100,160],[161,105],[162,107],[108,163],[110,164], // 154
    [165,105],[166,117],[118,167],[110,168],[169,105],[170,117],[118,171], // 161
    [110,172],[173,105],[174,117],[118,175],[110,176],[177,105],[178,117], // 168
    [118,179],[110,180],[181,115],[182,117],[118,183],[120,184],[185,115], // 175
    [186,127],[128,187],[120,188],[189,115],[190,127],[128,191],[120,192], // 182
    [193,115],[194,127],[128,195],[120,196],[197,115],[198,127],[128,199], // 189
    [120,200],[201,115],[202,127],[128,203],[120,204],[205,115],[206,127], // 196
    [128,207],[120,208],[209,125],[210,127],[128,211],[130,212],[213,125], // 203
    [214,137],[138,215],[130,216],[217,125],[218,137],[138,219],[130,220], // 210
    [221,125],[222,137],[138,223],[130,224],[225,125],[226,137],[138,227], // 217
    [130,228],[229,125],[230,137],[138,231],[130,232],[233,125],[234,137], // 224
    [138,235],[130,236],[237,125],[238,137],[138,239],[130,240],[241,125], // 231
    [242,137],[138,243],[130,244],[245,135],[246,137],[138,247],[140,248], // 238
    [249,135],[250, 69],[ 80,251],[140,252],[249,135],[250, 69],[ 80,251], // 245
    [140,252],[  0,  0],[  0,  0],[  0,  0]]; // 252

fn squash(mut d: i32) -> i32 {
    const SQUASH_TABLE: [i32; 33] = [
        1, 2, 3, 6, 10, 16, 27, 45, 73, 120, 194, 310, 488, 747, 1101, 1546, 2047, 2549, 2994,
        3348, 3607, 3785, 3901, 3975, 4022, 4050, 4068, 4079, 4085, 4089, 4092, 4093, 4094,
    ];
    if d > 2047 {
        return 4095;
    }
    if d < -2047 {
        return 0;
    }
    let w = d & 127;
    d = (d >> 7) + 16;
    (SQUASH_TABLE[d as usize] * (128 - w) + SQUASH_TABLE[(d + 1) as usize] * w + 64) >> 7
}

struct Stretch {
    stretch_table: [i16; 4096],
}

impl Stretch {
    fn new() -> Stretch {
        let mut s = Stretch {
            stretch_table: [0; 4096],
        };
        let mut pi = 0;
        for x in -2047..=2047 {
            let i = squash(x);
            for j in pi..=i {
                s.stretch_table[j as usize] = x as i16;
            }
            pi = i + 1;
        }
        s.stretch_table[4095] = 2047;
        s
    }
    fn stretch(&self, p: i32) -> i32 {
        debug_assert!(p < 4096);
        self.stretch_table[p as usize] as i32
    }
}

const MAX_MIXABLE: usize = NUM_HIGHER_ORDER + 1;
const MAX_WEIGHTS: usize = MAX_MIXABLE * 10;

pub struct Mixer {
    tx: [i32; MAX_MIXABLE],
    wx: [i32; MAX_MIXABLE * MAX_WEIGHTS],
    cxt: u32,
    nx: u32,
    pr: u32,
}

impl Mixer {
    pub fn new() -> Self {
        Self {
            tx: [0; MAX_MIXABLE],
            wx: [0; MAX_MIXABLE * MAX_WEIGHTS],
            cxt: 0,
            nx: 0,
            pr: 2048,
        }
    }

    pub fn update(&mut self, y: u32) {
        let err = (((y as i32) << 12) - self.pr as i32) * MAX_MIXABLE as i32;
        //assert!(err >= -32768 && err < 32768);
        train(
            &self.tx[0..],
            &mut self.wx[(self.cxt as usize * MAX_MIXABLE)..],
            MAX_MIXABLE,
            err,
        );
        self.nx = 0;
    }

    pub fn add(&mut self, x: i32) {
        debug_assert!(self.nx < MAX_MIXABLE as u32);
        self.tx[self.nx as usize] = x;
        self.nx += 1;
    }

    pub fn set(&mut self, cxt: u32) {
        debug_assert!(cxt < MAX_WEIGHTS as u32);
        self.cxt = cxt;
    }

    pub fn p(&mut self) -> u32 {
        self.pr = squash(
            dot_product(
                &self.tx[0..],
                &self.wx[(self.cxt as usize * MAX_MIXABLE)..],
                MAX_MIXABLE,
            ) >> 8,
        ) as u32;
        self.pr
    }
}

fn dot_product(t: &[i32], w: &[i32], n: usize) -> i32 {
    let mut sum = 0;
    for i in 0..n {
        sum += t[i] * w[i];
    }
    sum >> 8
}

fn train(t: &[i32], w: &mut [i32], n: usize, err: i32) {
    for i in 0..n {
        w[i] += t[i] * err + 0x8000 >> 16;
    }
}

struct Apm<'a> {
    state_map: StateMap<'a>,
    stretcher: Stretch,
}

impl<'a> Apm<'a> {
    fn new(n: usize) -> Apm<'a> {
        let state_map = StateMap::new(n * 24);
        for i in 0..state_map.t.len() {
            let p = ((i as isize % 24 * 2 + 1) * 4096) / 48 - 2048;
            state_map.t[i] = (((squash(p as i32)) as u32) << 20) + 6;
        }

        Self {
            state_map,
            stretcher: Stretch::new(),
        }
    }

    fn pp(&mut self, bit: u32, pr: u16, mut cx: u32) -> u16 {
        debug_assert!(pr < 4096);
        debug_assert!(cx < (self.state_map.t.len() / 24) as u32);
        self.state_map.update(bit, 255);
        let mut pr = ((self.stretcher.stretch(pr as i32) + 2048) * 23) as i32;
        let wt = (pr & 0xfff) as u32; // interpolation weight of next element
        cx = cx * 24 + ((pr as u32) >> 12);
        debug_assert!(cx < (self.state_map.t.len() - 1) as u32);
        pr = ((self.state_map.t[cx as usize] >> 13) * (0x1000 - wt)
            + (self.state_map.t[cx as usize + 1] >> 13) * wt
            >> 19) as i32;
        self.state_map.cxt = (cx + (wt >> 11)) as usize;
        pr as u16
    }
}
// -----------------------------------------------------------------

struct HashTable<'a> {
    t: &'a mut [u8],
    n: usize,
}

impl<'a> HashTable<'a> {
    pub fn new(n: usize) -> Self {
        let size = n + 16 * 4 + 64;
        let ptr = unsafe { alloc_zeroed(Layout::from_size_align_unchecked(size, 1)) };
        let t = unsafe { core::slice::from_raw_parts_mut(ptr, size) };

        Self { t, n }
    }

    pub fn get_offset_mut(&mut self, i: u32) -> usize {
        let i = i.wrapping_mul(111111111);
        let i = i << 16 | i >> 16;
        let i = i.wrapping_mul(222222222);
        let chk = (i >> 24) as u8;
        let mut i = (i.wrapping_mul(16) & self.n as u32 - 16) as usize;
        if self.t[i] == chk {
            i
        } else if self.t[i ^ 16] == chk {
            i ^ 16
        } else if self.t[i ^ 16 * 2] == chk {
            i ^ 16 * 2
        } else {
            if self.t[i + 1] > self.t[i + 1 ^ 16] || self.t[i + 1] > self.t[i + 1 ^ 16 * 2] {
                i ^= 16;
            }
            if self.t[i + 1] > self.t[i + 1 ^ 16 ^ 16 * 2] {
                i ^= 16 ^ 16 * 2
            }
            self.t[i] = chk;
            i
        }
    }
}

static mut STATEMAP_DT: [i32; 1024] = [0; 1024];

struct StateMap<'a> {
    cxt: usize,
    t: &'a mut [u32],
}

impl<'a> StateMap<'a> {
    fn new(n: usize) -> StateMap<'a> {
        let ptr = unsafe {
            alloc_zeroed(Layout::from_size_align_unchecked(
                n * core::mem::size_of::<u32>(),
                1,
            ))
        } as *mut u32;
        let t = unsafe { core::slice::from_raw_parts_mut(ptr, n) };
        for i in 0..n {
            // Initialize probabilities to 0.5 (= 1 << 31)
            t[i] = 1 << 31;
        }

        for i in 0..1024 {
            unsafe {
                STATEMAP_DT[i] = (16384 / (i + i + 3)) as i32;
            }
        }

        StateMap { cxt: 0, t }
    }

    fn p(&mut self, bit: u32, cx: usize) -> i32 {
        debug_assert!(bit == 0 || bit == 1);
        debug_assert!(cx < self.t.len());
        self.update(bit, 1023); // update prediction for previous context
        self.cxt = cx;
        (self.t[self.cxt] >> 20) as i32 // output prediction for new context
    }

    fn update(&mut self, bit: u32, limit: u32) {
        debug_assert!(bit == 0 || bit == 1);
        let p = &mut self.t[self.cxt];
        let mut p0 = *p;

        let count: u32 = (p0 & 1023) as u32;
        let prediction: i32 = (p0 >> 10) as i32;

        #[allow(overflowing_literals)]
        const MASK: i32 = 0xFFFFFc00;

        if count < limit {
            p0 += 1;
        } else {
            p0 = (p0 & MASK as u32) | limit;
        }

        *p = p0.wrapping_add(
            (((((bit << 22) as i32 - prediction) >> 3) * unsafe { STATEMAP_DT[count as usize] })
                & MASK) as u32,
        );
    }
}

const NUM_HIGHER_ORDER: usize = 5 + MASKS.len();
const MASKS: [u32; 10] = [
    0xff00ff00, 0x00ff00ff, 0xffff0000, 0x0000ffff, 0xff0000ff, 0x00ffff00, 0xff000000, 0x00ff0000,
    0x0000ff00, 0x000000ff,
];

pub struct Predictor<'a> {
    t0: &'a mut [u8], // order 1 context --> state
    t: HashTable<'a>,
    c0: u32, // last 0-7 bits with leading 1
    c4: u32, // last 4 bytes
    bit_count: u8,
    sm0: StateMap<'a>,
    cp0: usize, // pointer to bit history
    sms: [StateMap<'a>; NUM_HIGHER_ORDER],
    cps: [usize; NUM_HIGHER_ORDER], // pointer to bit history
    a1: Apm<'a>,
    a2: Apm<'a>,
    a3: Apm<'a>,
    h0: u32,
    hs: [u32; NUM_HIGHER_ORDER],
    m: Mixer,
    pr: u16,
    stretch: Stretch,
}

impl<'a> Predictor<'a> {
    pub fn new() -> Predictor<'a> {
        let ptr = unsafe { alloc_zeroed(Layout::from_size_align_unchecked(65536, 1)) };
        let t0 = unsafe { core::slice::from_raw_parts_mut(ptr, 65536) };

        let sms = unsafe {
            let mut arr: [MaybeUninit<StateMap<'a>>; NUM_HIGHER_ORDER] =
                core::mem::MaybeUninit::uninit().assume_init();
            for item in &mut arr {
                core::ptr::write(item.as_mut_ptr(), StateMap::new(256));
            }
            core::mem::transmute(arr)
        };

        const MEM: usize = 9; // use maximum amount of memory
        Predictor {
            t0,
            t: HashTable::new((1 << (MEM + 20)) * 2),
            c0: 1,
            c4: 0,
            sm0: StateMap::new(256),
            cp0: 0,
            bit_count: 0,
            sms,
            cps: [0; NUM_HIGHER_ORDER],
            a1: Apm::new(0x100),
            a2: Apm::new(0x10000),
            a3: Apm::new(0x10000),
            h0: 0,
            hs: [0; NUM_HIGHER_ORDER],
            m: Mixer::new(),
            pr: 2048,
            stretch: Stretch::new(),
        }
    }
    fn p(&mut self) -> u16 {
        self.pr as u16
    }

    fn update(&mut self, y: u32) {
        // update model
        self.t0[self.cp0] = STATE_TABLE[self.t0[self.cp0] as usize][y as usize];
        for i in 0..NUM_HIGHER_ORDER {
            self.t.t[self.cps[i]] = STATE_TABLE[self.t.t[self.cps[i]] as usize][y as usize];
        }
        self.m.update(y);

        // update context
        self.bit_count += 1;
        self.c0 += self.c0 + y;
        if self.c0 >= 256 {
            self.c0 -= 256;
            self.c4 = self.c4 << 8 | self.c0;
            self.h0 = self.c0 << 8; // order 1
            self.hs[0] = (self.c4 & 0xffff) << 5 | 0x57000000;
            self.hs[1] = (self.c4 << 8).wrapping_mul(235);
            self.hs[2] = self.c4.wrapping_mul(225);
            self.hs[3] = self.hs[3]
                .wrapping_mul(179 << 5)
                .wrapping_add(self.c0.wrapping_mul(147))
                & 0x3fffffff;

            // self.hs[4] is always 0

            // 4 byte context
            for i in 0..MASKS.len() {
                self.hs[i + 5] = self.c4 & MASKS[i];
            }

            for i in 0..NUM_HIGHER_ORDER {
                self.cps[i] = self.t.get_offset_mut(self.hs[i]) + 1;
            }
            self.c0 = 1;
            self.bit_count = 0;
        }
        if self.bit_count == 4 {
            for i in 0..NUM_HIGHER_ORDER {
                self.cps[i] = self.t.get_offset_mut(self.hs[i].wrapping_add(self.c0)) + 1;
            }
        } else if self.bit_count > 0 {
            let j = y + 1 << (self.bit_count & 3) - 1;
            for i in 0..NUM_HIGHER_ORDER {
                self.cps[i] += j as usize;
            }
        }
        self.cp0 = (self.h0 + self.c0) as usize;

        // predict
        let mut order = 0;
        for i in 0..NUM_HIGHER_ORDER {
            if self.t.t[self.cps[i]] != 0 {
                order += 1
            }
        }

        self.m.add(
            self.stretch
                .stretch(self.sm0.p(y, self.t0[self.cp0] as usize)),
        );
        for i in 0..NUM_HIGHER_ORDER {
            self.m.add(
                self.stretch
                    .stretch(self.sms[i].p(y, self.t.t[self.cps[i]] as usize)),
            );
        }
        self.m.set(order + 10 * (self.h0 >> 13));
        let mut pr = self.m.p();
        pr = pr + 3 * self.a1.pp(y, pr as u16, self.c0) as u32 >> 2;
        pr = pr + 3 * self.a2.pp(y, pr as u16, self.c0 ^ self.h0) as u32 >> 2;
        pr = pr + 3 * self.a3.pp(y, pr as u16, self.c4 & 0xffff) as u32 >> 2;
        debug_assert!(pr < 4096);
        self.pr = pr as u16;
    }
}
// -----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::codec::{Decoder, Encoder};

    #[test]
    fn can_roundtrip() {
        let input: &[u8] = include_bytes!("../../tests/test.wasm");

        let mut encoder = Encoder::new();
        let compressed = encoder.encode(input);
        println!("compressed size: {:?}", compressed.len());

        let mut decoder = Decoder::new();
        let output = decoder.decode(&compressed);
        assert_eq!(input, output);
    }
}
