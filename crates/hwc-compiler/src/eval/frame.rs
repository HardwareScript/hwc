//! HardwareScript v0.3.1 VM CallFrame and Activation Records
//!
//! Provides static activation records for function invocations and space execution,
//! holding the active Merkle `HierarchicalPath` for span-independent `EntityId` derivation.

use compact_str::CompactString;
use hwc_engine::entity_graph::identity::HierarchicalPath;
use std::sync::Arc;

use super::opcodes::{Chunk, Register};

/// Activation record for a function or block call in the VM.
#[derive(Debug, Clone)]
pub struct CallFrame {
    pub chunk: Arc<Chunk>,
    pub ip: usize,
    pub stack_base: usize,
    pub return_register: Option<Register>,
    pub function_name: CompactString,
    /// Active Merkle hierarchical path at this activation frame.
    /// Pushed when a PCell or function call occurs; popped on Return.
    pub path: HierarchicalPath,
}

impl CallFrame {
    pub fn new(
        chunk: Arc<Chunk>,
        stack_base: usize,
        return_register: Option<Register>,
        function_name: impl Into<CompactString>,
    ) -> Self {
        let name = function_name.into();
        Self {
            chunk,
            ip: 0,
            stack_base,
            return_register,
            path: HierarchicalPath::root(name.as_str()),
            function_name: name,
        }
    }

    pub fn with_path(
        chunk: Arc<Chunk>,
        stack_base: usize,
        return_register: Option<Register>,
        function_name: impl Into<CompactString>,
        path: HierarchicalPath,
    ) -> Self {
        Self {
            chunk,
            ip: 0,
            stack_base,
            return_register,
            function_name: function_name.into(),
            path,
        }
    }
}
