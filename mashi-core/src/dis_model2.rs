use crate::dis_model::DisModelState;
use core::fmt::{Display, Formatter};
use std::mem;
use std::vec::Vec;

#[derive(Clone, Debug, Default)]
struct Leb {
    val: u64,
    shift: u64,
    complete: bool,
}

impl Leb {
    /// Returns 'true' if the value has been completed
    pub fn update(&mut self, byte: u8) -> bool {
        self.val += ((byte & 0x7f) as u64) << self.shift;
        self.shift += 7;
        self.complete = byte & 0x80 != 0x80;
        self.complete
    }
}

impl Display for Leb {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.val)
    }
}

pub const QUEUE_LEN: usize = 16384;

pub struct DisModel2 {
    depth: u8,
    queue: [DisModelState; QUEUE_LEN],
    read_pos: usize,
    write_pos: usize,
    cur_leb: Leb,
    cur_float_bytes_left: usize,
    opcode: u8,
}

impl DisModel2 {
    pub fn new() -> Self {
        let mut state = Self {
            depth: 0,
            queue: unsafe { mem::zeroed() },
            read_pos: 0,
            write_pos: 3,
            cur_leb: Leb::default(),
            cur_float_bytes_left: 0,
            opcode: 0,
        };

        // the total count of functions in this module
        state.queue[0] = DisModelState::Leb;

        // the header of the first function
        state.queue[1] = DisModelState::FuncLength;
        state.queue[2] = DisModelState::LocalTypeCount;

        state
    }

    pub fn val(&self) -> u32 {
        self.queue[self.read_pos] as u32
    }

    fn write(&mut self, state: DisModelState) {
        self.queue[self.write_pos] = state;
        self.write_pos += 1;
        self.write_pos %= QUEUE_LEN;
    }

