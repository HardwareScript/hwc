//! Bridge definition types (v0.2.0 - First-Class Bridge Elevation)
//!
//! Bridges define physical material transitions for via generation.
//! Elevated from profile-nested rules to top-level definitions with export capability.

use serde::{Deserialize, Serialize};

use super::common::{Identifier, Measurement};
use crate::lexer::Span;
use compact_str::CompactString;

/// Top-level bridge definition: `bridge FromMaterial to ToMaterial:`
///
/// v0.2.0: Bridges are now first-class definitions that can be exported
/// and imported across files, eliminating the modularity defect of
/// profile-nested bridge rules.
///
/// Syntax:
/// ```hw
/// export bridge Silicon_N to Aluminum:
///     interface: Titanium_Silicide
///     thickness: 50nm
///     fill: Tungsten
/// ```
///
/// This defines the physical transition stack used by the Native Via Resolver
/// when generating contacts between the source and destination materials.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BridgeDefinition {
    /// Bridge identifier (auto-generated: from_to)
    pub name: Identifier,

    /// Whether this bridge is exported (v0.2.0: Access control)
    pub is_exported: bool,

    /// Source material name (e.g., "Silicon_N")
    pub from: CompactString,

    /// Destination material name (e.g., "Aluminum")
    pub to: CompactString,

    /// Bridge interface material name (e.g., "Titanium_Silicide")
    /// This is the thin layer that forms the ohmic contact with the source material
    pub interface_material: CompactString,

    /// Interface thickness (e.g., 50nm)
    /// Required for multi-layer bridge stacks
    pub interface_thickness: Option<Measurement>,

    /// Via fill material (e.g., "Tungsten")
    /// Fills the remainder of the via after the interface layer
    pub fill_material: Option<CompactString>,

    pub span: Span,
}

impl BridgeDefinition {
    /// Generate a canonical name for this bridge (from_to)
    pub fn canonical_name(&self) -> String {
        format!("{}_{}", self.from, self.to)
    }
}
