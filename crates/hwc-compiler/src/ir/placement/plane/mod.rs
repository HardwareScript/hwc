//! Plane placement orchestration.
//!
//! `place_plane` is the public entry point. It resolves the plane's elevation
//! and XY geometry, then delegates to focused submodules for interface
//! registration, layer-connection registration, collision checking, netlist
//! registration, and cutout carving.
//!
//! Submodules:
//! - [`elevation`] - layer name, thickness, and Z extent resolution
//! - [`shape`] - shape instance dimension resolution (v0.1.9 syntax)
//! - [`geometry`] - XY extent resolution from shape or `from:`/`to:` corners
//! - [`interface`] - `PhysicalInterface` registration for routing
//! - [`connection`] - layer connection database surface registration
//! - [`collision`] - substrate and pour interpenetration checks
//! - [`netlist`] - pour metadata and netlist component registration
//! - [`cutouts`] - cutout resolution and carving

mod collision;
mod connection;
mod cutouts;
mod elevation;
mod geometry;
mod interface;
mod netlist;
mod shape;

pub use elevation::ResolvedElevation;
pub use geometry::ResolvedGeometry;

use collision::check_plane_collisions;
use connection::register_plane_surface;
use cutouts::{apply_cutouts, resolve_cutouts};
use elevation::resolve_elevation;
use geometry::resolve_plane_geometry;
use interface::register_plane_interface;
use netlist::{push_plane_metadata, register_plane_netlist};

use super::super::errors::IrError;
use super::context::PlacementContext;
use crate::bounding_box_tracker::BoundingBoxTracker;
use hwc_engine::geometry::BoundingBox;
use hwc_engine::geometry_router::entity_graph::SubstrateLayerType;
use hwc_engine::space::HardwareSpace;
use hwc_engine::{NetId, Point3D};
use hwc_parser::PlanePlacement;

/// Place a plane into the hardware space.
///
/// Resolves geometry and elevation, registers the plane with the bounding box
/// tracker, entity graph, routing interface database, layer connection
/// database, and netlist, validates it against the substrate and existing
/// pours, and finally carves out any declared cutouts.
pub fn place_plane(
    space: &mut HardwareSpace,
    plane: &PlanePlacement,
    bbox_tracker: &mut BoundingBoxTracker,
    ctx: &PlacementContext,
) -> Result<(), IrError> {
    let material_id = space
        .material_registry
        .get_id(&plane.material)
        .ok_or_else(|| IrError::UndeclaredMaterial {
            material: plane.material.clone(),
        })?;

    let ResolvedElevation {
        layer_name,
        z_start_nm,
        z_end_nm,
    } = resolve_elevation(plane, ctx)?;

    // v0.2.1: bbox_tracker is passed through for anchor arithmetic evaluation
    let ResolvedGeometry {
        start,
        end,
        area_nm2,
    } = resolve_plane_geometry(plane, &space.dimensions, bbox_tracker, ctx)?;

    // Resolve cutouts while the bbox tracker is still free of this plane's own
    // registration, matching the original single-pass resolution order.
    let resolved_cutouts = resolve_cutouts(plane, &space.dimensions, bbox_tracker, ctx)?;

    let start_with_z = Point3D::new(start.x, start.y, z_start_nm);
    let end_with_z = Point3D::new(end.x, end.y, z_end_nm);
    let bbox = BoundingBox::new(start_with_z, end_with_z);

    bbox_tracker.register(plane.name.to_string(), bbox, start_with_z);

    // v0.1.8: Register plane in EntityGraph for O(1) resolution
    let net_id = if let Some(net_name) = &plane.net {
        require_trace_constraints(space)?;
        Some(space.netlist.get_or_create_net(&net_name.base))
    } else {
        None
    };

    space
        .entity_graph
        .register_space_entity(&plane.name.base, bbox, net_id, z_start_nm);

    // v0.1.9 CIR: Register PhysicalInterface so the router can query
    // AccessRegions and avoid pad penetration.
    register_plane_interface(space, plane.name.base.as_str(), bbox, ctx)?;

    log_plane_registration(plane, start_with_z, end_with_z);

    // Note: Substrate layer registration happens after netlist processing (see
    // below) to ensure we have the correct resolved net_id.
    check_plane_collisions(space, plane, bbox, material_id, z_start_nm)?;

    let resolved_net_name = plane.net.as_ref().map(|n| n.base.clone());

    push_plane_metadata(
        space,
        plane,
        resolved_net_name.clone(),
        z_start_nm,
        area_nm2,
        bbox,
    );

    let net_id = register_plane_netlist(
        space,
        plane,
        resolved_net_name.as_ref(),
        start_with_z,
        end_with_z,
        material_id,
    );

    // v0.2.0: Register plane surface in layer connection database
    register_plane_surface(
        space,
        &plane.name.base,
        &layer_name,
        start_with_z,
        end_with_z,
        material_id,
    );

    // v0.1.9: Register as substrate layer so routing can see it as an obstacle.
    // Planes with net_id are conductive pours; planes without are keepout zones.
    register_substrate_layer(space, plane, bbox, material_id, net_id);

    apply_cutouts(space, resolved_cutouts, z_start_nm, z_end_nm);

    Ok(())
}

/// Require the PDK to declare `trace.min_width_nm` before a netted plane may be
/// registered.
fn require_trace_constraints(space: &HardwareSpace) -> Result<(), IrError> {
    space
        .fabrication_constraints
        .as_ref()
        .map(|c| c.trace.min_width_nm)
        .ok_or_else(|| IrError::MissingAsicConstraint {
            message: "PDK missing required 'trace.min_width_nm' constraint".into(),
            hint: "Add a 'trace:' block to your profile with explicit min_width.\n\nExample:\n  trace:\n    min_width: 180nm".into(),
        })?;
    Ok(())
}

/// Register the plane as a substrate layer obstacle in the entity graph.
fn register_substrate_layer(
    space: &mut HardwareSpace,
    plane: &PlanePlacement,
    bbox: BoundingBox,
    material_id: u8,
    net_id: u32,
) {
    eprintln!(
        "[SUBSTRATE DEBUG] Adding plane '{}' as substrate layer: material_id={}, net={}, bbox=({},{},{}) to ({},{},{})",
        plane.name,
        material_id,
        net_id,
        bbox.min.x,
        bbox.min.y,
        bbox.min.z,
        bbox.max.x,
        bbox.max.y,
        bbox.max.z
    );

    space.entity_graph.add_substrate_layer(
        material_id,
        NetId::new(net_id),
        bbox,
        SubstrateLayerType::Pour,
    );
}

/// Emit the human-readable placement log line for the registered plane.
fn log_plane_registration(plane: &PlanePlacement, start_with_z: Point3D, end_with_z: Point3D) {
    println!(
        "   ├─ Registered plane '{}' bbox: min=({:.3}, {:.3}, {:.3}) max=({:.3}, {:.3}, {:.3})",
        plane.name,
        start_with_z.x as f64 / 1_000_000.0,
        start_with_z.y as f64 / 1_000_000.0,
        start_with_z.z as f64 / 1_000_000.0,
        end_with_z.x as f64 / 1_000_000.0,
        end_with_z.y as f64 / 1_000_000.0,
        end_with_z.z as f64 / 1_000_000.0,
    );
}
