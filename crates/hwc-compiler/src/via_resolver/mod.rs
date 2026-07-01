//! Native Via Resolver (v0.1.8)
//!
//! This module replaces the legacy AutoViaInserter with a native, data-driven
//! system that resolves vertical connectivity using the StackupManager and
//! BridgeRegistry.

pub mod library;

use crate::ir::errors::IrError;
use crate::ir::stackup_manager::StackupManager;
use hwc_engine::{HardwareSpace, Point3D};
use library::{ViaLibrary, ViaType};

/// The ViaResolver is responsible for ensuring physical continuity between
/// conductive layers by inserting vias or contacts where nets transition Z-layers.
pub struct ViaResolver {
    library: ViaLibrary,
    _min_spacing_nm: i64,
}

impl ViaResolver {
    /// Create a new ViaResolver from the project profile.
    pub fn from_profile(
        profile: Option<&hwc_parser::ProfileDefinition>,
        stackup_manager: &StackupManager,
        symbol_table: &crate::SymbolTable,
    ) -> Result<Self, IrError> {
        let bridge_table = if let Some(p) = profile {
            crate::bridge_resolver::BridgeTable::from_profile(p)
        } else {
            crate::bridge_resolver::BridgeTable::new()
        };

        let library = ViaLibrary::from_profile(profile, stackup_manager, &bridge_table, None, Some(symbol_table))?;
        
        let min_spacing_nm = profile
            .and_then(|p| p.via.as_ref())
            .and_then(|v| v.min_spacing.as_ref())
            .and_then(|m| crate::ir::conversions::measurement_to_nm(m, symbol_table).ok())
            .unwrap_or(200_000);

        Ok(Self {
            library,
            _min_spacing_nm: min_spacing_nm,
        })
    }

    /// Resolve all missing vertical connections in the space.
    pub fn resolve_connectivity(
        &self,
        space: &mut HardwareSpace,
        stackup_manager: &StackupManager,
    ) -> Result<(), IrError> {
        let mut new_vias = Vec::new();

        // 1. Identify all nets that have conductive elements on multiple layers
        let nets_to_resolve = space.netlist.all_net_ids();

        for net_id in nets_to_resolve {
            let net_name = space.netlist.get_net_name(net_id).unwrap_or_else(|| "unnamed".into());
            
            // 2. Query the EntityGraph for all conductive elements on this net
            let elements = space.entity_graph.get_all_elements_for_net(net_id);
            if elements.is_empty() {
                continue;
            }

            // 3. Group elements by conductive horizon (Layer)
            let mut layers_with_net = std::collections::HashSet::new();
            for el in &elements {
                let mid_z = (el.bbox.min.z + el.bbox.max.z) / 2;
                if let Some(layer_idx) = stackup_manager.get_layer_index_at_z(mid_z) {
                    if stackup_manager.is_layer_conductive(&stackup_manager.ordered_layers()[layer_idx]) {
                        layers_with_net.insert(layer_idx);
                    }
                }
            }

            if layers_with_net.len() < 2 {
                continue;
            }

            // 4. For each layer transition, find overlapping regions and insert vias
            let mut sorted_layers: Vec<usize> = layers_with_net.into_iter().collect();
            sorted_layers.sort();

            for window in sorted_layers.windows(2) {
                let from_layer = window[0];
                let to_layer = window[1];

                self.bridge_layers(
                    space,
                    net_id,
                    &net_name,
                    from_layer,
                    to_layer,
                    stackup_manager,
                    &mut new_vias,
                )?;
            }
        }

        // 5. Commit all new vias to the space
        space.add_vias(new_vias);

        Ok(())
    }

