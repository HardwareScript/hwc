//! Device Contract Extensions for Sprint 1.5
//!
//! This module extends the basic DeviceDefinition with advanced contract features:
//! - Material constraints (MustBe, MustNotBe, MustHaveProperty)
//! - Extraction rules (geometric and connectivity constraints)
//! - SPICE model templates
//!
//! These extensions enable the LVS engine to validate layouts against contracts
//! and provide detailed error messages when violations occur.

use compact_str::CompactString;
use serde::{Deserialize, Serialize};

use super::common::Identifier;
use crate::lexer::Span;
use rustc_hash::FxHashMap;

/// Material constraint for device terminals
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MaterialConstraint {
    /// Terminal must use one of these materials
    MustBe(Vec<CompactString>),

    /// Terminal must NOT use any of these materials
    MustNotBe(Vec<CompactString>),

    /// Terminal material must have a specific property value
    MustHaveProperty {
        property: CompactString,
        value: f64,
        tolerance: Option<f64>,
    },
}

/// Geometric constraint for terminal extraction
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GeometricConstraint {
    /// Terminal must cross another region
    Crosses { region: String },

    /// Terminal must be adjacent to another terminal
    AdjacentTo { terminal: String },

    /// Terminal must be on opposite side of another terminal
    OppositeSideOf {
        terminal: CompactString,
        reference: String,
    },

    /// Terminal must overlap a region
    Overlaps { region: String },

    /// Terminal must have minimum dimensions
    MinDimensions { width: f64, height: f64 },

    /// No geometric constraint
    None,
}

/// Connectivity constraint for terminal extraction
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConnectivityConstraint {
    /// Terminal must be electrically connected to another terminal
    ConnectedTo { terminal: String },

    /// Terminal must be isolated from another terminal
    IsolatedFrom { terminal: String },

    /// No connectivity constraint
    None,
}

/// Extraction rule for identifying device terminals in voxel grid
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractionRule {
    /// Terminal name this rule applies to
    pub terminal_name: CompactString,

    /// Material constraint for this terminal
    pub material_constraint: MaterialConstraint,

    /// Geometric constraint for this terminal
    pub geometric_constraint: GeometricConstraint,

    /// Connectivity constraint for this terminal
    pub connectivity_constraint: ConnectivityConstraint,
}

/// SPICE model template for device simulation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpiceModelTemplate {
    /// SPICE model type (nmos, pmos, resistor, capacitor, etc.)
    pub model_type: CompactString,

    /// Model card name (e.g., "NMOS_TSMC180")
    pub model_card: CompactString,

    /// Required parameters (W, L, AS, AD, PS, PD, etc.)
    pub parameters: Vec<CompactString>,
}

/// Extended device contract with validation rules
///
/// This extends the basic DeviceDefinition with:
/// - Advanced material constraints
/// - Extraction rules for terminal identification
/// - SPICE model templates for simulation
/// - Parameter tolerance specifications for LVS
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceContract {
    /// Device type name
    pub name: Identifier,

    /// Required terminals
    pub terminals: Vec<CompactString>,

    /// Material constraints for each terminal (replaces simple materials map)
    pub material_constraints: FxHashMap<CompactString, MaterialConstraint>,

    /// Extraction rules for identifying terminals in voxel grid
    pub extraction_rules: Vec<ExtractionRule>,

    /// SPICE model template for simulation
    pub spice_model: Option<SpiceModelTemplate>,

    /// Parameter tolerance specifications for Alignment Layer (e.g., W: 1%, L: 1%, AS: 5%)
    /// Values are relative tolerances (0.01 = 1%)
    pub parameter_tolerance: FxHashMap<CompactString, f64>,

    /// Source span for error reporting
    pub span: Span,
}

