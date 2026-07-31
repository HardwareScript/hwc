use compact_str::CompactString;
use rustc_hash::FxHashMap;

use crate::material::{MaterialConductivity, MaterialId, MaterialRegistry};
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
    UndeclaredMaterial { material: CompactString, layer: CompactString },
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
#[derive(Debug, Clone)]
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
    /// The routing Z is set to the bottom of the layer (where vias connect).
    pub fn from_stackup(
        stackup: &[StackupLayer],
        material_registry: &MaterialRegistry,
    ) -> Self {
        let mut db = Self {
            layers: FxHashMap::default(),
            ordered_names: Vec::new(),
        };

        for layer in stackup {
            // Look up the material to determine conductivity
            let mat_id = material_registry.get_id(&layer.material_name);
            let is_conductive = mat_id.map_or(false, |id| {
                matches!(
                    material_registry.get_conductivity(id),
                    Some(MaterialConductivity::Conductor | MaterialConductivity::Semiconductor)
                )
            });

            let routing_z = layer.z_bottom;

            db.layers.insert(
                layer.name.clone(),
                RoutingLayer {
                    name: layer.name.clone(),
                    material_id: mat_id.unwrap_or(0),
                    routing_z,
                    z_bottom: layer.z_bottom,
                    z_top: layer.z_top,
                    is_routable: is_conductive && layer.is_routable,
                },
            );
            db.ordered_names.push(layer.name.clone());
        }

        db
    }

    /// Get the routing Z elevation for a named layer.
    ///
    /// Returns `Err` if the layer doesn't exist or isn't routable.
    /// NO fallback. NO default. Query or fail.
    pub fn get_routing_z(&self, layer_name: &str) -> Result<i64, RoutingLayerError> {
        let layer = self.layers.get(layer_name).ok_or_else(|| {
            RoutingLayerError::LayerNotFound {
                layer: layer_name.into(),
            }
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
        self.layers.get(layer_name).ok_or_else(|| RoutingLayerError::LayerNotFound {
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

impl Default for RoutingLayerDatabase {
    fn default() -> Self {
        Self {
            layers: FxHashMap::default(),
            ordered_names: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::ManufacturingProcess;

    fn make_test_stackup() -> Vec<StackupLayer> {
        vec![
            StackupLayer::new("active".into(), 0, 400, 400, "Silicon".into(), true),
            StackupLayer::new("poly".into(), 400, 850, 450, "Polysilicon".into(), true),
            StackupLayer::new("metal1".into(), 1250, 1650, 400, "Aluminum".into(), true),
        ]
    }

    fn make_test_registry() -> MaterialRegistry {
        let mut reg = MaterialRegistry::new();
        reg.register_with_properties(
            "Silicon",
            MaterialConductivity::Semiconductor,
            ManufacturingProcess::Deposited,
        );
        reg.register_with_properties(
            "Polysilicon",
            MaterialConductivity::Conductor,
            ManufacturingProcess::Deposited,
        );
        reg.register_with_properties(
            "Aluminum",
            MaterialConductivity::Conductor,
            ManufacturingProcess::Deposited,
        );
        reg
    }

    #[test]
    fn test_routing_z_from_stackup() {
        let stackup = make_test_stackup();
        let registry = make_test_registry();
        let db = RoutingLayerDatabase::from_stackup(&stackup, &registry);

        assert_eq!(db.get_routing_z("active").unwrap(), 0);
        assert_eq!(db.get_routing_z("poly").unwrap(), 400);
        assert_eq!(db.get_routing_z("metal1").unwrap(), 1250);
    }

    #[test]
    fn test_nonexistent_layer_fails() {
        let stackup = make_test_stackup();
        let registry = make_test_registry();
        let db = RoutingLayerDatabase::from_stackup(&stackup, &registry);

        let result = db.get_routing_z("nonexistent");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RoutingLayerError::LayerNotFound { .. }));
    }

    #[test]
    fn test_list_routable_layers() {
        let stackup = make_test_stackup();
        let registry = make_test_registry();
        let db = RoutingLayerDatabase::from_stackup(&stackup, &registry);

        let routable = db.list_routable_layers();
        assert_eq!(routable.len(), 3);
        assert!(routable.contains(&"active"));
        assert!(routable.contains(&"poly"));
        assert!(routable.contains(&"metal1"));
    }
}
