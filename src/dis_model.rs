use std::mem;

pub const NUM_DIS_MODEL_STATES: usize = mem::variant_count::<DisModelState>();

#[repr(u8)]
#[derive(Copy, Clone, Debug, Default)]
pub enum DisModelState {
    #[default]
    Opcode = 0,
    PrefixedOpcode = 1,
    Leb = 2,
    FuncLength = 3,
    LocalTypeCount = 4,
    LocalCount = 5,
    LocalType = 6,
    ConstI32 = 7,
    ConstI64 = 8,
    ConstF32 = 9,
    ConstF64 = 10,
    DataIdx = 11,
    MemIdx = 12,
    LocalIdx = 13,
    GlobalIdx = 14,
    MemArgAlign = 15,
    MemArgX = 16,
    MemArgOffset = 17,
    FuncIdx = 18,
    BlockType = 19,
    BrTable = 20,
    LabelIdx = 21,
    LaneIdx = 22,
    VectorByte = 23,
    MiscLeb = 24, // LEBs that we don't want to specifically group
}

pub const QUEUE_LEN: usize = 0x4000;

pub struct DisModel {
    depth: u8,
    queue: [DisModelState; QUEUE_LEN],
    read_pos: usize,
    write_pos: usize,
    leb_val: u64,
    leb_shift: u64,
    float_bytes_left: usize,
    opcode: u8,
}

impl DisModel {
    pub fn new() -> Self {
        let mut model = Self {
            depth: 0,
            queue: unsafe { mem::zeroed() },
            read_pos: 0,
            write_pos: 3,
            leb_val: 0,
            leb_shift: 0,
            float_bytes_left: 0,
            opcode: 0,
        };

        // the total count of functions in this module
        model.queue[0] = DisModelState::Leb;

        // the header of the first function
        model.queue[1] = DisModelState::FuncLength;
        model.queue[2] = DisModelState::LocalTypeCount;

        model
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

        log::info!("{:?} | self.opcode: {:0X} | incoming byte: {:0X} (r: {}, w: {})", state, self.opcode, byte, self.read_pos, self.write_pos);

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

                    0xc | 0xd => {
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
                        self.float_bytes_left = 4;
                    }
                    0x44 => {
                        self.write(DisModelState::ConstF64);
                        self.float_bytes_left = 8;
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
                if let Some(opcode) = self.update_leb(byte) {
                    match self.opcode /* prefix */ {
                        0xfc => {
                            match opcode {
                                0x8 | 0xa => {
                                    self.write(DisModelState::DataIdx);
                                    self.write(DisModelState::MemIdx);
                                }
                                0x9 | 0xb | 0xd | 0xf..=0x12 => self.write(DisModelState::MemIdx),
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
                if let Some(num_local_types) = self.update_leb(byte) {
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
                if let Some(offset) = self.update_leb(byte) {
                    if offset >= 64 {
                        self.write(DisModelState::MemArgX);
                    }
                    self.write(DisModelState::MemArgOffset);
                }
            }
            DisModelState::BrTable => {
                if let Some(item_count) = self.update_leb(byte) {
                    // br_tables always contain one more element
                    for _ in 0..item_count {
                        self.write(DisModelState::LabelIdx);
                    }
                    self.write(DisModelState::LabelIdx);
                }
            }
            DisModelState::ConstF32 | DisModelState::ConstF64 => {
                // Floats are stored as IEEE floats, so just ignore the bytes
                self.float_bytes_left -= 1;
                if self.float_bytes_left > 0 {
                    // not yet done, remain in this state
                    should_remain = true;
                }
            }
            _ => {
                // In all other cases we're consuming some form of LEB
                if self.update_leb(byte).is_none() {
                    should_remain = true;
                }
            }
        }

        // Should we remain in the current state or move on to the next one in the queue?
        if !should_remain {
            self.read_pos += 1;
            self.read_pos %= QUEUE_LEN;

            // We've processed everything in the queue, so let's clear the queue and fall back to reading another opcode
            if self.read_pos == self.write_pos {
                self.read_pos = 0;
                self.write_pos = 1;
                self.queue[0] = DisModelState::Opcode;
            }
        }
    }

    fn update_leb(&mut self, byte: u8) -> Option<u64> {
        self.leb_val += ((byte & 0x7f) as u64) << self.leb_shift;
        self.leb_shift += 7;

        if byte & 0x80 != 0x80 {
            // done! move to the next state on the next byte
            let val = self.leb_val;
            self.leb_val = 0;
            self.leb_shift = 0;
            Some(val)
        } else {
            // not yet done, remain in this state
            None
        }
    }
}