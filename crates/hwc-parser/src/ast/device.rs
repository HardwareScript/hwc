//! Device definition types for foundry primitives

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use super::common::Identifier;
use crate::lexer::Span;
use compact_str::CompactString;
use rustc_hash::FxHashMap;

/// Device definition: `device NMOS:` (v0.1.6)
///
/// Defines the physical contract for a foundry primitive (transistor, diode, etc.)
/// Specifies required terminals and expected materials for each terminal.
///
/// Sprint 1.5: Enhanced to support multiple allowed materials per terminal
/// Sprint 4.1: Enhanced to support parameter tolerance specifications
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceDefinition {
    pub name: Identifier,
    pub terminals: SmallVec<[CompactString; 4]>,
    /// Terminal materials: terminal_name -> allowed material(s)
    /// Can be single material or list of alternatives
    pub materials: FxHashMap<CompactString, SmallVec<[CompactString; 2]>>,
    /// Parameter tolerance specifications (e.g., W: 1%, L: 1%, AS: 5%)
    /// Values are relative tolerances (0.01 = 1%)
    pub tolerance: Option<FxHashMap<CompactString, f64>>,
    pub span: Span,
}

impl DeviceDefinition {
    /// Get the expected materials for a terminal
    pub fn get_terminal_materials(&self, terminal: &str) -> Option<&[CompactString]> {
        self.materials.get(terminal).map(|v| v.as_slice())
    }

    /// Check if a terminal is defined for this device
    pub fn has_terminal(&self, terminal: &str) -> bool {
        self.terminals.iter().any(|t| t.as_str() == terminal)
    }

    /// Check if a material is allowed for a terminal
    pub fn is_material_allowed(&self, terminal: &str, material: &str) -> bool {
        self.materials
            .get(terminal)
            .map(|allowed| allowed.iter().any(|m| m == material))
            .unwrap_or(false)
    }
}
