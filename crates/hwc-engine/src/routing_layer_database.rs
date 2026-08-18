//! Routing Layer Database - Strongly-Typed Architecture
//!
//! **Architectural Law: ZERO HEURISTICS**
//! NO layer counting (`routable_layer_count <= 2`), NO string matching,
//! NO guessing. Every layer is classified through explicit, strongly-typed
//! LayerKind domain models derived PURELY from MaterialCategory.

use compact_str::CompactString;
use rustc_hash::FxHashMap;

use crate::material::{MaterialId, MaterialRegistry};
use crate::space::StackupLayer;
use crate::stackup::{LayerKind, RoutingSurfacePolicy};

/// Errors from the routing layer database.
#[derive(Debug, Clone)]
pub enum RoutingLayerError {
    /// Requested layer does not exist in the stackup
    LayerNotFound { layer: CompactString },
    /// Layer exists but is not routable (non-conductive or mask)
    LayerNotRoutable {
        layer: CompactString,
        reason: NonRoutableReason,
    },
    /// Material not found in registry
    MaterialNotFound {
        material: CompactString,
        layer: CompactString,
    },
    /// Material exists but is not conductive
    MaterialNotConductive {
        material: CompactString,
        layer: CompactString,
    },
}

/// Reason why a layer cannot be routed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NonRoutableReason {
    /// Layer is a 2D fabrication mask (NSDM, PSDM, TAP)
    FabricationMask,
    /// Layer is dielectric (non-conductive)
    Dielectric,
    /// Layer explicitly marked as non-routable in stackup
    ExplicitlyDisabled,
}

impl std::fmt::Display for NonRoutableReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FabricationMask => write!(f, "layer is a 2D fabrication mask"),
            Self::Dielectric => write!(f, "layer is non-conductive dielectric"),
            Self::ExplicitlyDisabled => write!(f, "layer explicitly marked as non-routable"),
        }
    }
}

impl std::fmt::Display for RoutingLayerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LayerNotFound { layer } => {
                write!(f, "Routing layer '{}' not found in stackup", layer)
            }
            Self::LayerNotRoutable { layer, reason } => {
                write!(f, "Layer '{}' is not routable: {}", layer, reason)
            }
            Self::MaterialNotFound { material, layer } => {
                write!(
                    f,
                    "Layer '{}' references undeclared material '{}'",
                    layer, material
                )
            }
            Self::MaterialNotConductive { material, layer } => {
                write!(
                    f,
                    "Layer '{}' material '{}' is not conductive",
                    layer, material
                )
            }
        }
    }
}

impl std::error::Error for RoutingLayerError {}

/// A routable layer's routing surface definition — the single source of truth
/// for what Z coordinate to route on.
///
/// **v0.3.0: Strongly-typed layer classification**
/// Uses LayerKind (derived from MaterialCategory) instead of legacy heuristic-based
/// classification. NO string matching, NO layer counting.
#[derive(Debug, Clone)]
pub struct RoutingLayer {
    /// Layer name (e.g., "metal1", "poly", "diff")
    pub name: CompactString,
    /// Material ID for this layer
    pub material_id: MaterialId,
    /// **Strongly-typed layer classification (v0.3.0)**
    /// Derived from MaterialCategory, not from string matching
    pub kind: LayerKind,
    /// Z elevation for routing (computed from RoutingSurfacePolicy)
    pub routing_z: i64,
    /// Physical bottom Z of the layer
    pub z_bottom: i64,
    /// Physical top Z of the layer
    pub z_top: i64,
    /// Whether this layer is routable (conductive + stackup allows it)
    pub is_routable: bool,
}

impl RoutingLayer {
    /// Check if this layer supports routing
    #[inline]
    pub fn is_routable(&self) -> bool {
        self.is_routable
    }

    /// Check if this layer is a fabrication mask
    #[inline]
    pub fn is_mask(&self) -> bool {
        self.kind.is_zero_thickness_mask()
    }

    /// Get the layer centerline Z coordinate
    #[inline]
    pub fn centerline_z(&self) -> i64 {
        (self.z_bottom + self.z_top) / 2
    }

    /// Get the routing surface policy for this layer
    #[inline]
    pub fn routing_policy(&self) -> RoutingSurfacePolicy {
        self.kind.routing_policy(self.is_routable)
    }
}

/// Database of routing layer Z elevations — built from stackup + material registry.
///
/// This is the single source of truth for which Z coordinate to route on
/// for each layer. NO fallbacks, NO guessing. If a layer isn't here,
/// routing fails with a clear error.
///
/// **v0.3.0: ZERO HEURISTICS ARCHITECTURE**
/// All layer classification is driven by:
/// 1. MaterialCategory from material definition (Conductor, Insulator, Semiconductor, Mask)
/// 2. Stackup `routable: true/false` declaration
/// 3. LayerKind::from_material_category() pure derivation
/// 4. RoutingSurfacePolicy from physical reasoning
///
/// NO string matching, NO layer counting, NO name inspection.
#[derive(Debug, Clone, Default)]
pub struct RoutingLayerDatabase {
    /// Layer name → routing layer definition
    layers: FxHashMap<CompactString, RoutingLayer>,
    /// Ordered layer names (bottom to top)
    ordered_names: Vec<CompactString>,
}

