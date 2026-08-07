use compact_str::CompactString;
use rustc_hash::FxHashMap;

use crate::material::{MaterialId, MaterialRegistry};
use crate::space::StackupLayer;

/// Errors from the via-layer mapping database.
#[derive(Debug, Clone)]
pub enum ViaLayerMappingError {
    /// Material not found in registry
    MaterialNotFound { material: CompactString },
    /// No stackup layer found for a material
    LayerNotFoundForMaterial { material: CompactString },
    /// No via connection defined for this material pair
    ViaConnectionNotFound {
        from_material: CompactString,
        to_material: CompactString,
    },
}

impl std::fmt::Display for ViaLayerMappingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MaterialNotFound { material } => {
                write!(f, "Material '{}' not found in registry", material)
            }
            Self::LayerNotFoundForMaterial { material } => {
                write!(f, "No stackup layer found for material '{}'", material)
            }
            Self::ViaConnectionNotFound {
                from_material,
                to_material,
            } => {
                write!(
                    f,
                    "No via connection defined for '{}' → '{}'",
                    from_material, to_material
                )
            }
        }
    }
}

impl std::error::Error for ViaLayerMappingError {}

/// Via connection specification — generated from bridge rules + stackup.
///
/// Describes which layers a via connects and at what Z elevations.
#[derive(Debug, Clone)]
pub struct ViaConnection {
    /// Material ID of the via fill/interface
    pub via_material_id: MaterialId,
    /// Bottom layer name (e.g., "poly")
    pub bottom_layer_name: CompactString,
    /// Bottom connection Z (top of bottom layer — where the via meets lower routing layer)
    pub bottom_connection_z: i64,
    /// Top layer name (e.g., "metal1")
    pub top_layer_name: CompactString,
    /// Top connection Z (bottom of top layer — where the via meets upper routing layer)
    pub top_connection_z: i64,
}

/// Database of via-to-layer mappings — built from bridge rules + stackup.
///
/// Maps (from_material, to_material) pairs to via connection specs.
/// This is the single source of truth for which layers a via connects.
#[derive(Debug, Clone)]
pub struct ViaLayerMappingDatabase {
    /// Maps (from_material_id, to_material_id) → ViaConnection
    via_specs: FxHashMap<(MaterialId, MaterialId), ViaConnection>,
}

impl ViaLayerMappingDatabase {
    /// Build from bridge rules and stackup.
    ///
    /// Each bridge rule defines a material transition. The stackup provides
    /// the layer Z coordinates for each material.
    pub fn from_stackup(stackup: &[StackupLayer], material_registry: &MaterialRegistry) -> Self {
        let mut db = Self {
            via_specs: FxHashMap::default(),
        };

        // Generate via connections for all adjacent conductive layer pairs
        let conductive_layers: Vec<&StackupLayer> =
            stackup.iter().filter(|l| l.is_routable).collect();

        for window in conductive_layers.windows(2) {
            let from_layer = window[0];
            let to_layer = window[1];

            if let Some(from_mat_id) = material_registry.get_id(&from_layer.material_name) {
                if let Some(to_mat_id) = material_registry.get_id(&to_layer.material_name) {
                    let via_conn = ViaConnection {
                        via_material_id: from_mat_id,
                        bottom_layer_name: from_layer.name.clone(),
                        bottom_connection_z: from_layer.z_top,
                        top_layer_name: to_layer.name.clone(),
                        top_connection_z: to_layer.z_bottom,
                    };
                    db.via_specs.insert((from_mat_id, to_mat_id), via_conn);
                }
            }
        }

        db
    }

    /// Build from bridge rules, stackup, and material registry.
    ///
    /// Bridge rules explicitly define which materials connect via which interface.
    /// This is the authoritative source for via connections.
    pub fn from_bridge_rules(
        bridge_rules: &[BridgeRuleInput],
        stackup: &[StackupLayer],
        material_registry: &MaterialRegistry,
    ) -> Result<Self, ViaLayerMappingError> {
        let mut db = Self {
            via_specs: FxHashMap::default(),
        };

        for bridge in bridge_rules {
            let from_mat_id = material_registry
                .get_id(&bridge.from_material)
                .ok_or_else(|| ViaLayerMappingError::MaterialNotFound {
                    material: bridge.from_material.clone(),
                })?;

            let to_mat_id = material_registry
                .get_id(&bridge.to_material)
                .ok_or_else(|| ViaLayerMappingError::MaterialNotFound {
                    material: bridge.to_material.clone(),
                })?;

            let from_layer = stackup
                .iter()
                .find(|l| {
                    material_registry
                        .get_id(&l.material_name)
                        .map_or(false, |id| id == from_mat_id)
                })
                .ok_or_else(|| ViaLayerMappingError::LayerNotFoundForMaterial {
                    material: bridge.from_material.clone(),
                })?;

            let to_layer = stackup
                .iter()
                .find(|l| {
                    material_registry
                        .get_id(&l.material_name)
                        .map_or(false, |id| id == to_mat_id)
                })
                .ok_or_else(|| ViaLayerMappingError::LayerNotFoundForMaterial {
                    material: bridge.to_material.clone(),
                })?;

            let via_material_id = material_registry
                .get_id(&bridge.interface_material)
                .unwrap_or(from_mat_id);

            let via_conn = ViaConnection {
                via_material_id,
                bottom_layer_name: from_layer.name.clone(),
                bottom_connection_z: from_layer.z_top,
                top_layer_name: to_layer.name.clone(),
                top_connection_z: to_layer.z_bottom,
            };

            db.via_specs.insert((from_mat_id, to_mat_id), via_conn);
        }

        Ok(db)
    }

