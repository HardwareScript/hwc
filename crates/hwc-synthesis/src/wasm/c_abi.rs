// crates/hwc-synthesis/src/wasm/c_abi.rs

use std::ffi::c_char;

/// Synthesized Standard-Cell Instance Record (64-bit pointers).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct HwcMappedCell64 {
    pub instance_id: u64,
    pub cell_name: *const c_char,
    pub pos_x_pm: i64,
    pub pos_y_pm: i64,
    pub pin_count: u32,
    pub pin_names: *const *const c_char,
    pub net_ids: *const u32,
}

/// Synthesis Input Task Payload (64-bit pointers).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct HwcSynthesisTask64 {
    pub top_module_name: *const c_char,
    pub rtl_source_json: *const c_char,
    pub rtl_source_len: u64,
    pub liberty_db_ptr: *const c_char,
    pub liberty_db_len: u64,
    pub target_freq_mhz: f32,
}

/// Synthesis Output GateNetlist.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct HwcSynthesisOutput64 {
    pub cell_count: u64,
    pub cells_ptr: *const HwcMappedCell64,
    pub status_code: u32, // 0 = Success, >0 = Error Code
    pub error_msg: *const c_char,
}
