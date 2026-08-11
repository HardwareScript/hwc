//! Resolves `Elevation::Physical` and `Elevation::Semantic` to absolute Z in nanometers.

use rustc_hash::{FxHashMap, FxHashSet};

use hwc_parser::ast::{Elevation, Expression, LayerStackup, MountingSide, Span, Unit};

use crate::ir::conversions::evaluate_expression_to_nm;
use crate::ir::errors::IrError;
use crate::SymbolTable;

/// Manages the mapping from semantic layer names to physical Z positions in nanometers.
#[derive(Debug, Clone)]
pub struct StackupManager {
    /// Maps semantic layer name (e.g. "l1", "d1") to the absolute Z starting height (bottom of the layer) in nm.
    layer_start_z_nm: FxHashMap<String, i64>,

    /// Maps semantic layer name to its thickness in nanometers.
    layer_thickness_nm: FxHashMap<String, i64>,

    /// Ordered list of layer names (bottom-to-top) for index-based lookup.
    ordered_layers: Vec<String>,

    /// Maps semantic layer name to its material name.
    layer_materials: FxHashMap<String, String>,

    /// Set of layer names that are conductive (Conductor or Semiconductor).
    /// v0.1.8: Determined at construction by looking up materials in the Symbol Table.
    conductive_layers: FxHashSet<String>,

    /// Set of layer names that are zero-thickness masks (v0.2.1).
    /// These layers have Z-coordinates but contribute 0nm to stackup height.
    mask_layers: FxHashSet<String>,
}

impl StackupManager {
    /// Create an empty stackup manager for tests or fallbacks.
    pub fn new_empty() -> Self {
        Self {
            layer_start_z_nm: FxHashMap::default(),
            layer_thickness_nm: FxHashMap::default(),
            ordered_layers: Vec::new(),
            layer_materials: FxHashMap::default(),
            conductive_layers: FxHashSet::default(),
            mask_layers: FxHashSet::default(),
        }
    }