    fn bridge_layers(
        &self,
        space: &HardwareSpace,
        net_id: hwc_engine::netlist::NetId,
        net_name: &str,
        from_layer: usize,
        to_layer: usize,
        stackup_manager: &StackupManager,
        new_vias: &mut Vec<hwc_engine::geometry_router::Via>,
    ) -> Result<(), IrError> {
        println!("\n🔍 [VIA RESOLVER] bridge_layers called for net '{}'", net_name);
        println!("   Layer {} to Layer {}", from_layer, to_layer);
        
        // v0.1.8: Get ALL elements for the net and filter them by layer index manually.
        // This compensates for the EntityGraph's lack of layer awareness.
        let all_elements = space.entity_graph.get_all_elements_for_net(net_id);
        
        let from_elements: Vec<&hwc_engine::geometry_router::substrate_types::SubstrateLayer> = all_elements.iter()
            .filter(|el| {
                let mid_z = (el.bbox.min.z + el.bbox.max.z) / 2;
                stackup_manager.get_layer_index_at_z(mid_z) == Some(from_layer)
            })
            .collect();

        let to_elements: Vec<&hwc_engine::geometry_router::substrate_types::SubstrateLayer> = all_elements.iter()
            .filter(|el| {
                let mid_z = (el.bbox.min.z + el.bbox.max.z) / 2;
                stackup_manager.get_layer_index_at_z(mid_z) == Some(to_layer)
            })
            .collect();
        
        println!("   Found {} elements on layer {}, {} elements on layer {}", 
            from_elements.len(), from_layer, to_elements.len(), to_layer);

        for from_el in &from_elements {
            for to_el in &to_elements {
                // v0.1.8: Via placement is a 2D XY operation, per the NativeViaResolver spec
                // (§2.2: "Pass 2 fetches all PlanarIslands (2D contours) at the transition
                // coordinates (X, Y)"). Stackup layers always have disjoint Z-ranges by design,
                // so a 3D bbox intersection will always return None. We check only XY overlap.
                let xy_x_min = from_el.bbox.min.x.max(to_el.bbox.min.x);
                let xy_x_max = from_el.bbox.max.x.min(to_el.bbox.max.x);
                let xy_y_min = from_el.bbox.min.y.max(to_el.bbox.min.y);
                let xy_y_max = from_el.bbox.max.y.min(to_el.bbox.max.y);

                if xy_x_min < xy_x_max && xy_y_min < xy_y_max {
                    // XY overlap found — place a via at the centroid of the overlap region.
                    let center_x = (xy_x_min + xy_x_max) / 2;
                    let center_y = (xy_y_min + xy_y_max) / 2;

                    // v0.1.8: Resolve materials for the specific elements being bridged.
                    // This ensures we use the correct bridge rule even if the layer's
                    // default material is different (e.g. Silicon_P on an 'active' layer).
                    let from_material = space.material_registry.get_name(from_el.material)
                        .unwrap_or("Unknown").to_string();
                    let to_material = space.material_registry.get_name(to_el.material)
                        .unwrap_or("Unknown").to_string();

                    self.insert_via_stack(
                        space,
                        center_x,
                        center_y,
                        from_layer,
                        to_layer,
                        &from_material,
                        &to_material,
                        net_id,
                        net_name,
                        stackup_manager,
                        new_vias,
                    )?;
                }
            }
        }

        Ok(())
    }

    fn insert_via_stack(
        &self,
        _space: &HardwareSpace,
        x: i64,
        y: i64,
        from_layer: usize,
        to_layer: usize,
        from_material: &str,
        to_material: &str,
        net_id: hwc_engine::netlist::NetId,
        _net_name: &str,
        stackup_manager: &StackupManager,
        new_vias: &mut Vec<hwc_engine::geometry_router::Via>,
    ) -> Result<(), IrError> {
        eprintln!("\n🔍 [VIA RESOLVER] Attempting to bridge:");
        eprintln!("   From: {} (Layer {})", from_material, from_layer);
        eprintln!("   To: {} (Layer {})", to_material, to_layer);
        eprintln!("   Available vias in library: {}", self.library.vias.len());

        let via_def = self.library.find_via(from_layer, to_layer, from_material, to_material, stackup_manager)?;

        eprintln!("   ✅ Selected via: {} -> {} (Layer {} -> {})", 
            via_def.from_material, via_def.to_material, via_def.from_layer, via_def.to_layer);
        
        let via = hwc_engine::geometry_router::Via::new(
            (x, y),
            via_def.z_start_nm,
            via_def.z_end_nm,
            (via_def.diameter_mm * 1_000_000.0) as i64,
            net_id,
            0, // material_id will be filled in by the engine
            via_def.z_start_nm,
            via_def.z_end_nm,
            0,
        );
        new_vias.push(via);

        Ok(())
    }

    fn _is_colliding(&self, space: &HardwareSpace, x: i64, y: i64, via_type: &ViaType, net_id: hwc_engine::netlist::NetId) -> bool {
        let radius = (via_type.diameter_mm * 1_000_000.0) as i64 / 2;
        let query_bbox = hwc_engine::geometry::BoundingBox::new(
            Point3D::new(x - radius - self._min_spacing_nm, y - radius - self._min_spacing_nm, via_type.z_start_nm),
            Point3D::new(x + radius + self._min_spacing_nm, y + radius + self._min_spacing_nm, via_type.z_end_nm),
        );

        // Query the global spatial index for any elements in this volume
        let collisions = space.entity_graph.query_bbox(&query_bbox);
        
        for col in collisions {
            // Ignore elements on the same net (vias can overlap their own pads/pours)
            if col.net == net_id.raw() {
                continue;
            }
            return true;
        }

        false
    }
}
