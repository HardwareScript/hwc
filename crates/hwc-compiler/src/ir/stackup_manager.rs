//! Resolves `Elevation::Physical` and `Elevation::Semantic` to absolute Z in nanometers.

use std::collections::HashMap;

use hwc_parser::ast::{Elevation, Expression, LayerStackup, Span, Unit};

use crate::ir::conversions::evaluate_expression_to_nm;
use crate::ir::errors::IrError;
use crate::SymbolTable;

/// Manages the mapping from semantic layer names to physical Z positions in nanometers.
#[derive(Debug, Clone)]
pub struct StackupManager {
    /// Maps semantic layer name (e.g. "l1", "d1") to the absolute Z starting height (bottom of the layer) in nm.
    /// For the voxel engine, this is the lowest Z voxel this layer occupies.
    layer_start_z_nm: HashMap<String, i64>,

    /// Maps semantic layer name to its thickness in nanometers.
    layer_thickness_nm: HashMap<String, i64>,

    /// Fallback when operating in pure Assembly mode (no profile/stackup).
    /// Planned for future use; currently Physical always evaluates the expression directly.
    #[allow(dead_code)]
    default_z_voxel_nm: i64,
}

impl StackupManager {
    /// Creates a new StackupManager from an optional `LayerStackup`.
    ///
    /// The stackup is assumed to be defined **top-to-bottom** in the source file
    /// (common in PCB design: l1 is the top copper layer).
    ///
    /// The manager inverts this so that `Z=0` is at the bottom of the board,
    /// matching the Voxel Engine's coordinate system.
    pub fn new(
        stackup_opt: Option<&LayerStackup>,
        symbol_table: &SymbolTable,
        default_z_voxel_nm: i64,
        origin_z: hwc_parser::OriginZ,
    ) -> Result<Self, IrError> {
        let mut layer_start_z_nm = HashMap::new();
        let mut layer_thickness_nm = HashMap::new();

        if let Some(stackup) = stackup_opt {
            // Step 1: Resolve all thicknesses and calculate total height
            let mut resolved: Vec<(String, i64)> = Vec::new();

            for layer in &stackup.layers {
                let thickness_nm = evaluate_expression_to_nm(&layer.thickness, symbol_table)
                    .map_err(|e| IrError::PlacementError(format!(
                        "Failed to evaluate thickness for layer '{}': {}",
                        layer.name.name, e
                    )))?;

                resolved.push((layer.name.name.to_string(), thickness_nm));
            }

            // Step 2: Assign absolute Z positions
            // Rule: The first layer in the stackup (top of the file) is the PHYSICAL TOP.
            match origin_z {
                hwc_parser::OriginZ::Bottom => {
                     // Z=0 is the bottom of the board.
                     // The last layer in the file is the physical bottom.
                     let mut current_z = 0;
                     for (name, thickness_nm) in resolved.into_iter().rev() {
                         layer_start_z_nm.insert(name.clone(), current_z);
                         layer_thickness_nm.insert(name.clone(), thickness_nm);
                         eprintln!("[DEBUG stackup] Bottom-Up Mapping: {} -> z: {} nm (t: {} nm)", name, current_z, thickness_nm);
                         current_z += thickness_nm;
                     }
                 }
                 hwc_parser::OriginZ::Top => {
                     // Z=0 is the top of the board.
                     // The first layer in the file is the physical top.
                     let mut current_z = 0;
                     for (name, thickness_nm) in resolved {
                         layer_start_z_nm.insert(name.clone(), current_z);
                         layer_thickness_nm.insert(name.clone(), thickness_nm);
                         eprintln!("[DEBUG stackup] Top-Down Mapping: {} -> z: {} nm (t: {} nm)", name, current_z, thickness_nm);
                         current_z += thickness_nm;
                     }
                 }
            }
        }

        Ok(Self {
            layer_start_z_nm,
            layer_thickness_nm,
            default_z_voxel_nm,
        })
    }

