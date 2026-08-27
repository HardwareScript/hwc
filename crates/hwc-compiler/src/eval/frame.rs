//! HardwareScript v0.3.0 VM CallFrame and Activation Records
//!
//! Provides static activation records for function invocations and space execution.

use compact_str::CompactString;
use std::sync::Arc;

use super::opcodes::{Chunk, Register};

/// Activation record for a function or block call in the VM
#[derive(Debug, Clone)]
pub struct CallFrame {
    pub chunk: Arc<Chunk>,
    pub ip: usize,
    pub stack_base: usize,
    pub return_register: Option<Register>,
    pub function_name: CompactString,
}

impl CallFrame {
    pub fn new(
        chunk: Arc<Chunk>,
        stack_base: usize,
        return_register: Option<Register>,
        function_name: impl Into<CompactString>,
    ) -> Self {
        Self {
            chunk,
            ip: 0,
            stack_base,
            return_register,
            function_name: function_name.into(),
        }
    }
}