    /// Get the via connection spec for a material pair.
    ///
    /// Returns `Err` if no connection is defined — never guesses.
    pub fn get_via_connection(
        &self,
        from_material: MaterialId,
        to_material: MaterialId,
    ) -> Result<&ViaConnection, ViaLayerMappingError> {
        self.via_specs
            .get(&(from_material, to_material))
            .ok_or_else(|| ViaLayerMappingError::ViaConnectionNotFound {
                from_material: format!("mat_{}", from_material).into(),
                to_material: format!("mat_{}", to_material).into(),
            })
    }

    /// Get via connection using material names (convenience wrapper).
    pub fn get_via_connection_by_names(
        &self,
        from_material: &str,
        to_material: &str,
        material_registry: &MaterialRegistry,
    ) -> Result<&ViaConnection, ViaLayerMappingError> {
        let from_id = material_registry.get_id(from_material).ok_or_else(|| {
            ViaLayerMappingError::MaterialNotFound {
                material: from_material.into(),
            }
        })?;
        let to_id = material_registry.get_id(to_material).ok_or_else(|| {
            ViaLayerMappingError::MaterialNotFound {
                material: to_material.into(),
            }
        })?;
        self.get_via_connection(from_id, to_id)
    }

    /// Check if a via connection exists for a material pair.
    pub fn has_via_connection(&self, from_material: MaterialId, to_material: MaterialId) -> bool {
        self.via_specs.contains_key(&(from_material, to_material))
    }

    /// Get the total number of registered via connections.
    pub fn connection_count(&self) -> usize {
        self.via_specs.len()
    }
}

impl Default for ViaLayerMappingDatabase {
    fn default() -> Self {
        Self {
            via_specs: FxHashMap::default(),
        }
    }
}

/// Simplified bridge rule input for database construction.
#[derive(Debug, Clone)]
pub struct BridgeRuleInput {
    pub from_material: CompactString,
    pub to_material: CompactString,
    pub interface_material: CompactString,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::{ManufacturingProcess, MaterialConductivity};

    fn make_test_data() -> (Vec<StackupLayer>, MaterialRegistry) {
        let stackup = vec![
            StackupLayer::new("poly".into(), 400, 850, 450, "Polysilicon".into(), true),
            StackupLayer::new("metal1".into(), 1250, 1650, 400, "Aluminum".into(), true),
        ];

        let mut reg = MaterialRegistry::new();
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

        (stackup, reg)
    }

    #[test]
    fn test_from_stackup_generates_adjacent_connections() {
        let (stackup, reg) = make_test_data();
        let db = ViaLayerMappingDatabase::from_stackup(&stackup, &reg);

        let poly_id = reg.get_id("Polysilicon").unwrap();
        let alum_id = reg.get_id("Aluminum").unwrap();

        let conn = db.get_via_connection(poly_id, alum_id).unwrap();
        assert_eq!(conn.bottom_layer_name.as_str(), "poly");
        assert_eq!(conn.top_layer_name.as_str(), "metal1");
        assert_eq!(conn.bottom_connection_z, 850); // top of poly
        assert_eq!(conn.top_connection_z, 1250); // bottom of metal1
    }

    #[test]
    fn test_from_bridge_rules() {
        let (stackup, reg) = make_test_data();
        let bridges = vec![BridgeRuleInput {
            from_material: "Polysilicon".into(),
            to_material: "Aluminum".into(),
            interface_material: "Titanium_Silicide".into(),
        }];

        let db = ViaLayerMappingDatabase::from_bridge_rules(&bridges, &stackup, &reg).unwrap();

        let poly_id = reg.get_id("Polysilicon").unwrap();
        let alum_id = reg.get_id("Aluminum").unwrap();

        let conn = db.get_via_connection(poly_id, alum_id).unwrap();
        assert_eq!(conn.bottom_connection_z, 850);
        assert_eq!(conn.top_connection_z, 1250);
    }

    #[test]
    fn test_missing_connection_returns_error() {
        let db = ViaLayerMappingDatabase::default();
        let result = db.get_via_connection(1, 2);
        assert!(result.is_err());
    }
}