    /// Creates a new StackupManager from an optional `LayerStackup`.
    ///
    /// v0.2.1 (Bloat Purge Categories 1 & 7): The stackup is a pure, ordered
    /// bottom-to-top sandwich of material layers. Layer 0 starts at `Z = 0`
    /// (the absolute floor) and each subsequent layer stacks upward. There are
    /// no special cases: solder mask, coverlay, and passivation are ordinary
    /// declared layers in `profile.stackup`.
    ///
    /// v0.2.1 (Zero-Thickness Masks): Layers whose material category is `mask`
    /// have no physical Z-height. They anchor to the current cumulative Z
    /// (Z-Plane Surface Locking) without incrementing it, so the total board
    /// thickness is unaffected by mask layers.
    pub fn new(
        stackup_opt: Option<&LayerStackup>,
        symbol_table: &SymbolTable,
        eval_context: &hwc_parser::EvaluationContext,
    ) -> Result<Self, IrError> {
        let mut layer_start_z_nm = FxHashMap::default();
        let mut layer_thickness_nm = FxHashMap::default();
        let mut ordered_layers = Vec::new();
        let mut layer_materials = FxHashMap::default();
        let mut conductive_layers = FxHashSet::default();
        let mut mask_layers = FxHashSet::default();

        if let Some(stackup) = stackup_opt {
            // Step 1: Resolve all thicknesses, conductivity, and mask status.
            let mut resolved: Vec<(String, i64, bool, bool, String)> = Vec::new();

            for layer in &stackup.layers {
                let thickness_nm =
                    evaluate_expression_to_nm(&layer.thickness, symbol_table, eval_context)
                        .map_err(|e| IrError::StackupResolutionFailed {
                            layer_name: layer.name.name.clone(),
                            reason: format!("Failed to evaluate thickness: {}", e),
                        })?;

                // v0.1.8: Determine conductivity by looking up the material in the Symbol Table.
                // No hardcoded names or fallbacks.
                let Ok(mat_def) = symbol_table.get_material(&layer.material) else {
                    // Material not found in symbol table - this is an error in the design.
                    return Err(IrError::UndeclaredMaterial {
                        material: layer.material.clone(),
                    });
                };

                // v0.2.1: Masks are zero-thickness fabrication instructions.
                // They are never conductive and never routable.
                let is_mask = mat_def.category.is_zero_thickness();

                let is_conductive = match mat_def.category {
                    hwc_parser::MaterialCategory::Conductor
                    | hwc_parser::MaterialCategory::OhmicContact
                    | hwc_parser::MaterialCategory::DieInterconnect
                    | hwc_parser::MaterialCategory::PcbSolder
                    | hwc_parser::MaterialCategory::BarrierLayer
                    | hwc_parser::MaterialCategory::Adhesive
                    | hwc_parser::MaterialCategory::Semiconductor => true,
                    hwc_parser::MaterialCategory::Insulator
                    | hwc_parser::MaterialCategory::Mask => false,
                };

                resolved.push((
                    layer.name.name.to_string(),
                    thickness_nm,
                    is_conductive,
                    is_mask,
                    layer.material.to_string(),
                ));
            }

            // Step 2: Assign absolute Z positions with Z-Plane Surface Locking (v0.2.1).
            // The first layer in the stackup block is the PHYSICAL BOTTOM.
            // Physical layers accumulate Z-height; mask layers anchor to the current
            // Z_cumulative without incrementing it.
            // ZERO SPECIAL CASES. ZERO MAGIC.
            let mut current_z = 0i64;
            for (name, thickness_nm, is_conductive, is_mask, material) in resolved {
                // Z-PLANE SURFACE LOCKING THEOREM:
                // Mask layers lock to current Z_cumulative without incrementing height.
                layer_start_z_nm.insert(name.clone(), current_z);
                layer_thickness_nm.insert(name.clone(), thickness_nm);
                layer_materials.insert(name.clone(), material);
                ordered_layers.push(name.clone());

                if is_mask {
                    // FAIL-FAST: Masks MUST have zero thickness.
                    if thickness_nm != 0 {
                        return Err(IrError::InvalidMaskThickness {
                            layer_name: name.as_str().into(),
                            declared_nm: thickness_nm,
                        });
                    }
                    mask_layers.insert(name);
                    // DO NOT increment current_z (Z-plane surface lock).
                } else {
                    // Physical layer: accumulate Z-height normally.
                    if is_conductive {
                        conductive_layers.insert(name);
                    }
                    current_z += thickness_nm;
                }
            }
        }

        Ok(Self {
            layer_start_z_nm,
            layer_thickness_nm,
            ordered_layers,
            layer_materials,
            conductive_layers,
            mask_layers,
        })
    }

    /// Returns true if the named layer is a zero-thickness mask layer (v0.2.1).
    #[inline(always)]
    pub fn is_mask_layer(&self, layer_name: &str) -> bool {
        self.mask_layers.contains(layer_name)
    }

    /// Returns the set of all mask layer names (v0.2.1).
    pub fn get_mask_layers(&self) -> &FxHashSet<String> {
        &self.mask_layers
    }

    /// Returns the total board thickness in nm (sum of ALL stackup layers).
    pub fn board_thickness_nm(&self) -> i64 {
        self.layer_thickness_nm.values().sum()
    }

    /// Get the absolute physical Z-boundary of the board for a mounting side.
    ///
    /// v0.2.1: Protective coatings (solder mask, coverlay, passivation) are
    /// ordinary stackup layers, so the surfaces are simply the stackup bounds.
    /// Z = 0 is the absolute floor — no negative coordinates.
    pub fn board_surface_z(&self, side: MountingSide) -> i64 {
        match side {
            MountingSide::Top => self.board_thickness_nm(),
            MountingSide::Bottom => 0,
            MountingSide::Embedded => self.board_thickness_nm() / 2,
        }
    }

