// crates/hwc-synthesis/src/wasm/mod.rs

pub mod c_abi;
pub mod wasm64_runner;

pub use c_abi::{HwcMappedCell64, HwcSynthesisOutput64, HwcSynthesisTask64};
pub use wasm64_runner::Wasm64SynthesisRunner;