    /// Resolves any `Elevation` into an absolute Z position in nanometers.
    ///
    /// - `Physical`: Evaluates the expression directly (Assembly paradigm).
    /// - `Semantic`: Looks up the pre-computed starting Z from the LayerStackup (High-Level paradigm).
    pub fn resolve_elevation(
        &self,
        elevation: &Elevation,
        symbol_table: &SymbolTable,
    ) -> Result<i64, IrError> {
        match elevation {
            Elevation::Physical { start, .. } => {
                // Assembly paradigm — direct physical measurement/expression
                evaluate_expression_to_nm(start, symbol_table)
                    .map_err(|e| IrError::PlacementError(format!("Failed to evaluate physical Z: {}", e)))
            }
            Elevation::Semantic(ident) => {
                self.layer_start_z_nm
                    .get(&ident.name.to_string())
                    .copied()
                    .ok_or_else(|| {
                        IrError::PlacementError(format!(
                            "Unknown semantic layer '{}' in profile stackup",
                            ident.name
                        ))
                    })
            }
        }
    }

    /// Returns the thickness in nm for a semantic layer (useful for via/contact spanning).
    pub fn get_layer_thickness(&self, layer_name: &str) -> Option<i64> {
        self.layer_thickness_nm.get(layer_name).copied()
    }

    /// Returns the starting Z (bottom) in nm for a semantic layer.
    pub fn get_layer_start_z(&self, layer_name: &str) -> Option<i64> {
        self.layer_start_z_nm.get(layer_name).copied()
    }

    /// Top Z (exclusive upper bound) for an elevation: bottom + layer thickness.
    ///
    /// Physical elevations use `default_layer_height_nm` (typically one Z voxel).
    /// Semantic elevations use stackup thickness when available.
    pub fn resolve_elevation_top(
        &self,
        elevation: &Elevation,
        symbol_table: &SymbolTable,
        default_layer_height_nm: i64,
    ) -> Result<i64, IrError> {
        let bottom = self.resolve_elevation(elevation, symbol_table)?;
        let thickness = match elevation {
            Elevation::Semantic(ident) => self
                .get_layer_thickness(&ident.name.to_string())
                .unwrap_or(default_layer_height_nm),
            Elevation::Physical { end, .. } => {
                if let Some(end_expr) = end {
                    let top = evaluate_expression_to_nm(end_expr, symbol_table).map_err(|e| {
                        IrError::PlacementError(format!("Failed to evaluate physical Z-end: {}", e))
                    })?;
                    top - bottom
                } else {
                    default_layer_height_nm
                }
            }
        };
        Ok(bottom + thickness.max(1))
    }

    /// Build a Physical `Elevation` from an absolute Z position in nanometers (IR / auto-via codegen).
    pub fn elevation_from_z_nm(z_nm: i64, span: Span) -> Elevation {
        let value_mm = z_nm as f64 / 1_000_000.0;
        Elevation::Physical {
            start: Expression::Measurement {
                value: value_mm,
                unit: Unit::Millimeter,
                span,
            },
            end: None,
        }
    }

    /// Voxel slab index (0-based) from a bottom Z elevation — used only for via-library lookup in auto-via.
    pub fn grid_layer_index(z_bottom_nm: i64, voxel_z_nm: i64) -> usize {
        (z_bottom_nm / voxel_z_nm.max(1)).max(0) as usize
    }

    /// Resolve a Z coordinate expression, supporting semantic layer names (e.g. Variable "l1")
    /// for Z-Context Inheritance in modules. Falls back to physical evaluation.
    /// This allows module internals to inherit the parent's StackupManager profile.
    pub fn resolve_z_expression(
        &self,
        z_expr: &Expression,
        symbol_table: &SymbolTable,
    ) -> Result<i64, IrError> {
        match z_expr {
            Expression::Variable { name, .. } => {
                if let Some(z) = self.get_layer_start_z(name.as_str()) {
                    return Ok(z);
                }
                // Not a known semantic layer — treat as physical expression
                evaluate_expression_to_nm(z_expr, symbol_table)
                    .map_err(|e| IrError::PlacementError(format!("Failed to evaluate Z variable '{}': {}", name, e)))
            }
            _ => evaluate_expression_to_nm(z_expr, symbol_table)
                .map_err(|e| IrError::PlacementError(format!("Failed to evaluate Z expression: {}", e))),
        }
    }
}