    /// Get the thickness of the outermost conductive layer on the specified side.
    ///
    /// v0.2.0: Returns an error if no conductive layer is found, rather than
    /// silently returning 0. The old behavior masked missing stackup layers.
    pub fn outer_conductive_thickness_nm(
        &self,
        side: hwc_parser::MountingSide,
    ) -> Result<i64, IrError> {
        match side {
            hwc_parser::MountingSide::Top => {
                // Search for the first conductive layer from the top
                for name in self.ordered_layers.iter().rev() {
                    if self.is_layer_conductive(name) {
                        return self.get_layer_thickness(name).ok_or_else(|| {
                            IrError::StackupResolutionFailed {
                                layer_name: name.clone().into(),
                                reason: "Conductive layer found but thickness is missing.".into(),
                            }
                        });
                    }
                }
                Err(IrError::StackupResolutionFailed {
                    layer_name: "top".into(),
                    reason: "No conductive layer found in stackup for top mounting.".into(),
                })
            }
            hwc_parser::MountingSide::Bottom => {
                // Search for the first conductive layer from the bottom
                for name in self.ordered_layers.iter() {
                    if self.is_layer_conductive(name) {
                        return self.get_layer_thickness(name).ok_or_else(|| {
                            IrError::StackupResolutionFailed {
                                layer_name: name.clone().into(),
                                reason: "Conductive layer found but thickness is missing.".into(),
                            }
                        });
                    }
                }
                Err(IrError::StackupResolutionFailed {
                    layer_name: "bottom".into(),
                    reason: "No conductive layer found in stackup for bottom mounting.".into(),
                })
            }
            hwc_parser::MountingSide::Embedded => Ok(0),
        }
    }

    /// Returns true if the layer name is conductive according to the Symbol Table.
    pub fn is_layer_conductive(&self, name: &str) -> bool {
        self.conductive_layers.contains(name)
    }

    /// Returns a reference to the ordered layer names (bottom-to-top).
    pub fn ordered_layers(&self) -> &[String] {
        &self.ordered_layers
    }

    /// **v0.2.0: Export stackup metadata for HardwareSpace**
    ///
    /// Converts the StackupManager's layer information into a format suitable for
    /// embedding in HardwareSpace. This provides a single source of truth for layer
    /// Z-coordinates accessible during export and validation without needing the full
    /// StackupManager.
    ///
    /// # Panics
    /// Never panics - returns an empty Vec if stackup is empty.
    pub fn export_stackup_layers(&self) -> Vec<hwc_engine::space::StackupLayer> {
        self.ordered_layers
            .iter()
            .filter_map(|name| {
                let z_bottom = self.layer_start_z_nm.get(name)?;
                let thickness = self.layer_thickness_nm.get(name)?;
                let material_name = self.layer_materials.get(name)?;
                let is_mask = self.mask_layers.contains(name);
                // v0.2.1: Mask layers are never routable, regardless of material
                // conductivity, because they carry no physical Z-height.
                let is_routable = !is_mask && self.conductive_layers.contains(name);

                Some(hwc_engine::space::StackupLayer::new(
                    name.as_str().into(),
                    *z_bottom,
                    z_bottom + thickness,
                    *thickness,
                    material_name.as_str().into(),
                    is_routable,
                    is_mask,
                ))
            })
            .collect()
    }

    /// Returns the material name for a layer by its index.
    pub fn get_material_for_layer_index(&self, index: usize) -> Option<String> {
        let name = self.ordered_layers.get(index)?;
        self.layer_materials.get(name).cloned()
    }

    /// Returns the top Z coordinate for a layer by its index.
    pub fn get_layer_top_z(&self, index: usize) -> Option<i64> {
        let name = self.ordered_layers.get(index)?;
        let start = self.layer_start_z_nm.get(name)?;
        let thickness = self.layer_thickness_nm.get(name)?;
        Some(start + thickness)
    }

