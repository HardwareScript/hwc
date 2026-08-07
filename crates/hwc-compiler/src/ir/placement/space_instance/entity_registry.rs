//! Transform and copy the entity registry, including PhysicalInterface (CIR) metadata.

use crate::ir::errors::IrError;
use hwc_engine::geometry::entity_ids::EntityId;
use hwc_engine::geometry::{BoundingBox, Point3D};
use hwc_engine::geometry_router::connection_interface::{AccessRegion, InterfaceGeometry};
use hwc_engine::geometry_router::entity_graph::{EntityGraph, EntityType};
use hwc_engine::netlist::NetId;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::sync::Arc;

use super::transform::FixedTransform2D;

/// Transform and copy entity registry entries from child to parent
///
/// This enables cross-instance routing by registering child entities with hierarchical names.
/// For example, a child entity "Out_Pad" in instance "PMOS_Inst" becomes "PMOS_Inst.Out_Pad".
///
/// v0.2.1 FIX: Also copies PhysicalInterface (CIR) metadata so the global router
/// can resolve boundary points for cross-instance routes.
pub(super) fn transform_entity_registry(
    child_graph: &EntityGraph,
    parent_graph: &mut EntityGraph,
    transform: &FixedTransform2D,
    net_id_map: &FxHashMap<NetId, NetId>,
    instance_name: &str,
) -> Result<(), IrError> {
    eprintln!(
        "[HIERARCHICAL] Transforming entity registry: {} entities",
        child_graph.iter_entity_ids().count()
    );

    // Build a lookup: child entity name -> PhysicalInterface
    // We'll use this after registering each entity to also transfer its interface.
    let child_interfaces: FxHashMap<compact_str::CompactString, _> = child_graph
        .iter_entity_interfaces()
        .map(|(name, iface)| (name.clone(), iface.clone()))
        .collect();

    for (_child_entity_id, child_entity_data) in child_graph.iter_entity_registry() {
        // FIX v0.2.1: Determine entity type and construct hierarchical names directly
        // from the unhashed child_entity_data properties instead of parsing the debug hash string.
        let (new_id_str, hierarchical_name) = match child_entity_data.entity_type {
            EntityType::SpacePour => {
                // e.g., child name "Out_Pad" in instance "PMOS_Inst"
                // -> EntityId: "space:PMOS_Inst.Out_Pad"
                // -> name field: "PMOS_Inst.Out_Pad"
                let id_str = format!("space:{}.{}", instance_name, child_entity_data.name);
                let name = format!("{}.{}", instance_name, child_entity_data.name);
                (id_str, name)
            }
            EntityType::ComponentPin => {
                // e.g., child name "Via_Source.gate" in instance "PMOS_Inst"
                // -> EntityId: "pin:PMOS_Inst.Via_Source:gate"
                // -> name field: "PMOS_Inst.Via_Source.gate"
                // The child name format is "ComponentName.PinName"
                let name_with_dot = child_entity_data.name.as_str();
                let id_str = if let Some((comp, pin)) = name_with_dot.split_once('.') {
                    format!("pin:{}.{}:{}", instance_name, comp, pin)
                } else {
                    // Fallback if name doesn't have expected format
                    format!("pin:{}:{}", instance_name, name_with_dot)
                };
                let name = format!("{}.{}", instance_name, name_with_dot);
                (id_str, name)
            }
            _ => {
                eprintln!(
                    "[HIERARCHICAL WARN] Skipping un-routable entity type in child space: {:?}",
                    child_entity_data.entity_type
                );
                continue;
            }
        };

        // Create new EntityId with hierarchical name
        let parent_entity_id = EntityId::from_semantic(&new_id_str);

        // Clone and transform the entity data
        let mut parent_entity_data = child_entity_data.clone();

        // Remap the net ID
        if let Some(child_net_id) = child_entity_data.net_id {
            if let Some(&parent_net_id) = net_id_map.get(&child_net_id) {
                parent_entity_data.net_id = Some(parent_net_id);
            } else {
                return Err(IrError::PlacementError(format!(
                    "Entity '{}' has net {:?} with no mapping in net_map",
                    child_entity_data.name, child_net_id
                )));
            }
        }

        // Transform the bounding box
        parent_entity_data.bbox = transform.transform_bbox(&child_entity_data.bbox)?;

        // Update the hierarchical name property inside the metadata
        parent_entity_data.name = hierarchical_name.clone().into();

        eprintln!(
            "[HIERARCHICAL DEBUG] Creating entity - ID: '{}', name: '{}', EntityId hash: {}",
            new_id_str, parent_entity_data.name, parent_entity_id
        );

        // Register in parent's entity registry
        match parent_graph.register_entity_from_data(parent_entity_id, parent_entity_data) {
            Ok(_) => {
                eprintln!(
                    "[HIERARCHICAL] ✓ Successfully registered: '{}' -> '{}' (hash: {})",
                    child_entity_data.name, new_id_str, parent_entity_id
                );
            }
            Err(e) => {
                eprintln!(
                    "[HIERARCHICAL ERROR] ✗ Failed to register: '{}' -> '{}' (hash: {}): {}",
                    child_entity_data.name, new_id_str, parent_entity_id, e
                );
                return Err(IrError::PlacementError(e));
            }
        }

        // v0.2.1: Also transfer PhysicalInterface (CIR) metadata.
        //
        // The child's entity_interface_map stores interfaces keyed by the
        // child entity name (e.g., "Out_Pad"). We need to clone the interface,
        // translate all coordinates by the affine transform, allocate a new
        // InterfaceId in the parent, and register it under the hierarchical name
        // (e.g., "PMOS_Inst.Out_Pad").
        //
        // Without this step, resolve_route_boundary_points() fails with:
        //   "No PhysicalInterface registered for entity 'PMOS_Inst.Out_Pad'"
        let child_entity_name_str: compact_str::CompactString =
            child_entity_data.name.as_str().into();
        if let Some(child_iface) = child_interfaces.get(&child_entity_name_str) {
            // Clone and translate the interface
            let mut parent_iface = child_iface.clone();

            // Allocate a fresh InterfaceId in the parent
            parent_iface.id = parent_graph.allocate_interface_id();

            // Translate InterfaceGeometry coordinates
            parent_iface.geometry = match &child_iface.geometry {
                InterfaceGeometry::Point(p) => {
                    let (tx, ty, tz) = transform.transform_point(p.x, p.y, p.z)?;
                    InterfaceGeometry::Point(Point3D::new(tx, ty, tz))
                }
                InterfaceGeometry::Edge { start, end } => {
                    let (sx, sy, sz) = transform.transform_point(start.x, start.y, start.z)?;
                    let (ex, ey, ez) = transform.transform_point(end.x, end.y, end.z)?;
                    InterfaceGeometry::Edge {
                        start: Point3D::new(sx, sy, sz),
                        end: Point3D::new(ex, ey, ez),
                    }
                }
                InterfaceGeometry::Polygon(vertices) => {
                    let mut new_verts = Vec::with_capacity(vertices.len());
                    for v in vertices {
                        let (tx, ty, tz) = transform.transform_point(v.x, v.y, v.z)?;
                        new_verts.push(Point3D::new(tx, ty, tz));
                    }
                    InterfaceGeometry::Polygon(new_verts)
                }
            };

            // Translate pre-computed AccessRegion entry_points and corridors.
            // boundary_normals stay the same (rotation is 0 for now; extend later if needed).
            let translated_regions: SmallVec<[AccessRegion; 8]> = child_iface
                .access_regions
                .iter()
                .map(|ar| -> Result<AccessRegion, IrError> {
                    let (ex, ey, ez) = transform.transform_point(
                        ar.entry_point.x,
                        ar.entry_point.y,
                        ar.entry_point.z,
                    )?;
                    let (cmin_x, cmin_y, cmin_z) = transform.transform_point(
                        ar.corridor.min.x,
                        ar.corridor.min.y,
                        ar.corridor.min.z,
                    )?;
                    let (cmax_x, cmax_y, cmax_z) = transform.transform_point(
                        ar.corridor.max.x,
                        ar.corridor.max.y,
                        ar.corridor.max.z,
                    )?;
                    Ok(AccessRegion {
                        entry_point: Point3D::new(ex, ey, ez),
                        normal: ar.normal,
                        corridor: BoundingBox::new(
                            Point3D::new(
                                cmin_x.min(cmax_x),
                                cmin_y.min(cmax_y),
                                cmin_z.min(cmax_z),
                            ),
                            Point3D::new(
                                cmin_x.max(cmax_x),
                                cmin_y.max(cmax_y),
                                cmin_z.max(cmax_z),
                            ),
                        ),
                        priority: ar.priority,
                    })
                })
                .collect::<Result<SmallVec<[AccessRegion; 8]>, IrError>>()?;

            parent_iface.access_regions = Arc::new(translated_regions);

            // Register in the parent under the hierarchical entity name
            parent_graph.register_space_entity_interface(hierarchical_name.clone(), parent_iface);

            eprintln!(
                "[HIERARCHICAL] ✓ Transferred PhysicalInterface: '{}' -> '{}'",
                child_entity_data.name, hierarchical_name
            );
        }
    }

    eprintln!(
        "[HIERARCHICAL] Entity registry transformation complete: {} entities added to parent",
        child_graph.iter_entity_ids().count()
    );

    Ok(())
}