impl DeviceContract {
    /// Create a device contract from a basic device definition
    ///
    /// Converts material mappings to MustBe constraints.
    /// Supports both single materials and lists of allowed materials.
    /// Copies tolerance specifications if present.
    pub fn from_device_definition(def: &super::device::DeviceDefinition) -> Self {
        let mut material_constraints = FxHashMap::default();

        // Convert material mappings to MustBe constraints
        // DeviceDefinition stores SmallVec, convert to Vec for MaterialConstraint
        for (terminal, materials) in &def.materials {
            material_constraints.insert(
                terminal.clone(),
                MaterialConstraint::MustBe(materials.to_vec()),
            );
        }

        // Copy tolerance specifications if present
        let parameter_tolerance = def.tolerance.clone().unwrap_or_default();

        Self {
            name: def.name.clone(),
            terminals: def.terminals.to_vec(),
            material_constraints,
            extraction_rules: Vec::new(),
            spice_model: None,
            parameter_tolerance,
            span: def.span,
        }
    }

    /// Get parameter tolerance for a specific parameter
    /// Returns default 1% if not specified
    pub fn get_parameter_tolerance(&self, param_name: &str) -> f64 {
        self.parameter_tolerance
            .get(param_name)
            .copied()
            .unwrap_or(0.01)
    }

    /// Get material constraint for a terminal
    pub fn get_material_constraint(&self, terminal: &str) -> Option<&MaterialConstraint> {
        self.material_constraints.get(terminal)
    }

    /// Check if a terminal is defined for this device
    pub fn has_terminal(&self, terminal: &str) -> bool {
        self.terminals.iter().any(|t| t.as_str() == terminal)
    }

    /// Validate a material against the constraint for a terminal
    ///
    /// Returns Ok(()) if valid, Err with reason if invalid
    pub fn validate_terminal_material(&self, terminal: &str, material: &str) -> Result<(), String> {
        let constraint = self
            .get_material_constraint(terminal)
            .ok_or_else(|| format!("Terminal '{}' not defined in contract", terminal))?;

        match constraint {
            MaterialConstraint::MustBe(allowed) => {
                if allowed.iter().any(|m| m.as_str() == material) {
                    Ok(())
                } else {
                    Err(format!(
                        "Material '{}' not allowed for terminal '{}'. Expected one of: {}",
                        material,
                        terminal,
                        allowed.join(", ")
                    ))
                }
            }
            MaterialConstraint::MustNotBe(forbidden) => {
                if forbidden.iter().any(|m| m.as_str() == material) {
                    Err(format!(
                        "Material '{}' is forbidden for terminal '{}'. Cannot use: {}",
                        material,
                        terminal,
                        forbidden.join(", ")
                    ))
                } else {
                    Ok(())
                }
            }
            MaterialConstraint::MustHaveProperty {
                property,
                value,
                tolerance,
            } => {
                // Property validation would require material database lookup
                // For now, we'll accept any material (property validation is future work)
                let _ = (property, value, tolerance);
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_material_constraint_must_be() {
        let constraint = MaterialConstraint::MustBe(vec!["Polysilicon".into(), "Aluminum".into()]);

        match constraint {
            MaterialConstraint::MustBe(materials) => {
                assert_eq!(materials.len(), 2);
                assert!(materials.contains(&"Polysilicon".into()));
                assert!(materials.contains(&"Aluminum".into()));
            }
            _ => panic!("Wrong constraint type"),
        }
    }

    #[test]
    fn test_device_contract_validation() {
        let mut contract = DeviceContract {
            name: Identifier::new("NMOS".into(), Span::new(0, 0)),
            terminals: vec!["gate".into(), "source".into()],
            material_constraints: FxHashMap::default(),
            extraction_rules: Vec::new(),
            spice_model: None,
            parameter_tolerance: FxHashMap::default(),
            span: Span::new(0, 0),
        };

        contract.material_constraints.insert(
            "gate".into(),
            MaterialConstraint::MustBe(vec!["Polysilicon".into()]),
        );

        // Valid material
        assert!(contract
            .validate_terminal_material("gate", "Polysilicon")
            .is_ok());

        // Invalid material
        assert!(contract
            .validate_terminal_material("gate", "Copper")
            .is_err());
    }
}
