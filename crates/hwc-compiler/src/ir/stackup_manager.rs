//! Resolves `Elevation::Physical` and `Elevation::Semantic` to absolute Z in nanometers.

use std::collections::HashMap;

use hwc_parser::ast::{Elevation, Expression, LayerStackup, MountingSide, Span, Unit};

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

    /// Ordered list of layer names (bottom-to-top) for index-based lookup.
    ordered_layers: Vec<String>,

    /// Fallback when operating in pure Assembly mode (no profile/stackup).
    /// Planned for future use; currently Physical always evaluates the expression directly.
    #[allow(dead_code)]
    default_z_voxel_nm: i64,

    /// Solder mask thickness loaded dynamically from the active profile.
    /// Used to offset component mounting planes so bodies sit on the mask, not on copper.
    pub solder_mask_thickness_nm: i64,
}

impl StackupManager {
    /// Create an empty stackup manager for tests or fallbacks.
    pub fn new_empty() -> Self {
        Self {
            layer_start_z_nm: HashMap::new(),
            layer_thickness_nm: HashMap::new(),
            ordered_layers: Vec::new(),
            default_z_voxel_nm: 0,
            solder_mask_thickness_nm: 0, // Opt-in: disabled unless profile declares solder_mask_thickness
        }
    }

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
        solder_mask_thickness_nm: i64,
    ) -> Result<Self, IrError> {
        let mut layer_start_z_nm = HashMap::new();
        let mut layer_thickness_nm = HashMap::new();
        let mut ordered_layers = Vec::new();

        if let Some(stackup) = stackup_opt {
            // Step 1: Resolve all thicknesses and calculate total height
            let mut resolved: Vec<(String, i64)> = Vec::new();

            for layer in &stackup.layers {
                let thickness_nm = evaluate_expression_to_nm(&layer.thickness, symbol_table)
                    .map_err(|e| {
                        IrError::PlacementError(format!(
                            "Failed to evaluate thickness for layer '{}': {}",
                            layer.name.name, e
                        ))
                    })?;

                resolved.push((layer.name.name.to_string(), thickness_nm));
            }

            // Step 2: Assign absolute Z positions
            // v0.1.7: The first layer in the stackup block is the PHYSICAL BOTTOM (Z=0).
            // This follows the "Foundation-First" principle for both ASIC and PCB.
            match origin_z {
                hwc_parser::OriginZ::Bottom => {
                    // Z=0 is the bottom of the board.
                    // The first layer in the file is the physical bottom.
                    let mut current_z = 0;
                    for (name, thickness_nm) in resolved {
                        layer_start_z_nm.insert(name.clone(), current_z);
                        layer_thickness_nm.insert(name.clone(), thickness_nm);
                        ordered_layers.push(name.clone());
                        eprintln!(
                            "[DEBUG stackup] Bottom-Up Mapping: {} -> z: {} nm (t: {} nm)",
                            name, current_z, thickness_nm
                        );
                        current_z += thickness_nm;
                    }
                }
                hwc_parser::OriginZ::Top => {
                    // Z=0 is the top of the board.
                    // The first layer in the file is the physical bottom, so it's at Z = -total_thickness.
                    // But for Top-origin, we usually want Z=0 to be the top copper.
                    // So the first layer (bottom) starts at -total_height.
                    let total_height: i64 = resolved.iter().map(|(_, t)| t).sum();
                    let mut current_z = -total_height;
                    for (name, thickness_nm) in resolved {
                        layer_start_z_nm.insert(name.clone(), current_z);
                        layer_thickness_nm.insert(name.clone(), thickness_nm);
                        ordered_layers.push(name.clone());
                        eprintln!("[DEBUG stackup] Top-Origin Mapping (First=Bottom): {} -> z: {} nm (t: {} nm)", name, current_z, thickness_nm);
                        current_z += thickness_nm;
                    }
                }
            }
        }

        Ok(Self {
            layer_start_z_nm,
            layer_thickness_nm,
            ordered_layers,
            default_z_voxel_nm,
            solder_mask_thickness_nm,
        })
    }

    /// Returns the total board thickness in nm.
    pub fn board_thickness_nm(&self) -> i64 {
        self.layer_thickness_nm.values().sum()
    }

    /// Get the absolute physical Z-boundary of the board for a mounting side.
    ///
    /// Accounts for the solder mask layer applied on outer surfaces.
    /// The mask thickness is loaded dynamically from the active profile's
    /// `manufacturing.solder_mask_thickness` (default: 20µm).
    /// Components mounted on top/bottom sit on the mask, not on copper.
    pub fn board_surface_z(&self, side: MountingSide) -> i64 {
        match side {
            MountingSide::Top => {
                // Top-mounted components sit on top of the top solder mask
                self.board_thickness_nm() + self.solder_mask_thickness_nm
            }
            MountingSide::Bottom => {
                // Bottom-mounted components sit underneath the bottom solder mask
                -self.solder_mask_thickness_nm
            }
            MountingSide::Embedded => {
                // Custom logic for cavities (defaults to middle layer)
                self.board_thickness_nm() / 2
            }
        }
    }

    /// Get the thickness of the outermost copper layer on the specified side
    pub fn outer_copper_thickness_nm(&self, side: hwc_parser::MountingSide) -> i64 {
        match side {
            hwc_parser::MountingSide::Top => {
                // Search for the first copper layer from the top
                for name in self.ordered_layers.iter().rev() {
                    if name.to_lowercase().contains('m') || name.to_lowercase().contains("copper") {
                        if let Some(t) = self.get_layer_thickness(name) {
                            return t;
                        }
                    }
                }
                // v0.1.7: No more hardcoded 35um. If no layer is found, return 0.
                // Callers must handle the missing layer error.
                self.get_layer_thickness("top")
                    .or_else(|| self.get_layer_thickness("L1"))
                    .unwrap_or(0)
            }
            hwc_parser::MountingSide::Bottom => {
                // Search for the first copper layer from the bottom
                for name in self.ordered_layers.iter() {
                    if name.to_lowercase().contains('m') || name.to_lowercase().contains("copper") {
                        if let Some(t) = self.get_layer_thickness(name) {
                            return t;
                        }
                    }
                }
                self.get_layer_thickness("bottom")
                    .or_else(|| self.get_layer_thickness("L2"))
                    .unwrap_or(0)
            }
            hwc_parser::MountingSide::Embedded => 0,
        }
    }

    /// Resolves any `Elevation` into an absolute Z position in nanometers.
    ///
    /// - `Physical`: Evaluates the expression directly (Assembly paradigm).
    /// - `Semantic`: Looks up the pre-computed starting Z from the LayerStackup (High-Level paradigm).
    /// - `Relative`: Returns 0 (intended for use in component-relative unrolling).
    pub fn resolve_elevation(
        &self,
        elevation: &Elevation,
        symbol_table: &SymbolTable,
    ) -> Result<i64, IrError> {
        match elevation {
            Elevation::Physical { start, .. } => {
                // Assembly paradigm — direct physical measurement/expression
                evaluate_expression_to_nm(start, symbol_table).map_err(|e| {
                    IrError::PlacementError(format!("Failed to evaluate physical Z: {}", e))
                })
            }
            Elevation::Semantic(ident) => self
                .layer_start_z_nm
                .get(&ident.name.to_string())
                .copied()
                .ok_or_else(|| {
                    IrError::PlacementError(format!(
                        "Unknown semantic layer '{}' in profile stackup",
                        ident.name
                    ))
                }),
            Elevation::Relative => Ok(0),
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
                .get_layer_thickness(ident.name.as_ref())
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
            Elevation::Relative => default_layer_height_nm,
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

    /// Returns the 0-based semantic layer index for a physical Z position.
    ///
    /// The index corresponds to the `ordered_layers` vector (bottom-to-top).
    pub fn get_layer_index_at_z(&self, z_nm: i64) -> Option<usize> {
        let count = self.ordered_layers.len();
        for (idx, name) in self.ordered_layers.iter().enumerate() {
            let start = *self.layer_start_z_nm.get(name)?;
            let thickness = *self.layer_thickness_nm.get(name)?;
            let is_top = idx == count - 1;
            
            // v0.1.7: Inclusive top boundary for the topmost layer.
            // This prevents coordinates exactly at the board surface (e.g. 1.27mm)
            // from being identified as "outside the board".
            let contains = if is_top {
                z_nm >= start && z_nm <= start + thickness
            } else {
                z_nm >= start && z_nm < start + thickness
            };

            if contains {
                return Some(idx);
            }
        }
        None
    }

    /// Returns the semantic layer name for a physical Z position.
    pub fn get_layer_name_at_z(&self, z_nm: i64) -> Option<String> {
        let count = self.ordered_layers.len();
        for (idx, name) in self.ordered_layers.iter().enumerate() {
            let start = *self.layer_start_z_nm.get(name)?;
            let thickness = *self.layer_thickness_nm.get(name)?;
            let is_top = idx == count - 1;

            let contains = if is_top {
                z_nm >= start && z_nm <= start + thickness
            } else {
                z_nm >= start && z_nm < start + thickness
            };

            if contains {
                return Some(name.clone());
            }
        }
        None
    }

    /// Returns a reference to the ordered layer names (bottom-to-top).
    pub fn ordered_layers(&self) -> &[String] {
        &self.ordered_layers
    }

    /// Returns the absolute Z starting position (bottom) in nm for a layer index.
    pub fn get_z_start_nm_for_layer_index(&self, index: usize) -> i64 {
        if let Some(name) = self.ordered_layers.get(index) {
            self.layer_start_z_nm.get(name).copied().unwrap_or(0)
        } else {
            0
        }
    }

    /// Returns the thickness in nm for a layer index.
    pub fn get_thickness_for_layer_index(&self, index: usize) -> i64 {
        if let Some(name) = self.ordered_layers.get(index) {
            self.layer_thickness_nm.get(name).copied().unwrap_or(0)
        } else {
            0
        }
    }

    /// Returns true if the layer name is the topmost physical layer.
    pub fn is_top_layer(&self, name: &str) -> bool {
        self.ordered_layers
            .last()
            .map(|n| n == name)
            .unwrap_or(false)
    }

    /// Returns true if the layer name is the bottommost physical layer.
    pub fn is_bottom_layer(&self, name: &str) -> bool {
        self.ordered_layers
            .first()
            .map(|n| n == name)
            .unwrap_or(false)
    }

    /// Returns the name of the layer at the given elevation.
    pub fn get_layer_name(&self, elevation: &Elevation) -> Option<String> {
        match elevation {
            Elevation::Semantic(ident) => Some(ident.name.to_string()),
            _ => None,
        }
    }

    /// Returns the number of semantic layers in the stackup.
    pub fn layer_count(&self) -> usize {
        self.ordered_layers.len()
    }

    /// Returns the semantic layer index for a named layer.
    pub fn get_index_for_layer(&self, layer_name: &str) -> Option<usize> {
        self.ordered_layers.iter().position(|l| l == layer_name)
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
                evaluate_expression_to_nm(z_expr, symbol_table).map_err(|e| {
                    IrError::PlacementError(format!(
                        "Failed to evaluate Z variable '{}': {}",
                        name, e
                    ))
                })
            }
            _ => evaluate_expression_to_nm(z_expr, symbol_table).map_err(|e| {
                IrError::PlacementError(format!("Failed to evaluate Z expression: {}", e))
            }),
        }
    }
}
