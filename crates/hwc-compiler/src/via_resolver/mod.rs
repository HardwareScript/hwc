//! Native Via Resolver (v0.1.8)
//!
//! This module replaces the legacy AutoViaInserter with a native, data-driven
//! system that resolves vertical connectivity using the StackupManager and
//! BridgeRegistry.

pub mod library;

use crate::ir::errors::IrError;
use crate::ir::stackup_manager::StackupManager;
use hwc_engine::{geometry_router::Via, HardwareSpace, Point3D};
use library::{ViaLibrary, ViaStackRequest, ViaType};

/// Shared context for a single net's layer-bridging operation.
///
/// Carries the parameters that are constant across every layer transition
/// for one net, so `bridge_layers` / `insert_via_stack` only receive the
/// per-transition values.
struct ViaBridgeContext<'a> {
    space: &'a HardwareSpace,
    net_id: hwc_engine::netlist::NetId,
    stackup_manager: &'a StackupManager,
}

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
        eval_context: &hwc_parser::EvaluationContext,
    ) -> Result<Self, IrError> {
        // v0.2.0: Query bridges from global symbol table (first-class definitions)
        let bridge_table =
            crate::bridge_resolver::BridgeTable::from_profile_and_symbol_table(profile, Some(symbol_table));

        let library = ViaLibrary::from_profile(
            profile,
            stackup_manager,
            &bridge_table,
            None,
            Some(symbol_table),
        )?;

        let min_spacing_nm = profile
            .and_then(|p| p.via.as_ref())
            .and_then(|v| v.min_spacing.as_ref())
            .and_then(|m| crate::ir::conversions::measurement_to_nm(m, symbol_table, eval_context).ok())
            .ok_or_else(|| {
                IrError::MissingProfileConstraint {
                    field: "via.min_spacing".to_string()
                }
            })?;

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
                    if stackup_manager
                        .is_layer_conductive(&stackup_manager.ordered_layers()[layer_idx])
                    {
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

                let ctx = ViaBridgeContext {
                    space,
                    net_id,
                    stackup_manager,
                };

                self.bridge_layers(&ctx, from_layer, to_layer, &mut new_vias)?;
            }
        }

        // 5. Commit all new vias to the space
        space.add_vias(new_vias);

        Ok(())
    }

    fn bridge_layers(
        &self,
        ctx: &ViaBridgeContext,
        from_layer: usize,
        to_layer: usize,
        new_vias: &mut Vec<Via>,
    ) -> Result<(), IrError> {
        let ViaBridgeContext {
            space,
            net_id,
            stackup_manager,
        } = *ctx;
        

        // v0.2.0 STRUCTURAL FIX: Query only routable surfaces (pours), not bridges (contacts).
        // The EntityGraph now provides get_pours_for_net() which excludes Contact-type elements.
        // This prevents vias that span multiple layers from triggering duplicate via insertion.
        let all_elements = space.entity_graph.get_pours_for_net(net_id);

        println!(
            "   DEBUG: get_pours_for_net({:?}) returned {} total elements",
            net_id,
            all_elements.len()
        );
        for (idx, el) in all_elements.iter().enumerate() {
            let mid_z = (el.bbox.min.z + el.bbox.max.z) / 2;
            let layer_idx = stackup_manager.get_layer_index_at_z(mid_z);
            let mat_name = space.material_registry.get_name(el.material).unwrap_or("Unknown");
            println!(
                "     [{}] layer_type={:?}, mid_z={}nm, layer_idx={:?}, material={}, bbox={:?}",
                idx, el.layer_type, mid_z, layer_idx, mat_name, el.bbox
            );
        }

        let from_elements: Vec<&hwc_engine::geometry_router::substrate_types::SubstrateLayer> =
            all_elements
                .iter()
                .filter(|el| {
                    let mid_z = (el.bbox.min.z + el.bbox.max.z) / 2;
                    stackup_manager.get_layer_index_at_z(mid_z) == Some(from_layer)
                })
                .collect();

        let to_elements: Vec<&hwc_engine::geometry_router::substrate_types::SubstrateLayer> =
            all_elements
                .iter()
                .filter(|el| {
                    let mid_z = (el.bbox.min.z + el.bbox.max.z) / 2;
                    stackup_manager.get_layer_index_at_z(mid_z) == Some(to_layer)
                })
                .collect();

        println!(
            "   Found {} pours on layer {}, {} pours on layer {}",
            from_elements.len(),
            from_layer,
            to_elements.len(),
            to_layer
        );

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
                    // XY overlap found — query ViaInstanceDatabase to check if explicit via exists.
                    let center_x = (xy_x_min + xy_x_max) / 2;
                    let center_y = (xy_y_min + xy_y_max) / 2;

                    // v0.2.0 DATABASE-DRIVEN: Query ViaInstanceDatabase for existing explicit vias.
                    let from_layer_name = &stackup_manager.ordered_layers()[from_layer];
                    let to_layer_name = &stackup_manager.ordered_layers()[to_layer];
                    
                    println!(
                        "   [VIA CHECK] XY overlap at ({}, {}) between {} (layer {}) and {} (layer {})",
                        center_x, center_y, from_layer_name, from_layer, to_layer_name, to_layer
                    );
                    println!(
                        "   [VIA CHECK] Querying ViaInstanceDatabase for net {:?}: {} -> {}",
                        net_id, from_layer_name, to_layer_name
                    );
                    
                    if space.via_instance_db.has_via_at(
                        net_id,
                        from_layer_name,
                        to_layer_name,
                        (center_x, center_y),
                    ) {
                        println!(
                            "   ✓ Skipping auto-via at ({}, {}) - explicit contact already bridges {} and {}",
                            center_x, center_y, from_layer_name, to_layer_name
                        );
                        continue;
                    }
                    
                    println!(
                        "   ✗ No explicit via found at ({}, {}) - will attempt auto-insertion",
                        center_x, center_y
                    );

                    // No explicit via found — insert automatic via
                    // v0.1.8: Resolve materials for the specific elements being bridged.
                    // This ensures we use the correct bridge rule even if the layer's
                    // default material is different (e.g. Silicon_P on an 'active' layer).
                    let from_material = space
                        .material_registry
                        .get_name(from_el.material)
                        .unwrap_or("Unknown")
                        .to_string();
                    let to_material = space
                        .material_registry
                        .get_name(to_el.material)
                        .unwrap_or("Unknown")
                        .to_string();

                    self.insert_via_stack(
                        ctx,
                        ViaStackRequest {
                            x: center_x,
                            y: center_y,
                            from_layer,
                            to_layer,
                            from_material: from_material.into(),
                            to_material: to_material.into(),
                        },
                        new_vias,
                    )?;
                }
            }
        }

        Ok(())
    }

    fn insert_via_stack(
        &self,
        ctx: &ViaBridgeContext,
        request: ViaStackRequest,
        new_vias: &mut Vec<Via>,
    ) -> Result<(), IrError> {
        let ViaStackRequest {
            x,
            y,
            from_layer,
            to_layer,
            from_material,
            to_material,
        } = request;
       

        let ViaBridgeContext {
            net_id,
            stackup_manager,
            ..
        } = *ctx;
        let via_def = self.library.find_via(
            from_layer,
            to_layer,
            &from_material,
            &to_material,
            stackup_manager,
        )?;

       

        let via = hwc_engine::geometry_router::Via::new(hwc_engine::geometry_router::ViaSpec {
            position: (x, y),
            from_z_nm: via_def.z_start_nm,
            to_z_nm: via_def.z_end_nm,
            diameter_nm: (via_def.diameter_mm * 1_000_000.0) as i64,
            net_id,
            material_id: 0, // material_id will be filled in by the engine
            annular_ring_nm: 0,
            board_min_z_nm: via_def.z_start_nm,
            board_max_z_nm: via_def.z_end_nm,
        });
        new_vias.push(via);

        Ok(())
    }

    fn _is_colliding(
        &self,
        space: &HardwareSpace,
        x: i64,
        y: i64,
        via_type: &ViaType,
        net_id: hwc_engine::netlist::NetId,
    ) -> bool {
        let radius = (via_type.diameter_mm * 1_000_000.0) as i64 / 2;
        let query_bbox = hwc_engine::geometry::BoundingBox::new(
            Point3D::new(
                x - radius - self._min_spacing_nm,
                y - radius - self._min_spacing_nm,
                via_type.z_start_nm,
            ),
            Point3D::new(
                x + radius + self._min_spacing_nm,
                y + radius + self._min_spacing_nm,
                via_type.z_end_nm,
            ),
        );

        // Query the global spatial index for any elements in this volume
        let collisions = space.entity_graph.query_bbox(&query_bbox);

        for col in collisions {
            // Ignore elements on the same net (vias can overlap their own pads/pours)
            if col.net == net_id {
                continue;
            }
            return true;
        }

        false
    }
}
