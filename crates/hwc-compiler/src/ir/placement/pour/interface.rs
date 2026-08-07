//! Physical interface registration for pour routing connectivity.

use super::super::super::errors::IrError;
use super::super::context::PlacementContext;
use hwc_engine::geometry::BoundingBox;
use hwc_engine::space::HardwareSpace;
use hwc_engine::Point3D;
use hwc_materials::IntentCostWeights;
use hwc_parser::OriginXY;

/// Register a `PhysicalInterface` for the placed pour so the router can connect
/// to it as a routing endpoint.
///
/// The interface polygon winding order is derived from the coordinate system
/// origin (Y-up vs Y-down), and the routing intent is looked up from the
/// profile's `net_type` declarations (no hardcoded fallbacks).
#[allow(clippy::too_many_lines)]
pub fn register_pour_interface(
    space: &mut HardwareSpace,
    pour_name: &str,
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

    // Calculate middle Z for alignment with routing queries
    let middle_z_nm = (bbox.min.z + bbox.max.z) / 2;

    // Determine vertex winding order based on coordinate system origin
    let is_y_upward = matches!(ctx.origin.xy, OriginXY::BL | OriginXY::BR);

    let geometry = if is_y_upward {
        // CCW winding for Y-up coordinate systems (BL, BR)
        InterfaceGeometry::Polygon(vec![
            Point3D::new(bbox.min.x, bbox.min.y, middle_z_nm), // bottom-left
            Point3D::new(bbox.max.x, bbox.min.y, middle_z_nm), // bottom-right
            Point3D::new(bbox.max.x, bbox.max.y, middle_z_nm), // top-right
            Point3D::new(bbox.min.x, bbox.max.y, middle_z_nm), // top-left
        ])
    } else {
        // CW winding for Y-down coordinate systems (TL, TR)
        InterfaceGeometry::Polygon(vec![
            Point3D::new(bbox.min.x, bbox.min.y, middle_z_nm), // top-left
            Point3D::new(bbox.min.x, bbox.max.y, middle_z_nm), // bottom-left
            Point3D::new(bbox.max.x, bbox.max.y, middle_z_nm), // bottom-right
            Point3D::new(bbox.max.x, bbox.min.y, middle_z_nm), // top-right
        ])
    };

    let interface_id = space.entity_graph.allocate_interface_id();

    // Routing intent must come from profile net_type declarations
    // No hardcoded defaults - explicit declarations enforce design intent
    let profile_def = ctx.profile.ok_or_else(|| IrError::MissingAsicConstraint {
        message: "Cannot register routing interface without a profile".into(),
        hint: "Ensure the space has a profile declaration".into(),
    })?;

    // Build intent lookup table from profile
    let profile_intents: Vec<RoutingIntent> = profile_def
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
        .collect();

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

    // Pours use Derived orientation - polygon winding encodes the correct outward direction
    let interface = PhysicalInterface::new(
        interface_id,
        pseudo_component_id,
        geometry,
        smallvec![],
        intent,
        Orientation::Derived,
        &db,
        trace_width_nm,
        clearance_nm * 2,
    );

    space
        .entity_graph
        .register_space_entity_interface(pour_name.to_string(), interface);

    Ok(())
}