    /// Returns the bottom Z coordinate for a layer by its index.
    pub fn get_layer_bottom_z(&self, index: usize) -> Option<i64> {
        let name = self.ordered_layers.get(index)?;
        self.layer_start_z_nm.get(name).copied()
    }

    /// Returns the starting Z (bottom) in nm for a semantic layer.
    pub fn get_layer_start_z(&self, layer_name: &str) -> Option<i64> {
        self.layer_start_z_nm.get(layer_name).copied()
    }

    /// Returns the centerline Z in nm for a semantic layer.
    pub fn get_layer_centerline_z(&self, layer_name: &str) -> Option<i64> {
        let start = self.layer_start_z_nm.get(layer_name).copied()?;
        let thickness = self.layer_thickness_nm.get(layer_name).copied()?;
        Some(start + thickness / 2)
    }

    /// Resolves any `Elevation` into an absolute Z position in nanometers.
    pub fn resolve_elevation(
        &self,
        elevation: &Elevation,
        symbol_table: &SymbolTable,
        eval_context: &hwc_parser::EvaluationContext,
    ) -> Result<i64, IrError> {
        self.resolve_elevation_bottom(elevation, symbol_table, eval_context, 0)
    }

    /// Returns the thickness in nm for a semantic layer (useful for via/contact spanning).
    pub fn get_layer_thickness(&self, layer_name: &str) -> Option<i64> {
        self.layer_thickness_nm.get(layer_name).copied()
    }

    /// Bottom Z for an elevation.
    pub fn resolve_elevation_bottom(
        &self,
        elevation: &Elevation,
        symbol_table: &SymbolTable,
        eval_context: &hwc_parser::EvaluationContext,
        _resolution_nm: i64,
    ) -> Result<i64, IrError> {
        match elevation {
            Elevation::Physical { start, .. } => {
                evaluate_expression_to_nm(start, symbol_table, eval_context).map_err(|e| {
                    IrError::CoordinateResolutionFailed {
                        coordinate_str: "physical Z expression".into(),
                        reason: e.to_string(),
                    }
                })
            }
            Elevation::Semantic(ident) => self
                .layer_start_z_nm
                .get(&ident.name.to_string())
                .copied()
                .ok_or_else(|| IrError::StackupResolutionFailed {
                    layer_name: ident.name.clone(),
                    reason: format!("Unknown semantic layer '{}' in profile stackup", ident.name),
                }),
            Elevation::Relative => Ok(0),
        }
    }

