pub mod coordinates;
pub mod mounting;
pub mod unrolling;
pub mod validation;

use super::super::errors::IrError;
use super::context::PlacementContext;
use super::helpers::parse_rectangle_dimensions;
use super::module::place_module_instance;
use hwc_engine::geometry::{BoundingBox, Point3D};
use hwc_engine::HardwareSpace;

pub fn place_component(
    space: &mut HardwareSpace,
    component: &hwc_parser::ComponentPlacement,
    layouts: &[hwc_parser::ModuleLayoutBlock],
    bbox_tracker: &mut crate::bounding_box_tracker::BoundingBoxTracker,
    ctx: &PlacementContext,
) -> Result<(), IrError> {
    if let Some(array_config) = &component.array_config {
        return super::array::place_component_array(
            space,
            component,
            array_config,
            layouts,
            bbox_tracker,
            ctx,
        );
    }

    if ctx
        .symbol_table
        .has_module(component.component_type.as_str())
    {
        return place_module_instance(space, component, layouts, bbox_tracker, ctx);
    }

    let placement_intent =
        coordinates::resolve_position(component, bbox_tracker, ctx.eval_context)?;

    // Use raw point for mounting/elevation (semantics don't matter here)
    let mut position = placement_intent.point();

    let mut mounting_res = mounting::resolve_mounting_and_elevation(
        space,
        component,
        ctx.symbol_table,
        ctx.eval_context,
        ctx.stackup_manager,
        position,
        ctx.origin,
    )?;
    position = mounting_res.position;

    if component.waivers.snap_to_surface {
        mounting::handle_snap_to_surface(space, &mut position);
        mounting_res.position = position;
    }

    // Get component dimensions for bounding box and centering calculation
    let (width_nm, height_nm, depth_nm) = if let Ok(component_def) = ctx
        .symbol_table
        .get_component(component.component_type.as_str())
    {
        if let Some(layout) = &component_def.layout {
            if let Some(shape_str) = &layout.shape {
                let resolved_shape = super::helpers::resolve_parameterized_shape(
                    shape_str,
                    &component.parameters,
                    ctx.symbol_table,
                    ctx.eval_context,
                );
                if let Some(resolved) = resolved_shape {
                    parse_rectangle_dimensions(&resolved, ctx.symbol_table).unwrap_or_default()
                } else {
                    (0, 0, 0)
                }
            } else {
                (0, 0, 0)
            }
        } else {
            (0, 0, 0)
        }
    } else {
        (0, 0, 0)
    };

    // NATIVE FIX: Calculate actual origin using placement intent
    // This is where centering happens - in ONE place, cleanly
    let mut untransformed_origin = placement_intent.calculate_origin(width_nm, height_nm, depth_nm);

    if component.elevation.is_some() || component.waivers.snap_to_surface {
        untransformed_origin.z = position.z;
    }

    let rotation_deg = component.rotation.as_ref()
        .map(|r| r.angle)
        .ok_or_else(|| {
            let is_asic = space.fabrication_constraints.as_ref().is_some_and(|c| {
                c.technology.is_asic()
            });

            if is_asic {
                IrError::MissingAsicConstraintWithSpan {
                    message: format!("Component '{}' missing required rotation.",
                        component.name.as_ref().map(|n| n.as_str()).unwrap_or("unnamed")),
                    hint: "Under ASIC technology, add 'rotated <Angle>' after the position (e.g., 'at [x: 1mm, y: 2mm] rotated 0deg').".into(),
                    span: miette::SourceSpan::new(
                        component.span.start.into(),
                        (component.span.end - component.span.start).into(),
                    ),
                }
            } else {
                IrError::PlacementError(format!("Component '{}' missing required rotation.",
                    component.name.as_ref().map(|n| n.as_str()).unwrap_or("unnamed")))
            }
        })?;
    let z_val = untransformed_origin.z / 1_000_000; // Use mm-scale for name if needed
    let name = component
        .name
        .as_ref()
        .map(|n| n.to_string())
        .unwrap_or_else(|| format!("{}_{}", component.component_type, z_val).into());

    let _engine_position =
        hwc_engine::geometry::Point3D::new(position.x, position.y, mounting_res.body_min_z);

    // Add component to netlist arena (v0.1.8 replacement for ComponentPlacer)
    let component_id = space.netlist.add_component(
        name.clone(),
        component.component_type.to_string().into(),
        (position.x, position.y, position.z),
    );

    // Register pins in netlist arena so they can be connected during unrolling
    if let Ok(component_def) = ctx
        .symbol_table
        .get_component(component.component_type.as_str())
    {
        for pin_name in &component_def.pins {
            space.netlist.add_pin(
                component_id,
                pin_name.clone(),
                (0, 0, 0), // Local offsets are handled during analytic unrolling
                None,
            );
        }
    }

    // v0.1.9: Register physical interfaces for each component pin.
    // Each pin gets a Point-geometry interface for CIR candidate selection.
    if let Ok(component_def) = ctx
        .symbol_table
        .get_component(component.component_type.as_str())
    {
        use hwc_engine::geometry_router::connection_interface::{
            DefaultRoutingDatabase, InterfaceGeometry,
        };
        use hwc_engine::geometry_router::routing_intent::RoutingIntent;
        use smallvec::smallvec;

        let db = DefaultRoutingDatabase::default();
        // v0.1.9 CIR: Require fabrication constraints - no fallbacks
        let trace_width_nm = space
            .fabrication_constraints
            .as_ref()
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: "Fabrication constraints required for interface generation".into(),
                hint: "Add a 'trace:' block to your profile with min_width".into(),
            })?
            .trace
            .min_width_nm;

        for pin_name in &component_def.pins {
            let pin_pos = component_def
                .layout
                .as_ref()
                .and_then(|l| l.pin_positions.get(pin_name.as_str()))
                .map(|pp| {
                    let x_nm = (pp.x * 1_000_000.0) as i64;
                    let y_nm = (pp.y * 1_000_000.0) as i64;
                    let z_nm = pp.z.map(|z| (z * 1_000_000.0) as i64).unwrap_or(0);
                    hwc_engine::geometry::Point3D::new(
                        position.x + x_nm,
                        position.y + y_nm,
                        position.z + z_nm,
                    )
                })
                .unwrap_or(position);

            let geometry = InterfaceGeometry::Point(pin_pos);
            let id = space.entity_graph.allocate_interface_id();
            let intent = RoutingIntent::new("Default");

            // TODO(v0.1.10): Add orientation lookup from component definition
            //
            // Components should support explicit orientation declaration in their layout block:
            //
            //   component MyIC:
            //     layout:
            //       orientation: north  # Optional: north, south, east, west
            //       shape: Rectangle(...)
            //       pin_positions: { VCC: ..., GND: ... }
            //
            // Implementation steps:
            // 1. Add `orientation: Option<Orientation>` field to LayoutBlock in parser/ast/component.rs
            // 2. Parse orientation keyword in parser (north/south/east/west → Orientation enum)
            // 3. Here: Read component_def.layout.orientation and use if Some(), else:
            //    - Point geometry → Orientation::None (radial, no preference)
            //    - Edge/Polygon geometry → Orientation::Derived (from vertex winding)
            //
            // For now: Point geometry always uses None (correct for ball/via contacts)
            let orientation = hwc_engine::geometry_router::connection_interface::Orientation::None;

            let interface =
                hwc_engine::geometry_router::connection_interface::PhysicalInterface::new(
                    hwc_engine::geometry_router::connection_interface::PhysicalInterfaceParams {
                        id,
                        component_id,
                        geometry,
                        capabilities: smallvec![],
                        routing_intent: intent,
                        orientation: Some(orientation),
                        trace_width_nm,
                        escape_stub_length_nm: trace_width_nm * 2,
                    },
                    &db,
                );

            space.entity_graph.register_interface_with_pin(
                component_id,
                pin_name.clone(),
                interface,
            );
        }
    }

    if width_nm > 0 && height_nm > 0 {
        let bbox = if rotation_deg.abs() < 0.001 {
            BoundingBox::new(
                Point3D::new(
                    untransformed_origin.x,
                    untransformed_origin.y,
                    untransformed_origin.z,
                ),
                Point3D::new(
                    untransformed_origin.x + width_nm,
                    untransformed_origin.y + height_nm,
                    untransformed_origin.z + depth_nm,
                ),
            )
        } else {
            let center_x = untransformed_origin.x + width_nm / 2;
            let center_y = untransformed_origin.y + height_nm / 2;
            let half_w = width_nm / 2;
            let half_h = height_nm / 2;
            let corners = [
                (-half_w, -half_h),
                (half_w, -half_h),
                (half_w, half_h),
                (-half_w, half_h),
            ];
            let angle_rad = rotation_deg.to_radians();
            let cos_theta = angle_rad.cos();
            let sin_theta = angle_rad.sin();
            let mut min_x = i64::MAX;
            let mut max_x = i64::MIN;
            let mut min_y = i64::MAX;
            let mut max_y = i64::MIN;
            for (cx, cy) in corners.iter() {
                let rx = (*cx as f64 * cos_theta - *cy as f64 * sin_theta) as i64;
                let ry = (*cx as f64 * sin_theta + *cy as f64 * cos_theta) as i64;
                min_x = min_x.min(center_x + rx);
                max_x = max_x.max(center_x + rx);
                min_y = min_y.min(center_y + ry);
                max_y = max_y.max(center_y + ry);
            }
            BoundingBox::new(
                Point3D::new(min_x, min_y, untransformed_origin.z),
                Point3D::new(max_x, max_y, untransformed_origin.z + depth_nm),
            )
        };
        bbox_tracker.register(name.clone(), bbox, untransformed_origin);
    }

    unrolling::unroll_internal_features(
        space,
        &super::context::ComponentPlacementData {
            component,
            name: name.to_string(),
            position,
            rotation_deg,
            mount_side: mounting_res.mount_side,
        },
        bbox_tracker,
        ctx,
    )?;

    validation::validate_and_register(
        space,
        &super::context::ComponentPlacementData {
            component,
            name: name.to_string(),
            position,
            rotation_deg,
            mount_side: mounting_res.mount_side,
        },
        &super::context::ValidationParams {
            untransformed_origin,
            position,
            rotation_deg,
            body_min_z: mounting_res.body_min_z,
            body_max_z: mounting_res.body_max_z,
        },
        bbox_tracker,
        ctx,
    )?;

    Ok(())
}
