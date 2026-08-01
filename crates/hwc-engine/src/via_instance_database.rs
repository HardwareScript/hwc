//! Via Instance Database
//!
//! Tracks all explicit via/contact instances in the design.
//! Queried by ViaResolver to avoid inserting duplicate automatic vias.

use compact_str::CompactString;
use rustc_hash::FxHashMap;

use crate::netlist::NetId;

/// A single via/contact instance in the design.
#[derive(Debug, Clone)]
pub struct ViaInstance {
    /// Entity name of the via/contact
    pub entity_name: CompactString,
    /// Net this via connects
    pub net_id: NetId,
    /// Bottom layer name (e.g., "active", "poly")
    pub from_layer: CompactString,
    /// Top layer name (e.g., "metal1")
    pub to_layer: CompactString,
    /// XY bounding box of the via
    pub xy_bbox: (i64, i64, i64, i64), // (min_x, min_y, max_x, max_y)
    /// Z range of the via (for validation)
    pub z_range: (i64, i64), // (min_z, max_z)
}

/// Database of all explicit via/contact instances.
///
/// Populated during contact placement.
/// Queried during auto-via insertion to avoid duplicates.
#[derive(Debug, Clone, Default)]
pub struct ViaInstanceDatabase {
    /// All vias indexed by net
    vias_by_net: FxHashMap<NetId, Vec<ViaInstance>>,
    /// All vias indexed by layer pair (from_layer, to_layer)
    vias_by_layers: FxHashMap<(CompactString, CompactString), Vec<ViaInstance>>,
}

impl ViaInstanceDatabase {
    /// Create a new empty database.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an explicit via/contact instance.
    pub fn register(
        &mut self,
        entity_name: &str,
        net_id: NetId,
        from_layer: &str,
        to_layer: &str,
        xy_bbox: (i64, i64, i64, i64),
        z_range: (i64, i64),
    ) {
        let instance = ViaInstance {
            entity_name: entity_name.into(),
            net_id,
            from_layer: from_layer.into(),
            to_layer: to_layer.into(),
            xy_bbox,
            z_range,
        };

        // Index by net
        self.vias_by_net
            .entry(net_id)
            .or_insert_with(Vec::new)
            .push(instance.clone());

        // Index by layer pair
        self.vias_by_layers
            .entry((from_layer.into(), to_layer.into()))
            .or_insert_with(Vec::new)
            .push(instance);
    }

    /// Check if an explicit via exists that connects two layers on a specific net
    /// at a specific XY location.
    ///
    /// Returns true if a via exists within the query region.
    pub fn has_via_at(
        &self,
        net_id: NetId,
        from_layer: &str,
        to_layer: &str,
        query_xy: (i64, i64), // (center_x, center_y)
    ) -> bool {
        // Look up vias by layer pair
        let layer_key = (from_layer.into(), to_layer.into());
        if let Some(vias) = self.vias_by_layers.get(&layer_key) {
            vias.iter().any(|via| {
                // Check net matches
                if via.net_id != net_id {
                    return false;
                }

                // Check if query point is inside via's XY bounding box
                let (min_x, min_y, max_x, max_y) = via.xy_bbox;
                let (qx, qy) = query_xy;
                qx >= min_x && qx <= max_x && qy >= min_y && qy <= max_y
            })
        } else {
            false
        }
    }

    /// Get all vias connecting two layers for a specific net.
    pub fn get_vias_for_layers(
        &self,
        net_id: NetId,
        from_layer: &str,
        to_layer: &str,
    ) -> Vec<&ViaInstance> {
        let layer_key = (from_layer.into(), to_layer.into());
        if let Some(vias) = self.vias_by_layers.get(&layer_key) {
            vias.iter()
                .filter(|v| v.net_id == net_id)
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get all vias for a specific net.
    pub fn get_vias_for_net(&self, net_id: NetId) -> Option<&Vec<ViaInstance>> {
        self.vias_by_net.get(&net_id)
    }

    /// Get total number of registered vias.
    pub fn via_count(&self) -> usize {
        self.vias_by_net.values().map(|v| v.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_query() {
        let mut db = ViaInstanceDatabase::new();
        
        db.register(
            "Via_Source",
            NetId(1),
            "active",
            "metal1",
            (550, 900, 750, 1100), // 200nm square centered at (650, 1000)
            (0, 1650),
        );

        // Should find via at center
        assert!(db.has_via_at(NetId(1), "active", "metal1", (650, 1000)));
        
        // Should find via at edge
        assert!(db.has_via_at(NetId(1), "active", "metal1", (550, 900)));
        
        // Should not find via outside bbox
        assert!(!db.has_via_at(NetId(1), "active", "metal1", (800, 1000)));
        
        // Should not find via on wrong net
        assert!(!db.has_via_at(NetId(2), "active", "metal1", (650, 1000)));
        
        // Should not find via on wrong layers
        assert!(!db.has_via_at(NetId(1), "poly", "metal1", (650, 1000)));
    }

    #[test]
    fn test_get_vias_for_layers() {
        let mut db = ViaInstanceDatabase::new();
        
        db.register("Via1", NetId(1), "active", "metal1", (0, 0, 100, 100), (0, 1000));
        db.register("Via2", NetId(1), "poly", "metal1", (200, 200, 300, 300), (450, 1650));
        db.register("Via3", NetId(2), "active", "metal1", (400, 400, 500, 500), (0, 1000));

        let vias = db.get_vias_for_layers(NetId(1), "active", "metal1");
        assert_eq!(vias.len(), 1);
        assert_eq!(vias[0].entity_name, "Via1");

        let vias = db.get_vias_for_layers(NetId(1), "poly", "metal1");
        assert_eq!(vias.len(), 1);
        assert_eq!(vias[0].entity_name, "Via2");

        let vias = db.get_vias_for_layers(NetId(2), "active", "metal1");
        assert_eq!(vias.len(), 1);
        assert_eq!(vias[0].entity_name, "Via3");
    }
}
