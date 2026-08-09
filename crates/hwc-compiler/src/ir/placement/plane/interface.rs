//! Physical interface registration for plane routing connectivity.
//!
//! v0.1.9 CIR: Registers a `PhysicalInterface` for the plane/pad so the router
//! can query `AccessRegion`s and avoid pad penetration.

use super::super::super::errors::IrError;
use super::super::context::PlacementContext;
use hwc_engine::geometry::BoundingBox;
use hwc_engine::space::HardwareSpace;
use hwc_engine::Point3D;
use hwc_materials::IntentCostWeights;

/// Register a `PhysicalInterface` for the placed plane so the router can treat
/// it as a routing endpoint.
///
/// The polygon winding order is derived from the coordinate system origin
/// (Y-up vs Y-down), and the routing intent is looked up from the profile's
/// `net_type` declarations (no hardcoded fallbacks).
pub fn register_plane_interface(
    space: &mut HardwareSpace,
    plane_name: &str,
    bbox: BoundingBox,
    ctx: &PlacementContext,
) -> Result<(), IrError> {
    use hwc_engine::geometry_router::connection_interface::{
        DefaultRoutingDatabase, InterfaceGeometry, Orientation, PhysicalInterface,
    };
    use hwc_engine::geometry_router::routing_intent::RoutingIntent;
    use hwc_engine::netlist::ComponentId;
    use smallvec::smallvec;

    // Require fabrication constraints - no fallbacks
    let constraints =
        space
            .fabrication_constraints
            .as_ref()
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: "Fabrication constraints required for interface generation".into(),
                hint: "Add a 'trace:' block to your profile with min_width and min_spacing".into(),
            })?;

    let trace_width_nm = constraints.trace.min_width_nm;
    let clearance_nm = constraints.trace.min_spacing_nm;

    let geometry = InterfaceGeometry::Polygon(interface_polygon(bbox));

    let interface_id = space.entity_graph.allocate_interface_id();

    // Routing intent must come from profile net_type declarations.
    // No hardcoded defaults - explicit declarations enforce design intent.
    let profile_def = ctx.profile.ok_or_else(|| IrError::MissingAsicConstraint {
        message: "Cannot register routing interface without a profile".into(),
        hint: "Ensure the space has a profile declaration".into(),
    })?;

    let profile_intents = build_profile_intents(profile_def);

    // Require explicit "Signal" intent declaration - no fallbacks
    let intent = RoutingIntent::lookup("Signal", &profile_intents).ok_or_else(|| {
        IrError::MissingAsicConstraint {
            message: "Profile missing required 'Signal' net_type declaration".into(),
            hint: "Add routing intent to your profile:\n\n\
                   net_type Signal:\n    routing_style: auto\n    escape_stub: 0nm"
                .into(),
        }
    })?;

    let db = DefaultRoutingDatabase::default();
    let pseudo_component_id = ComponentId::new(0xFFFF_0000 + interface_id.raw());

    // Planes always use Derived orientation because the polygon winding
    // (determined by space origin) encodes the correct outward direction.
    let interface = PhysicalInterface::new(
        hwc_engine::geometry_router::connection_interface::PhysicalInterfaceParams {
            id: interface_id,
            component_id: pseudo_component_id,
            geometry,
            capabilities: smallvec![],
            routing_intent: intent,
            orientation: Some(Orientation::Derived),
            trace_width_nm,
            escape_stub_length_nm: clearance_nm * 2,
        },
        &db,
    );

    space
        .entity_graph
        .register_space_entity_interface(plane_name.to_string(), interface);

    Ok(())
}

/// Build the interface polygon vertices for the plane bounding box.
///
/// # v0.1.9.1 CRITICAL: middle-Z for Zero-Gap Z Lock alignment
///
/// PROBLEM: Previously `bbox.min.z` (bottom Z, e.g. 960nm) was used for
/// interface vertices, but routing queries occur at the trace centerline
/// (middle Z, e.g. 1160nm). That Z mismatch caused:
///   1. `AccessRegion` escape points at the wrong Z (960nm instead of 1160nm)
///   2. Boundary resolution creating routes with Z discontinuities
///   3. Spatial index queries missing obstacles (query at 1160, obstacles at 960)
///
/// SOLUTION: Register interface geometry at the middle Z to match where routing
/// occurs, ensuring perfect Z alignment between placement and routing phases.
///
/// v0.2.1: The canonical Bottom-Left origin means Y always increases upward,
/// so vertices always use CCW winding.
fn interface_polygon(bbox: BoundingBox) -> Vec<Point3D> {
    let middle_z_nm = (bbox.min.z + bbox.max.z) / 2;

    // CCW winding (Y increases upward)
    vec![
        Point3D::new(bbox.min.x, bbox.min.y, middle_z_nm), // bottom-left
        Point3D::new(bbox.max.x, bbox.min.y, middle_z_nm), // bottom-right
        Point3D::new(bbox.max.x, bbox.max.y, middle_z_nm), // top-right
        Point3D::new(bbox.min.x, bbox.max.y, middle_z_nm), // top-left
    ]
}

/// Build the routing intent lookup table from the profile's `net_type`
/// declarations.
fn build_profile_intents(
    profile_def: &hwc_parser::ProfileDefinition,
) -> Vec<hwc_engine::geometry_router::routing_intent::RoutingIntent> {
    use hwc_engine::geometry_router::routing_intent::RoutingIntent;

    profile_def
        .intents
        .iter()
        .map(|pi| {
            RoutingIntent::from_profile_data(
                pi.name.as_str(),
                pi.routing_style.as_ref().map(|id| id.as_str()),
                pi.cost_weights
                    .as_ref()
                    .map(|cw| IntentCostWeights {
                        base_cost: cw.base,
                        via_penalty: cw.via_penalty,
                        direction_penalty: cw.direction_penalty,
                        tight_clearance_penalty: cw.tight_clearance_penalty,
                        crosstalk_penalty: cw.crosstalk_penalty,
                        impedance_penalty: cw.impedance_penalty,
                        reference_void_penalty: cw.reference_void_penalty,
                    })
                    .as_ref(),
                pi.escape_stub
                    .as_ref()
                    .and_then(|meas| meas.to_picometers_i64().map(|pm| pm / 1000)),
            )
        })
        .collect()
}
