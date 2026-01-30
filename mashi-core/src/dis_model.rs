use alloc::collections::VecDeque;
use core::fmt::{Display, Formatter};

pub const NUM_DIS_MODEL_STATES: usize = core::mem::variant_count::<DisModelState>();

#[repr(u8)]
#[derive(Clone, Debug, Default)]
pub enum DisModelState {
    #[default]
    Opcode,
    PrefixedOpcode,
    Leb,
    FuncLength,
    LocalTypeCount,
    LocalCount,
    LocalType,
    ConstI32,
    ConstI64,
    ConstF32,
    ConstF64,
    DataIdx,
    MemIdx,
    LocalIdx,
    GlobalIdx,
    MemArgAlign,
    MemArgX,
    MemArgOffset,
    FuncIdx,
    BlockType,
    BrTable,
    LabelIdx,
    LaneIdx,
    VectorByte,
    MiscLeb, // LEBs that we don't want to specifically group
}

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

pub struct DisModel {
    depth: u8,
    queue: VecDeque<DisModelState>,
    cur_leb: Leb,
    cur_float_bytes_left: usize,
    opcode: u8,
}

impl DisModel {
    pub fn new() -> Self {
        let mut state = Self {
            depth: 0,
            queue: VecDeque::from([
                // the total count of functions in this module
                DisModelState::Leb,

                // the header of the first function
                DisModelState::FuncLength,
                DisModelState::LocalTypeCount
            ]),
            cur_leb: Leb::default(),
            cur_float_bytes_left: 0,
            opcode: 0,
        };
        state
    }

    pub fn val(&self) -> u32 {
        self.queue.front().unwrap_or(&DisModelState::default()).clone() as u32
    }

    pub fn update(&mut self, byte: u8) {
        let state = self.queue.pop_front().unwrap_or(DisModelState::default());

        //std::println!("{:?} | self.opcode: {:0X} | incoming byte: {:0X}", state, self.opcode, byte);

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
                        self.queue.push_back(DisModelState::BlockType);
                    }

                    // end of block
                    0xb => {
                        if self.depth > 0 {
                            self.depth -= 1;
                        } else {
                            // end of function, so prepare for the next one
                            self.queue.extend([DisModelState::FuncLength, DisModelState::LocalTypeCount]);
                        }
                    }

                    0xc | 0xd | 0xd5 | 0xd6 => {
                        self.queue.push_back(DisModelState::LabelIdx);
                    }

                    0xe => {
                        self.queue.push_back(DisModelState::BrTable);
                    }

                    0x10 => {
                        self.queue.push_back(DisModelState::FuncIdx);
                    }
                    0x11 => {
                        self.queue.extend([const { DisModelState::MiscLeb }; 2]);
                    }

                    0x20 | 0x21 | 0x22 => {
                        self.queue.push_back(DisModelState::LocalIdx);
                    }
                    0x23 | 0x24 => {
                        self.queue.push_back(DisModelState::GlobalIdx);
                    }
                    0x28..=0x3e => {
                        self.queue.push_back(DisModelState::MemArgAlign);
                    }
                    0x3f..=0x40 => {
                        self.queue.push_back(DisModelState::MemIdx);
                    }

                    0x41 => self.queue.push_back(DisModelState::ConstI32),
                    0x42 => self.queue.push_back(DisModelState::ConstI64),
                    0x43 => {
                        self.queue.push_back(DisModelState::ConstF32);
                        self.cur_float_bytes_left = 4;
                    }
                    0x44 => {
                        self.queue.push_back(DisModelState::ConstF64);
                        self.cur_float_bytes_left = 8;
                    }

                    0xd0 => self.queue.push_back(DisModelState::MiscLeb),
                    0xd2 | 0xd5 | 0xd6 => self.queue.push_back(DisModelState::FuncIdx),

                    0xfc | 0xfd | 0xfe => {
                        self.queue.push_back(DisModelState::PrefixedOpcode);
                    }

                    // This opcode doesn't need any special handling
                    _ => ()
                }
            }
            DisModelState::PrefixedOpcode => {
                if let Some(opcode) = self.update_leb(state, byte) {
                    match self.opcode /* prefix */ {
                        0xfc => {
                            match opcode {
                                0x8 | 0xa => self.queue.extend([DisModelState::DataIdx, DisModelState::MemIdx]),
                                0x9 | 0xb | 0xd | 0x12 | 0xf..=0x11 => self.queue.push_back(DisModelState::MemIdx),
                                0xc | 0xe => self.queue.extend([const { DisModelState::MiscLeb }; 2]),
                                _ => ()
                            }
                        }
                        0xfd => {
                            match opcode {
                                0..=11 | 92 | 93 => self.queue.extend([DisModelState::MemArgAlign]),
                                12 | 13 => self.queue.extend([const { DisModelState::VectorByte }; 16]),
                                21..=34 | 84..=91 => self.queue.extend([DisModelState::MemArgAlign, DisModelState::LaneIdx]),
                                _ => ()
                            }
                        }
                        0xfe => {
                            match opcode {
                                0x3 => self.queue.push_back(DisModelState::VectorByte), // fence parameter, should probably go elsewhere but meh
                                0x0..=0x2 | 0x10..=0x4e => self.queue.extend([DisModelState::MemArgAlign]),
                                _ => ()
                            }
                        }
                        _ => unimplemented!("Unsupported opcode prefix: {:0X}", byte)
                    }
                }
            }
            DisModelState::LocalTypeCount => {
                if let Some(num_local_types) = self.update_leb(state, byte) {
                    for _ in 0..num_local_types {
                        self.queue.extend([DisModelState::LocalCount, DisModelState::LocalType]);
                    }
                    self.queue.push_back(DisModelState::Opcode);
                }
            }
            DisModelState::LocalType | DisModelState::VectorByte => {
                // just ignore this byte
            }
            DisModelState::MemArgAlign => {
                if let Some(offset) = self.update_leb(state, byte) {
                    self.queue.push_front(DisModelState::MemArgOffset);
                    if offset >= 64 {
                        self.queue.push_front(DisModelState::MemArgX);
                    }
                }
            }
            DisModelState::BrTable => {
                if let Some(item_count) = self.update_leb(state, byte) {
                    // br_tables always contain one more element
                    for i in 0..item_count {
                        self.queue.push_back(DisModelState::LabelIdx);
                    }
                    self.queue.push_back(DisModelState::LabelIdx);
                }
            }
            DisModelState::ConstF32 | DisModelState::ConstF64 => {
                // Floats are stored as IEEE floats, so just ignore the bytes
                self.cur_float_bytes_left -= 1;
                if self.cur_float_bytes_left > 0 {
                    // not yet done, remain in this state
                    self.queue.push_front(state);
                }
            }
            _ => {
                // In all other cases we're consuming some form of LEB
                let _ = self.update_leb(state, byte);
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
            self.queue.push_front(state);
            None
        }
    }
}