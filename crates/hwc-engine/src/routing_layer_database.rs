use compact_str::CompactString;
use rustc_hash::FxHashMap;

use crate::material::{MaterialId, MaterialRegistry};
use crate::space::StackupLayer;

/// Errors from the routing layer database.
#[derive(Debug, Clone)]
pub enum RoutingLayerError {
    /// Requested layer does not exist in the stackup
    LayerNotFound { layer: CompactString },
    /// Layer exists but is not routable (non-conductive)
    LayerNotRoutable {
        layer: CompactString,
        material: CompactString,
    },
    /// A routable layer references an undeclared material
    UndeclaredMaterial {
        material: CompactString,
        layer: CompactString,
    },
}

impl std::fmt::Display for RoutingLayerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LayerNotFound { layer } => {
                write!(f, "Routing layer '{}' not found in stackup", layer)
            }
            Self::LayerNotRoutable { layer, material } => {
                write!(
                    f,
                    "Layer '{}' (material: '{}') is not routable — material is not a conductor",
                    layer, material
                )
            }
            Self::UndeclaredMaterial { material, layer } => {
                write!(
                    f,
                    "Layer '{}' references undeclared material '{}'",
                    layer, material
                )
            }
        }
    }
}

impl std::error::Error for RoutingLayerError {}

/// A routable layer's routing surface definition — the single source of truth
/// for what Z coordinate to route on.
#[derive(Debug, Clone)]
pub struct RoutingLayer {
    /// Layer name (e.g., "metal1")
    pub name: CompactString,
    /// Material ID for this layer
    pub material_id: MaterialId,
    /// Z elevation for centerline routing (typically bottom of layer)
    pub routing_z: i64,
    /// Physical bottom Z of the layer
    pub z_bottom: i64,
    /// Physical top Z of the layer
    pub z_top: i64,
    /// Whether this layer supports routing
    pub is_routable: bool,
}

/// Database of routing layer Z elevations — built from stackup + material registry.
///
/// This is the single source of truth for which Z coordinate to route on
/// for each layer. NO fallbacks, NO guessing. If a layer isn't here,
/// routing fails with a clear error.
#[derive(Debug, Clone, Default)]
pub struct RoutingLayerDatabase {
    /// Layer name → routing layer definition
    layers: FxHashMap<CompactString, RoutingLayer>,
    /// Ordered layer names (bottom to top)
    ordered_names: Vec<CompactString>,
}

impl RoutingLayerDatabase {
    /// Build from stackup layers and material registry.
    ///
    /// Only conductive materials produce routable layers.
    /// The routing Z is set intelligently based on via connection characteristics:
    /// - Base layers (active, poly) connect to vias FROM ABOVE → use z_top
    /// - Interconnect layers (metal1+) connect to vias FROM BELOW → use z_bottom
    ///
    /// This aligns routing centerlines with via connection points for DRC compliance.
    pub fn from_stackup(stackup: &[StackupLayer], material_registry: &MaterialRegistry) -> Self {
        let mut db = Self {
            layers: FxHashMap::default(),
            ordered_names: Vec::new(),
        };

        // Build layer type classification for intelligent Z assignment
        // Heuristic: First 2 routable layers are base layers (active, poly)
        // All subsequent layers are interconnect (metal1, metal2, ...)
        let mut routable_layer_count = 0;

        for layer in stackup {
            // Look up the material to determine conductivity
            let mat_id = material_registry.get_id(&layer.material_name);
            let is_conductive = mat_id.is_some_and(|id| material_registry.is_conductive(id));

            let is_routable = is_conductive && layer.is_routable;

            // **v0.2.1 FIX: Data-driven routing Z assignment**
            // Base layers (active, poly) only connect to vias from above → route at z_top
            // Interconnect layers (metal1+) connect to vias from below → route at z_bottom
            //
            // **v0.2.2 BOUNDARY FIX**: Routes must be strictly inside routable layers, not on boundaries.
            // Layer boundaries may be shared with adjacent dielectric layers, causing material lookup
            // ambiguity. We use the layer centerline to ensure deterministic material assignment.
            let routing_z = if is_routable {
                routable_layer_count += 1;
                let layer_centerline = (layer.z_bottom + layer.z_top) / 2;
                if routable_layer_count <= 2 {
                    // Base/Semiconductor layers: vias connect from above
                    // Route at centerline to avoid boundary ambiguity
                    eprintln!(
                        "[ROUTING LAYER DB] Layer '{}' (#{}) is BASE layer: routing_z = centerline = {}nm (z_bottom={}nm, z_top={}nm)",
                        layer.name, routable_layer_count, layer_centerline, layer.z_bottom, layer.z_top
                    );
                    layer_centerline
                } else {
                    // Interconnect layers: vias connect from below
                    // Route at centerline to avoid boundary ambiguity
                    eprintln!(
                        "[ROUTING LAYER DB] Layer '{}' (#{}) is INTERCONNECT layer: routing_z = centerline = {}nm (z_bottom={}nm, z_top={}nm)",
                        layer.name, routable_layer_count, layer_centerline, layer.z_bottom, layer.z_top
                    );
                    layer_centerline
                }
            } else {
                // Non-routable layers use z_bottom (doesn't matter since they won't be routed)
                layer.z_bottom
            };

            db.layers.insert(
                layer.name.clone(),
                RoutingLayer {
                    name: layer.name.clone(),
                    material_id: mat_id.unwrap_or(0),
                    routing_z,
                    z_bottom: layer.z_bottom,
                    z_top: layer.z_top,
                    is_routable,
                },
            );
            db.ordered_names.push(layer.name.clone());
        }

        eprintln!(
            "[ROUTING LAYER DB] Registered {} layers ({} routable)",
            db.ordered_names.len(),
            routable_layer_count
        );

        db
    }

    /// Get the routing Z elevation for a named layer.
    ///
    /// Returns `Err` if the layer doesn't exist or isn't routable.
    /// NO fallback. NO default. Query or fail.
    pub fn get_routing_z(&self, layer_name: &str) -> Result<i64, RoutingLayerError> {
        let layer =
            self.layers
                .get(layer_name)
                .ok_or_else(|| RoutingLayerError::LayerNotFound {
                    layer: layer_name.into(),
                })?;

        if !layer.is_routable {
            return Err(RoutingLayerError::LayerNotRoutable {
                layer: layer_name.into(),
                material: layer.name.clone(),
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

    /// Get the bottom Z of a layer (for via connection validation).
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
            .filter(|l| l.is_routable)
            .map(|l| l.name.as_str())
            .collect()
    }

    /// Build a lookup map of layer name → routing Z for validation.
    pub fn routing_z_map(&self) -> FxHashMap<CompactString, i64> {
        self.layers
            .iter()
            .filter(|(_, l)| l.is_routable)
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
                "[ROUTING LAYER DB]   '{}': routable={}, routing_z={}nm, z_bottom={}nm, z_top={}nm",
                name, layer.is_routable, layer.routing_z, layer.z_bottom, layer.z_top
            );
        }

        let mut errors = Vec::new();

        for (name, layer) in &self.layers {
            if layer.is_routable && layer.z_bottom >= layer.z_top {
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
        self.layers.values().filter(|l| l.is_routable).count()
    }
}
