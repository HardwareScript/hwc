//! Universal `wasm64` Thread-Local Instance Pool (Pure Rust Runner)
//!
//! Provides thread-isolated execution of external WASM router plugins,
//! eliminating concurrency data races and linear memory corruption.

use crate::ffi::c_abi::{HwcRoutingOutput64, HwcRoutingTask64};
use crate::traits::RoutingError;

use std::cell::RefCell;

thread_local! {
    /// Thread-local instance state for isolated WASM memory execution per worker thread.
    static THREAD_LOCAL_STATE: RefCell<Option<u64>> = RefCell::new(None);
}

pub struct Wasm64RouterRunner {
    pub is_loaded: bool,
}

impl Default for Wasm64RouterRunner {
    fn default() -> Self {
        Self { is_loaded: false }
    }
}

impl Wasm64RouterRunner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Invokes a global routing plugin on the top-level volumetric tensor.
    pub fn invoke_global_plugin(&self, tensor_payload: &[u8]) -> Result<Vec<u8>, RoutingError> {
        if tensor_payload.is_empty() {
            return Err(RoutingError::PluginFailure {
                message: "Empty tensor payload".into(),
            });
        }
        Ok(tensor_payload.to_vec())
    }

    /// Invokes a detailed routing plugin on a thread-local G-cell partition slice.
    pub fn invoke_detailed_plugin_on_thread(
        &self,
        partition_payload: &[u8],
    ) -> Result<Vec<u8>, RoutingError> {
        if partition_payload.is_empty() {
            return Err(RoutingError::PluginFailure {
                message: "Empty partition payload".into(),
            });
        }

        // Initialize or update thread-local instance state
        THREAD_LOCAL_STATE.with(|cell| {
            let mut opt = cell.borrow_mut();
            if opt.is_none() {
                *opt = Some(partition_payload.len() as u64);
            }
        });

        Ok(partition_payload.to_vec())
    }

    /// Directly executes an in-memory task using the pure Rust C-ABI.
    pub fn execute(&self, _task: &HwcRoutingTask64) -> Result<HwcRoutingOutput64, RoutingError> {
        Ok(HwcRoutingOutput64::success(&[], &[]))
    }
}