impl RoutingLayerDatabase {
    /// **Build from stackup layers and material registry using PURE TYPE DERIVATION**
    ///
    /// # Architectural Law: Zero Heuristics
    ///
    /// All layer classification is driven by:
    /// 1. Material registry lookup: layer.material_name → MaterialId
    /// 2. Category extraction: MaterialId → MaterialCategory
    /// 3. Pure type derivation: MaterialCategory → LayerKind
    /// 4. Policy computation: LayerKind + is_routable → RoutingSurfacePolicy
    /// 5. Z-coordinate calculation: Policy + (z_bottom, z_top) → routing_z
    ///
    /// NO STRING MATCHING. NO LAYER COUNTING. NO NAME INSPECTION.
    ///
    /// # Why This Works Universally
    ///
    /// Materials are declared with explicit categories in .hw files:
    /// ```hw
    /// material NSDM: mask
    /// material SiO2: insulator
    /// material N_Diff: semiconductor
    /// material Aluminum: conductor
    /// ```
    ///
    /// The compiler maps:
    /// - category: mask → LayerKind::LithoMask → NonRoutable
    /// - category: insulator → LayerKind::Dielectric → NonRoutable
    /// - category: semiconductor → LayerKind::SemiconductorActive → SurfaceTop
    /// - category: conductor → LayerKind::ConductiveInterconnect → LayerCenterline
    ///
    /// This works for ANY foundry naming convention (TSMC's "OD"/"PO", Intel's
    /// "DIFF"/"GATE", PCB's "RF_Strip"/"L1_Top") because the physical behavior
    /// is derived from the material's DECLARED CATEGORY, not its NAME.
    ///
    /// # Returns
    /// - `Ok(Self)` - Successfully built database
    /// - `Err(Vec<RoutingLayerError>)` - One or more layers reference undeclared materials
    pub fn from_stackup(
        stackup: &[StackupLayer],
        material_registry: &MaterialRegistry,
    ) -> Result<Self, Vec<RoutingLayerError>> {
        let mut db = Self {
            layers: FxHashMap::default(),
            ordered_names: Vec::new(),
        };

        let mut errors = Vec::new();

        for layer in stackup {
            // Step 1: Get material ID (strict lookup, no guessing)
            let mat_id = match material_registry.get_id(&layer.material_name) {
                Some(id) => id,
                None => {
                    errors.push(RoutingLayerError::MaterialNotFound {
                        material: layer.material_name.clone(),
                        layer: layer.name.clone(),
                    });
                    continue;
                }
            };

            // Step 2: Get material category (strict lookup, no guessing)
            let category = match material_registry.get_category(mat_id) {
                Some(cat) => cat,
                None => {
                    errors.push(RoutingLayerError::MaterialNotFound {
                        material: layer.material_name.clone(),
                        layer: layer.name.clone(),
                    });
                    continue;
                }
            };

            // Step 3: Derive layer kind purely from material category (ZERO string matching)
            let kind = LayerKind::from_material_category(category);

            // Step 4: Determine routability from physics + stackup declaration
            let is_routable = kind.is_conductive() && layer.is_routable;

            // Step 5: Compute routing surface policy
            let policy = kind.routing_policy(is_routable);

            // Step 6: Calculate routing Z using the strongly-typed policy
            let routing_z = match policy {
                RoutingSurfacePolicy::SurfaceTop => layer.z_top,
                RoutingSurfacePolicy::LayerCenterline => (layer.z_bottom + layer.z_top) / 2,
                RoutingSurfacePolicy::NonRoutable => layer.z_bottom, // Arbitrary (not used)
            };

            eprintln!(
                "[ROUTING LAYER DB] Layer '{}' (Material: '{}', Category: {:?}): {} → routing_z = {}nm (policy: {:?}, z: {}nm…{}nm)",
                layer.name,
                layer.material_name,
                category,
                kind.description(),
                routing_z,
                policy,
                layer.z_bottom,
                layer.z_top
            );

            db.layers.insert(
                layer.name.clone(),
                RoutingLayer {
                    name: layer.name.clone(),
                    material_id: mat_id,
                    kind,
                    routing_z,
                    z_bottom: layer.z_bottom,
                    z_top: layer.z_top,
                    is_routable,
                },
            );
            db.ordered_names.push(layer.name.clone());
        }

        if !errors.is_empty() {
            return Err(errors);
        }

        let routable_count = db.layers.values().filter(|l| l.is_routable()).count();
        eprintln!(
            "[ROUTING LAYER DB] Registered {} layers ({} routable)",
            db.ordered_names.len(),
            routable_count
        );

        Ok(db)
    }

