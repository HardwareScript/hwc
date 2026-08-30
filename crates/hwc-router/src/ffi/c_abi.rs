//! Canonical 64-bit Memory64 ABI for Physical Routing Plugins (100% Pure Rust).
//!
//! Replaces legacy manual C header `hwc_router_abi.h`.
//! Implements `#[repr(C)]` memory layouts compatible with W3C WebAssembly `Memory64`.

use std::ffi::c_char;

/// 1. Exact picometer wire segment (i64: 1 pm = 10^-12 m)
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HwcWireSegment64 {
    pub net_id: u32,
    pub layer_idx: u8,
    pub start_x_pm: i64,
    pub start_y_pm: i64,
    pub start_z_pm: i64,
    pub end_x_pm: i64,
    pub end_y_pm: i64,
    pub end_z_pm: i64,
    pub width_pm: i64,
}

/// 2. Exact picometer via instance
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HwcViaInstance64 {
    pub net_id: u32,
    pub x_pm: i64,
    pub y_pm: i64,
    pub z_bottom_pm: i64,
    pub z_top_pm: i64,
    pub from_layer_idx: u8,
    pub to_layer_idx: u8,
    pub diameter_pm: i64,
}

/// 3. Input Task Payload (64-bit pointers across W3C Memory64)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct HwcRoutingTask64 {
    pub num_nets: u64,
    pub num_obstacles: u64,
    pub num_access_points: u64,
    pub task_payload_ptr: *const u8,
    pub task_payload_len: u64,
}

/// 4. Output Geometry Returned by the Router
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct HwcRoutingOutput64 {
    pub wire_count: u64,
    pub wires_ptr: *const HwcWireSegment64,
    pub via_count: u64,
    pub vias_ptr: *const HwcViaInstance64,
    pub status_code: u32, // 0 = Success, >0 = Error Code
    pub error_msg: *const c_char,
}

impl HwcRoutingOutput64 {
    /// Constructs a success payload.
    pub fn success(wires: &[HwcWireSegment64], vias: &[HwcViaInstance64]) -> Self {
        Self {
            wire_count: wires.len() as u64,
            wires_ptr: if wires.is_empty() { std::ptr::null() } else { wires.as_ptr() },
            via_count: vias.len() as u64,
            vias_ptr: if vias.is_empty() { std::ptr::null() } else { vias.as_ptr() },
            status_code: 0,
            error_msg: std::ptr::null(),
        }
    }

    /// Constructs an error payload with an error code and message pointer.
    pub fn error(code: u32, message: *const c_char) -> Self {
        Self {
            wire_count: 0,
            wires_ptr: std::ptr::null(),
            via_count: 0,
            vias_ptr: std::ptr::null(),
            status_code: code,
            error_msg: message,
        }
    }
}