    /// Top Z (exclusive upper bound) for an elevation: bottom + layer thickness.
    pub fn resolve_elevation_top(
        &self,
        elevation: &Elevation,
        symbol_table: &SymbolTable,
        eval_context: &hwc_parser::EvaluationContext,
    ) -> Result<i64, IrError> {
        let bottom = self.resolve_elevation_bottom(elevation, symbol_table, eval_context, 0)?;
        let thickness = match elevation {
            Elevation::Semantic(ident) => self
                .get_layer_thickness(ident.name.as_ref())
                .ok_or_else(|| IrError::StackupResolutionFailed {
                    layer_name: ident.name.clone(),
                    reason: format!("Unknown semantic layer '{}' in profile stackup", ident.name),
                })?,
            Elevation::Physical { end, .. } => {
                if let Some(end_expr) = end {
                    let top = evaluate_expression_to_nm(end_expr, symbol_table, eval_context)
                        .map_err(|e| IrError::CoordinateResolutionFailed {
                            coordinate_str: "physical Z-end expression".into(),
                            reason: e.to_string(),
                        })?;
                    top - bottom
                } else {
                    return Err(IrError::CoordinateResolutionFailed {
                        coordinate_str: "physical elevation".into(),
                        reason: "Physical elevation must have an explicit 'to' Z-coordinate when resolving top boundary.".into(),
                    });
                }
            }
            Elevation::Relative => {
                return Err(IrError::CoordinateResolutionFailed {
                    coordinate_str: "relative elevation".into(),
                    reason:
                        "Relative elevation cannot resolve a top boundary without a layer context."
                            .into(),
                });
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
        self.get_layer_index_at_z(z_nm)
            .map(|idx| self.ordered_layers[idx].clone())
    }

    /// Returns the absolute Z starting position (bottom) in nm for a layer index.
    pub fn get_z_start_nm_for_layer_index(&self, index: usize) -> Result<i64, IrError> {
        if let Some(name) = self.ordered_layers.get(index) {
            self.layer_start_z_nm.get(name).copied().ok_or_else(|| {
                IrError::StackupResolutionFailed {
                    layer_name: name.clone().into(),
                    reason: "Layer index found but Z-start mapping is missing.".into(),
                }
            })
        } else {
            Err(IrError::StackupResolutionFailed {
                layer_name: format!("index {}", index).into(),
                reason: "Layer index out of bounds.".into(),
            })
        }
    }

    /// Returns the thickness in nm for a layer index.
    pub fn get_thickness_for_layer_index(&self, index: usize) -> Result<i64, IrError> {
        if let Some(name) = self.ordered_layers.get(index) {
            self.layer_thickness_nm.get(name).copied().ok_or_else(|| {
                IrError::StackupResolutionFailed {
                    layer_name: name.clone().into(),
                    reason: "Layer index found but thickness mapping is missing.".into(),
                }
            })
        } else {
            Err(IrError::StackupResolutionFailed {
                layer_name: format!("index {}", index).into(),
                reason: "Layer index out of bounds.".into(),
            })
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

    /// Returns the material name for a given layer (v0.2.1)
    ///
    /// # Arguments
    /// * `layer_name` - The semantic layer name (e.g., "poly", "metal1")
    ///
    /// # Returns
    /// Material name if layer exists, None otherwise
    pub fn get_layer_material(&self, layer_name: &str) -> Option<&str> {
        self.layer_materials.get(layer_name).map(|s| s.as_str())
    }

    /// Returns the number of semantic layers in the stackup.
    pub fn layer_count(&self) -> usize {
        self.ordered_layers.len()
    }

    /// Returns the semantic layer index for a named layer.
    pub fn get_index_for_layer(&self, layer_name: &str) -> Option<usize> {
        self.ordered_layers.iter().position(|l| l == layer_name)
    }

    /// Semantic layer index (0-based) from a bottom Z elevation.
    pub fn layer_index_at_z(&self, z_bottom_nm: i64) -> Option<usize> {
        self.get_layer_index_at_z(z_bottom_nm)
    }

    /// Resolve a Z coordinate expression, supporting semantic layer names (e.g. Variable "l1")
    /// for Z-Context Inheritance in modules. Falls back to physical evaluation.
    /// This allows module internals to inherit the parent's StackupManager profile.
    pub fn resolve_z_expression(
        &self,
        z_expr: &Expression,
        symbol_table: &SymbolTable,
        eval_context: &hwc_parser::EvaluationContext,
    ) -> Result<i64, IrError> {
        match z_expr {
            Expression::Variable { name, .. } => {
                if let Some(z) = self.get_layer_start_z(name.as_str()) {
                    return Ok(z);
                }
                // Not a known semantic layer — treat as physical expression
                evaluate_expression_to_nm(z_expr, symbol_table, eval_context).map_err(|e| {
                    IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("Z variable '{}'", name),
                        reason: e.to_string(),
                    }
                })
            }
            _ => evaluate_expression_to_nm(z_expr, symbol_table, eval_context).map_err(|e| {
                IrError::CoordinateResolutionFailed {
                    coordinate_str: "Z expression".into(),
                    reason: e.to_string(),
                }
            }),
        }
    }
}