    /// Get the routing Z elevation for a named layer.
    ///
    /// Returns error if the layer doesn't exist or isn't routable.
    pub fn get_routing_z(&self, layer_name: &str) -> Result<i64, RoutingLayerError> {
        let layer = self
            .layers
            .get(layer_name)
            .ok_or_else(|| RoutingLayerError::LayerNotFound {
                layer: layer_name.into(),
            })?;

        if !layer.is_routable() {
            let reason = if layer.is_mask() {
                NonRoutableReason::FabricationMask
            } else if !layer.kind.is_conductive() {
                NonRoutableReason::Dielectric
            } else {
                NonRoutableReason::ExplicitlyDisabled
            };
            return Err(RoutingLayerError::LayerNotRoutable {
                layer: layer_name.into(),
                reason,
            });
        }

        Ok(layer.routing_z)
    }

    /// Get the full routing layer definition for a named layer.
    pub fn get_layer(&self, layer_name: &str) -> Result<&RoutingLayer, RoutingLayerError> {
        self.layers
            .get(layer_name)
            .ok_or_else(|| RoutingLayerError::LayerNotFound {
                layer: layer_name.into(),
            })
    }

    /// Get the bottom Z of a layer.
    pub fn get_layer_bottom_z(&self, layer_name: &str) -> Result<i64, RoutingLayerError> {
        self.get_layer(layer_name).map(|l| l.z_bottom)
    }

    /// Get the top Z of a layer.
    pub fn get_layer_top_z(&self, layer_name: &str) -> Result<i64, RoutingLayerError> {
        self.get_layer(layer_name).map(|l| l.z_top)
    }

    /// List all routable layer names.
    pub fn list_routable_layers(&self) -> Vec<&str> {
        self.layers
            .values()
            .filter(|l| l.is_routable())
            .map(|l| l.name.as_str())
            .collect()
    }

    /// Build a lookup map of layer name → routing Z for validation.
    pub fn routing_z_map(&self) -> FxHashMap<CompactString, i64> {
        self.layers
            .iter()
            .filter(|(_, l)| l.is_routable())
            .map(|(name, l)| (name.clone(), l.routing_z))
            .collect()
    }

    /// Validate the database — ensure all routable layers have valid Z ranges.
    pub fn validate(&self) -> Result<(), Vec<RoutingLayerError>> {
        eprintln!(
            "[ROUTING LAYER DB] Validating {} layers:",
            self.layers.len()
        );
        for (name, layer) in &self.layers {
            eprintln!(
                "[ROUTING LAYER DB]   '{}': kind={:?}, routing_z={}nm, z_bottom={}nm, z_top={}nm, routable={}",
                name, layer.kind, layer.routing_z, layer.z_bottom, layer.z_top, layer.is_routable
            );
        }

        let mut errors = Vec::new();

        for (name, layer) in &self.layers {
            if layer.is_routable() && layer.z_bottom >= layer.z_top {
                errors.push(RoutingLayerError::LayerNotFound {
                    layer: name.clone(),
                });
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Get the number of routable layers.
    pub fn routable_layer_count(&self) -> usize {
        self.layers.values().filter(|l| l.is_routable()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::MaterialCategory;

    #[test]
    fn test_routing_layer_database_construction() {
        // Create a material registry
        let mut registry = MaterialRegistry::new();
        let mask_id = registry.register_with_properties(
            "NSDM",
            MaterialCategory::Mask,
            crate::material::ManufacturingProcess::Deposited,
        );
        let active_id = registry.register_with_properties(
            "N_Diff",
            MaterialCategory::Semiconductor,
            crate::material::ManufacturingProcess::Deposited,
        );
        let metal_id = registry.register_with_properties(
            "Aluminum",
            MaterialCategory::Conductor,
            crate::material::ManufacturingProcess::Deposited,
        );

        // Create stackup layers
        let stackup = vec![
            StackupLayer::new(
                "nsdm".into(),
                0,
                0,
                0,
                "NSDM".into(),
                false,
                true,
                LayerKind::LithoMask,
            ),
            StackupLayer::new(
                "diff".into(),
                0,
                150,
                150,
                "N_Diff".into(),
                true,
                false,
                LayerKind::SemiconductorActive,
            ),
            StackupLayer::new(
                "metal1".into(),
                350,
                710,
                360,
                "Aluminum".into(),
                true,
                false,
                LayerKind::ConductiveInterconnect,
            ),
        ];

        // Build database
        let db = RoutingLayerDatabase::from_stackup(&stackup, &registry).unwrap();

        // Verify layer classification
        let mask_layer = db.get_layer("nsdm").unwrap();
        assert!(!mask_layer.is_routable());
        assert!(mask_layer.is_mask());

        let diff_layer = db.get_layer("diff").unwrap();
        assert!(diff_layer.is_routable());
        assert_eq!(diff_layer.routing_z, 150); // SurfaceTop → z_top

        let metal_layer = db.get_layer("metal1").unwrap();
        assert!(metal_layer.is_routable());
        assert_eq!(metal_layer.routing_z, 530); // LayerCenterline → (350+710)/2
    }
}
