#![cfg_attr(not(feature = "std"), no_std)]

// The code in this file is heavily inspired and modified from the PAQ range of compressors.
// Go to http://mattmahoney.net/dc/ to learn more!

extern crate alloc;

pub mod codec;
mod predictor;