    pub fn update(&mut self, byte: u8) {
        let state = self.queue[self.read_pos];
        let mut should_remain = false;

        log::info!("{:?} | self.opcode: {:0X} | incoming byte: {:0X}            [m2] (r: {}, w: {})", state, self.opcode, byte, self.read_pos, self.write_pos);

        // Instruction handling is based on:
        // https://webassembly.github.io/spec/core/appendix/index-instructions.html
        // and
        // https://github.com/bytecodealliance/wasm-tools/blob/037cce497699bdc178906a3eedd6c63a31a44523/crates/wast/src/core/expr.rs
        //
        // Opcodes that don't require any arguments (or other special casing) are ignored, as are opcodes that are deprecated.
        // This logic may contain bugs, which causes the rest of the input to be handled incorrectly. Feel free to report with a test-case.
        match state {
            DisModelState::Opcode => {
                self.opcode = byte;
                match byte {
                    // block/loop/if
                    0x2 | 0x3 | 0x4 => {
                        self.depth += 1;
                        self.write(DisModelState::BlockType);
                    }

                    // end of block
                    0xb => {
                        if self.depth > 0 {
                            self.depth -= 1;
                        } else {
                            // end of function, so prepare for the next one
                            self.write(DisModelState::FuncLength);
                            self.write(DisModelState::LocalTypeCount);
                        }
                    }

                    0xc | 0xd | 0xd5 | 0xd6 => {
                        self.write(DisModelState::LabelIdx);
                    }

                    0xe => {
                        self.write(DisModelState::BrTable);
                    }

                    0x10 => {
                        self.write(DisModelState::FuncIdx);
                    }
                    0x11 => {
                        self.write(DisModelState::MiscLeb);
                        self.write(DisModelState::MiscLeb);
                    }

                    0x20 | 0x21 | 0x22 => {
                        self.write(DisModelState::LocalIdx);
                    }
                    0x23 | 0x24 => {
                        self.write(DisModelState::GlobalIdx);
                    }
                    0x28..=0x3e => {
                        self.write(DisModelState::MemArgAlign);
                    }
                    0x3f..=0x40 => {
                        self.write(DisModelState::MemIdx);
                    }

                    0x41 => self.write(DisModelState::ConstI32),
                    0x42 => self.write(DisModelState::ConstI64),
                    0x43 => {
                        self.write(DisModelState::ConstF32);
                        self.cur_float_bytes_left = 4;
                    }
                    0x44 => {
                        self.write(DisModelState::ConstF64);
                        self.cur_float_bytes_left = 8;
                    }

                    0xd0 => self.write(DisModelState::MiscLeb),
                    0xd2 | 0xd5 | 0xd6 => self.write(DisModelState::FuncIdx),

                    0xfc | 0xfd | 0xfe => {
                        self.write(DisModelState::PrefixedOpcode);
                    }

                    // This opcode doesn't need any special handling
                    _ => (),
                }
            }
            DisModelState::PrefixedOpcode => {
                if let Some(opcode) = self.update_leb(state, byte) {
                    match self.opcode /* prefix */ {
                        0xfc => {
                            match opcode {
                                0x8 | 0xa => {
                                    self.write(DisModelState::DataIdx);
                                    self.write(DisModelState::MemIdx);
                                }
                                0x9 | 0xb | 0xd | 0x12 | 0xf..=0x11 => self.write(DisModelState::MemIdx),
                                0xc | 0xe => {
                                    self.write(DisModelState::MiscLeb);
                                    self.write(DisModelState::MiscLeb);
                                }
                                _ => (),
                            }
                        }
                        0xfd => {
                            match opcode {
                                0..=11 | 92 | 93 => self.write(DisModelState::MemArgAlign),
                                12 | 13 => {
                                    for _ in 0..16 {
                                        self.write(DisModelState::VectorByte);
                                    }
                                }
                                21..=34 | 84..=91 => {
                                    self.write(DisModelState::MemArgAlign);
                                    self.write(DisModelState::LaneIdx);
                                }
                                _ => (),
                            }
                        }
                        0xfe => {
                            match opcode {
                                0x3 => self.write(DisModelState::VectorByte), // fence parameter, should probably go elsewhere but meh
                                0x0..=0x2 | 0x10..=0x4e => self.write(DisModelState::MemArgAlign),
                                _ => (),
                            }
                        }
                        _ => unimplemented!("Unsupported opcode prefix: {:0X}", byte)
                    }
                }
            }
            DisModelState::LocalTypeCount => {
                if let Some(num_local_types) = self.update_leb(state, byte) {
                    for _ in 0..num_local_types {
                        self.write(DisModelState::LocalCount);
                        self.write(DisModelState::LocalType);
                    }
                }
            }
            DisModelState::LocalType | DisModelState::VectorByte => {
                // just ignore this byte
            }
            DisModelState::MemArgAlign => {
                if let Some(offset) = self.update_leb(state, byte) {
                    if offset >= 64 {
                        self.write(DisModelState::MemArgX);
                    }
                    self.write(DisModelState::MemArgOffset);
                }
            }
            DisModelState::BrTable => {
                if let Some(item_count) = self.update_leb(state, byte) {
                    // br_tables always contain one more element
                    for i in 0..item_count {
                        self.write(DisModelState::LabelIdx);
                    }
                    self.write(DisModelState::LabelIdx);
                }
            }
            DisModelState::ConstF32 | DisModelState::ConstF64 => {
                // Floats are stored as IEEE floats, so just ignore the bytes
                self.cur_float_bytes_left -= 1;
                if self.cur_float_bytes_left > 0 {
                    // not yet done, remain in this state
                    should_remain = true;
                }
            }
            _ => {
                // In all other cases we're consuming some form of LEB
                if self.update_leb(state, byte).is_none() {
                    should_remain = true;
                }
            }
        }

        if !should_remain {
            self.read_pos += 1;
            self.read_pos %= QUEUE_LEN;

            // We've processed everything in the queue, so let's just fall back to reading another opcode
            if self.read_pos == self.write_pos {
                self.read_pos = 0;
                self.write_pos = 1;
                self.queue[0] = DisModelState::Opcode;
            }
        }
    }

    fn update_leb(&mut self, state: DisModelState, byte: u8) -> Option<u64> {
        if self.cur_leb.update(byte) {
            // done! move to the next state on the next byte
            let val = self.cur_leb.val;
            self.cur_leb = Leb::default();
            Some(val)
        } else {
            // not yet done, remain in this state
            None
        }
    }
}