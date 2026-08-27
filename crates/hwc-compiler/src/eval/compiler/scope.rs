//! Scope management for variable bindings during compilation

use compact_str::CompactString;
use rustc_hash::FxHashMap;

use crate::eval::opcodes::Register;

/// Scope tracking local variable registers and mutability
#[derive(Debug, Clone, Default)]
pub struct Scope {
    pub bindings: FxHashMap<CompactString, (Register, bool)>,
}

impl Scope {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bind(&mut self, name: impl Into<CompactString>, reg: Register, is_mutable: bool) {
        self.bindings.insert(name.into(), (reg, is_mutable));
    }

    pub fn lookup(&self, name: &str) -> Option<(Register, bool)> {
        self.bindings.get(name).copied()
    }
}
