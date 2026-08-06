use super::super::conversions::{spanning_coordinate_to_point, CoordinateContext};
use super::super::errors::IrError;
use super::context::PlacementContext;
use hwc_engine::space::PourMetadata;
use hwc_engine::{HardwareSpace, Point3D};

// REMOVED: matches_center_edge() function
// 
// v0.2.1 Refactoring: `at:` positioning semantics are now explicit and predictable
// 
// EXPLICIT POSITIONING (at:):
//   - `at: [x, y]` places the shape's origin-aligned corner at (x, y)
//   - The corner used depends on the space's `origin:` declaration:
//     * `origin: bl by b` → places bottom-left corner at (x, y)
//     * `origin: tl by t` → places top-left corner at (x, y)
//     * etc.
//   - Coordinates are evaluated using anchor arithmetic, but the semantic is ALWAYS
//     "place the origin corner here" - no implicit centering adjustments
//
// RELATIONAL CENTERING (align:):
//   - `align: center_x with expr` explicitly calculates centering offset
//   - This is ergonomic sugar that compiles to explicit corner positioning
//   - The relational resolver handles the centering math
//
// This removes implicit magic where the compiler tried to detect center references
// in `at:` expressions and auto-adjust positioning. That violated the principle
// of least surprise and broke with complex expressions like `(A.center_x + B.center_x) / 2`.

struct ResolvedCutout {
    at_pt: Point3D,
    width_nm: Option<i64>,
    height_nm: Option<i64>,
    radius_nm: Option<i64>,
}

/// Resolve shape dimensions from a shape instance (v0.1.9 middle-level syntax)
///
/// This function extracts width and height from the shape instance parameters.
/// The shape instance contains the actual parameter values passed when instantiating the shape.
/// v0.1.10: Now supports expressions (including variables) in parameters
fn resolve_shape_dimensions(
    shape_inst: &hwc_parser::ShapeInstance,
    symbol_table: &crate::SymbolTable,
    eval_context: &hwc_parser::EvaluationContext,
) -> Result<(i64, i64), IrError> {
    // Look up the shape definition to verify it exists
    let shape_def = symbol_table
        .get_shape(&shape_inst.shape_name)
        .ok_or_else(|| IrError::UndeclaredShape {
            shape: shape_inst.shape_name.clone(),
        })?;

    eprintln!(
        "[SHAPE DEBUG] Resolving dimensions for shape: {}",
        shape_inst.shape_name
    );
    eprintln!(
        "[SHAPE DEBUG] Parameters count: {}",
        shape_inst.parameters.len()
    );

    let mut eval_params: Vec<(String, i64)> = Vec::new();
    let mut width_nm = None;
    let mut height_nm = None;

    for param in &shape_inst.parameters {
        let hwc_parser::Parameter::Keyword { name, value } = param;
        eprintln!("[SHAPE DEBUG] Processing parameter: {} = {:?}", name, value);

        let value_nm = match value {
            hwc_parser::ParameterValue::Measurement(m) => {
                let pm = m
                    .to_picometers_i64()
                    .ok_or_else(|| IrError::ShapeResolutionFailed {
                        shape: shape_inst.shape_name.clone(),
                        reason: format!("Parameter '{}' has non-distance unit", name),
                    })?;
                let nm = pm / 1000; // Convert picometers to nanometers
                eprintln!("[SHAPE DEBUG] Converted literal {}pm to {}nm", pm, nm);
                nm
            }
            hwc_parser::ParameterValue::Expression(expr) => {
                let nm = crate::ir::conversions::evaluate_expression_to_nm(
                    expr,
                    symbol_table,
                    eval_context,
                )
                .map_err(|e| IrError::ShapeResolutionFailed {
                    shape: shape_inst.shape_name.clone(),
                    reason: format!("Failed to evaluate parameter '{}': {}", name, e),
                })?;
                eprintln!("[SHAPE DEBUG] Evaluated expression to {}nm", nm);
                nm
            }
            _ => {
                return Err(IrError::ShapeResolutionFailed {
                    shape: shape_inst.shape_name.clone(),
                    reason: format!("Parameter '{}' must be a Measurement or Expression", name),
                });
            }
        };

        eval_params.push((name.to_string(), value_nm));

        match name.as_str() {
            "w" | "width" => {
                eprintln!("[SHAPE DEBUG] Setting width = {}nm", value_nm);
                width_nm = Some(value_nm);
            }
            "h" | "height" => {
                eprintln!("[SHAPE DEBUG] Setting height = {}nm", value_nm);
                height_nm = Some(value_nm);
            }
            _ => {}
        }
    }

    // Evaluate shape CSG geometry AST if dimensions are incomplete from explicit instance call parameters
    if width_nm.is_none() || height_nm.is_none() {
        if let Some(ref csg_expr) = shape_def.csg {
            let param_refs: Vec<(&str, i64)> = eval_params
                .iter()
                .map(|(k, v)| (k.as_str(), *v))
                .collect();
            let contour =
                crate::via_resolver::library::csg_eval::evaluate_csg_expression(csg_expr, &param_refs);
            if !contour.is_empty() {
                let min_x = contour.iter().map(|p| p.x).min().unwrap_or(0);
                let max_x = contour.iter().map(|p| p.x).max().unwrap_or(0);
                let min_y = contour.iter().map(|p| p.y).min().unwrap_or(0);
                let max_y = contour.iter().map(|p| p.y).max().unwrap_or(0);

                let evaluated_w = (max_x - min_x).abs();
                let evaluated_h = (max_y - min_y).abs();
                if evaluated_w > 0 && evaluated_h > 0 {
                    eprintln!(
                        "[SHAPE DEBUG] Evaluated CSG geometry dimensions: {}nm x {}nm",
                        evaluated_w, evaluated_h
                    );
                    if width_nm.is_none() {
                        width_nm = Some(evaluated_w);
                    }
                    if height_nm.is_none() {
                        height_nm = Some(evaluated_h);
                    }
                }
            }
        }
    }

    eprintln!(
        "[SHAPE DEBUG] Final: width={:?}, height={:?}",
        width_nm, height_nm
    );

    match (width_nm, height_nm) {
        (Some(w), Some(h)) => {
            eprintln!("[SHAPE DEBUG] Returning dimensions: {}nm x {}nm", w, h);
            Ok((w, h))
        }
        _ => Err(IrError::ShapeResolutionFailed {
            shape: shape_inst.shape_name.clone(),
            reason: format!(
                "Could not evaluate width and height for shape '{}' from its definition or instance parameters",
                shape_inst.shape_name
            ),
        }),
    }
}

pub fn place_plane(
    space: &mut HardwareSpace,
    plane: &hwc_parser::PlanePlacement,
    bbox_tracker: &mut crate::bounding_box_tracker::BoundingBoxTracker,
    ctx: &PlacementContext,
) -> Result<(), IrError> {
    let material_id = space
        .material_registry
        .get_id(&plane.material)
        .ok_or_else(|| IrError::UndeclaredMaterial {
            material: plane.material.clone(),
        })?;

    let layer_name = match &plane.elevation {
        hwc_parser::Elevation::Semantic(id) => id.to_string(),
        _ => "top_copper".to_string(),
    };

    let thickness_nm = if let Some(t_expr) = &plane.thickness {
        crate::ir::conversions::evaluate_expression_to_nm(t_expr, ctx.symbol_table, ctx.eval_context).map_err(
            |e| IrError::CoordinateResolutionFailed {
                coordinate_str: format!("plane '{}' thickness", plane.name),
                reason: e.to_string(),
            },
        )?
    } else {
        ctx.profile
            .and_then(|p| p.get_layer_thickness(&layer_name))
            .and_then(|t_expr| {
                crate::ir::conversions::evaluate_expression_to_nm(t_expr, ctx.symbol_table, ctx.eval_context).ok()
            })
            .unwrap_or_else(|| {
                ctx.stackup_manager
                    .get_layer_thickness(&layer_name)
                    .unwrap_or(0)
            })
    };

    if thickness_nm == 0 && plane.thickness.is_none() {
        return Err(IrError::PlacementConstraint {
            message: format!(
                "Could not resolve physical thickness for plane '{}' on layer '{}'. \
                 Ensure the layer is defined in the profile stackup or provide an explicit 'thickness:' property.",
                plane.name, layer_name
            ),
            component: plane.name.to_string().into(),
        });
    }

    let z_start_nm = ctx
        .stackup_manager
        .resolve_elevation(&plane.elevation, ctx.symbol_table, ctx.eval_context)?;
    let z_end_nm = z_start_nm + thickness_nm;

    // v0.2.1: Pass bbox_tracker for anchor arithmetic evaluation
    let coord_ctx = CoordinateContext {
        origin: ctx.origin,
        space_dimensions: &space.dimensions,
        symbol_table: ctx.symbol_table,
        eval_context: ctx.eval_context,
        bbox_tracker: Some(bbox_tracker),
        stackup_manager: ctx.stackup_manager,
        profile: ctx.profile,
    };

    let solver = crate::constraint_solver::ConstraintSolver::new(bbox_tracker, ctx.eval_context);

    // v0.1.9: Handle shape-based planes with parameterized geometry
    let (start, end, area_nm2) = if let Some(shape_inst) = &plane.shape {
        // Resolve shape parameters to get dimensions
        let (width_nm, height_nm) = resolve_shape_dimensions(shape_inst, ctx.symbol_table, ctx.eval_context)?;

        // Get position from `at:` field or relational constraints
        let mut position = if let Some(from_coord) = &plane.from {
            if from_coord.is_relative() {
                let intent = solver.resolve_position(from_coord).map_err(|e| {
                    IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("plane '{}' position", plane.name),
                        reason: e.to_string(),
                    }
                })?;
                intent.point()
            } else {
                spanning_coordinate_to_point(from_coord, &coord_ctx, false).map_err(|e| {
                    IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("plane '{}' position", plane.name),
                        reason: e,
                    }
                })?
            }
        } else {
            return Err(IrError::PlacementConstraint {
                message: format!(
                    "Plane '{}' with shape requires 'at:' coordinate for positioning",
                    plane.name
                ),
                component: plane.name.to_string().into(),
            });
        };

        // v0.2.1: Apply centering adjustments for align: center constraints
        // 
        // When using `align: center with <target>`, the relational resolver returns
        // the center X and Y coordinates. We need to subtract half the width/height 
        // to get the corner position.
        // 
        // This is the EXPLICIT centering behavior - no implicit magic based on expression content.
        for constraint in &plane.relational_constraints {
            if let hwc_parser::RelationalConstraint::Align { axis, .. } = constraint {
                match axis {
                    hwc_parser::AlignmentAxis::Center => {
                        position.x -= width_nm / 2;
                        position.y -= height_nm / 2;
                    }
                    hwc_parser::AlignmentAxis::X => {
                        position.x -= width_nm / 2;
                    }
                    hwc_parser::AlignmentAxis::Y => {
                        position.y -= height_nm / 2;
                    }
                    hwc_parser::AlignmentAxis::Z => {
                        // Z-centering adjustment would go here if needed
                    }
                    _ => {
                        // Edge alignments (top, bottom, left, right) don't need adjustment
                    }
                }
            }
        }
        // This violated least-surprise principle and failed with complex expressions.

        let end_pt = Point3D::new(position.x + width_nm, position.y + height_nm, position.z);
        let area = width_nm * height_nm;
        (position, end_pt, area)
    } else {
        // Legacy behavior: use explicit from/to coordinates
        match (&plane.from, &plane.to) {
            (Some(from_raw), Some(to_raw)) => {
                let from = if from_raw.is_relative() {
                    let intent = solver.resolve_position(from_raw).map_err(|e| {
                        IrError::CoordinateResolutionFailed {
                            coordinate_str: format!("plane '{}' from position", plane.name),
                            reason: e.to_string(),
                        }
                    })?;
                    intent.point()
                } else {
                    spanning_coordinate_to_point(from_raw, &coord_ctx, false).map_err(|e| {
                        IrError::CoordinateResolutionFailed {
                            coordinate_str: format!("plane '{}' from", plane.name),
                            reason: e,
                        }
                    })?
                };

                let to = if to_raw.is_relative() {
                    let intent = solver.resolve_position(to_raw).map_err(|e| {
                        IrError::CoordinateResolutionFailed {
                            coordinate_str: format!("plane '{}' to position", plane.name),
                            reason: e.to_string(),
                        }
                    })?;
                    intent.point()
                } else {
                    spanning_coordinate_to_point(to_raw, &coord_ctx, true).map_err(|e| {
                        IrError::CoordinateResolutionFailed {
                            coordinate_str: format!("plane '{}' to", plane.name),
                            reason: e,
                        }
                    })?
                };

                let w = (to.x - from.x).abs();
                let h = (to.y - from.y).abs();
                (from, to, w * h)
            }
            _ => {
                return Err(IrError::PlacementConstraint {
                    message: format!(
                        "Plane '{}' requires 'from' and 'to' coordinates",
                        plane.name
                    ),
                    component: plane.name.to_string().into(),
                });
            }
        }
    };

    let mut resolved_cutouts = Vec::new();
    for cutout in &plane.cutouts {
        let (at_raw, w_expr, h_expr, r_expr) = match cutout {
            hwc_parser::CutoutShape::Rectangle { width, height, at } => {
                (at, Some(width), Some(height), None)
            }
            hwc_parser::CutoutShape::Circle { radius, at } => (at, None, None, Some(radius)),
        };

        let at_resolved = if at_raw.is_relative() {
            let intent = solver
                .resolve_position(at_raw)
                .map_err(|e| IrError::CoordinateResolutionFailed {
                    coordinate_str: format!("cutout position for plane '{}'", plane.name),
                    reason: e.to_string(),
                })?;
            intent.point()
        } else {
            spanning_coordinate_to_point(at_raw, &coord_ctx, false).map_err(|e| {
                IrError::CoordinateResolutionFailed {
                    coordinate_str: format!("cutout position for plane '{}'", plane.name),
                    reason: e,
                }
            })?
        };

        let at_pt = at_resolved;

        let width_nm = w_expr
            .map(|e| {
                crate::ir::conversions::evaluate_expression_to_nm(e, ctx.symbol_table, ctx.eval_context).map_err(
                    |err| IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("cutout width for plane '{}'", plane.name),
                        reason: err.to_string(),
                    },
                )
            })
            .transpose()?;

        let height_nm = h_expr
            .map(|e| {
                crate::ir::conversions::evaluate_expression_to_nm(e, ctx.symbol_table, ctx.eval_context).map_err(
                    |err| IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("cutout height for plane '{}'", plane.name),
                        reason: err.to_string(),
                    },
                )
            })
            .transpose()?;

        let radius_nm = r_expr
            .map(|e| {
                crate::ir::conversions::evaluate_expression_to_nm(e, ctx.symbol_table, ctx.eval_context).map_err(
                    |err| IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("cutout radius for plane '{}'", plane.name),
                        reason: err.to_string(),
                    },
                )
            })
            .transpose()?;

        resolved_cutouts.push(ResolvedCutout {
            at_pt,
            width_nm,
            height_nm,
            radius_nm,
        });
    }

    drop(solver);

    let start_with_z = Point3D::new(start.x, start.y, z_start_nm);
    let end_with_z = Point3D::new(end.x, end.y, z_end_nm);

    let bbox = hwc_engine::geometry::BoundingBox::new(start_with_z, end_with_z);

    bbox_tracker.register(plane.name.to_string(), bbox, start_with_z);

    // v0.1.8: Register plane in EntityGraph for O(1) resolution
    let net_id = if let Some(net_name) = &plane.net {
        let _min_width_nm = space.fabrication_constraints.as_ref().map(|c| c.trace.min_width_nm)
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: "PDK missing required 'trace.min_width_nm' constraint".into(),
                hint: "Add a 'trace:' block to your profile with explicit min_width.\n\nExample:\n  trace:\n    min_width: 180nm".into(),
            })?;
        Some(space.netlist.get_or_create_net(&net_name.base))
    } else {
        None
    };

    space
        .entity_graph
        .register_space_entity(&plane.name.base, bbox, net_id, z_start_nm);

    // v0.1.9 CIR: Register PhysicalInterface for this plane/pad
    // This enables the router to query AccessRegions and avoid pad penetration
    //
    // v0.1.9.1: Use middle Z for interface geometry to align with routing queries
    {
        use hwc_engine::geometry_router::connection_interface::{InterfaceGeometry, PhysicalInterface};
        use hwc_engine::geometry_router::routing_intent::RoutingIntent;
        use hwc_engine::netlist::ComponentId;
        use smallvec::smallvec;

        // Require fabrication constraints - no fallbacks
        let constraints = space.fabrication_constraints.as_ref()
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: "Fabrication constraints required for interface generation".into(),
                hint: "Add a 'trace:' block to your profile with min_width and min_spacing".into(),
            })?;
        
        let trace_width_nm = constraints.trace.min_width_nm;
        let clearance_nm = constraints.trace.min_spacing_nm;

        // v0.1.9.1 CRITICAL: Calculate middle Z for Zero-Gap Z Lock alignment.
        //
        // PROBLEM: Previously used bbox.min.z (bottom Z = 960nm) for interface vertices.
        // But routing queries occur at the trace centerline (middle Z = 1160nm).
        // This Z mismatch caused:
        //   1. AccessRegion escape points at wrong Z (960nm instead of 1160nm)
        //   2. Boundary resolution creating routes with Z discontinuities
        //   3. Spatial index queries missing obstacles (query at 1160, obstacles at 960)
        //
        // SOLUTION: Register interface geometry at middle Z to match where routing occurs.
        // This ensures perfect Z alignment between placement and routing phases.
        let middle_z_nm = (bbox.min.z + bbox.max.z) / 2;

        // Register a Polygon interface with all four edges
        // The vertex winding order depends on the space's coordinate system origin.
        // For BL/BR (Y increases upward), use CCW winding.
        // For TL/TR (Y increases downward), use CW winding.
        
        use hwc_parser::OriginXY;
        let is_y_upward = matches!(ctx.origin.xy, OriginXY::BL | OriginXY::BR);
        
        let geometry = if is_y_upward {
            // CCW winding for Y-up coordinate systems (BL, BR) - using middle_z_nm
            InterfaceGeometry::Polygon(vec![
                Point3D::new(bbox.min.x, bbox.min.y, middle_z_nm),  // bottom-left
                Point3D::new(bbox.max.x, bbox.min.y, middle_z_nm),  // bottom-right
                Point3D::new(bbox.max.x, bbox.max.y, middle_z_nm),  // top-right
                Point3D::new(bbox.min.x, bbox.max.y, middle_z_nm),  // top-left
            ])
        } else {
            // CW winding for Y-down coordinate systems (TL, TR) - using middle_z_nm
            InterfaceGeometry::Polygon(vec![
                Point3D::new(bbox.min.x, bbox.min.y, middle_z_nm),  // top-left
                Point3D::new(bbox.min.x, bbox.max.y, middle_z_nm),  // bottom-left
                Point3D::new(bbox.max.x, bbox.max.y, middle_z_nm),  // bottom-right
                Point3D::new(bbox.max.x, bbox.min.y, middle_z_nm),  // top-right
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
                    pi.cost_weights.as_ref().map(|cw| hwc_materials::IntentCostWeights {
                        base_cost: cw.base,
                        via_penalty: cw.via_penalty,
                        direction_penalty: cw.direction_penalty,
                        tight_clearance_penalty: cw.tight_clearance_penalty,
                        crosstalk_penalty: cw.crosstalk_penalty,
                        impedance_penalty: cw.impedance_penalty,
                        reference_void_penalty: cw.reference_void_penalty,
                    }).as_ref(),
                    pi.escape_stub.as_ref().and_then(|meas| {
                        meas.to_picometers_i64().map(|pm| pm / 1000)
                    }),
                )
            })
            .collect();
        
        // Require explicit "Signal" intent declaration - no fallbacks
        let intent = RoutingIntent::lookup("Signal", &profile_intents)
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: "Profile missing required 'Signal' net_type declaration".into(),
                hint: "Add routing intent to your profile:\n\n\
                       net_type Signal:\n    routing_style: auto\n    escape_stub: 0nm".into(),
            })?;
        
        let db = hwc_engine::geometry_router::connection_interface::DefaultRoutingDatabase::default();
        let pseudo_component_id = ComponentId::new(0xFFFF_0000 + interface_id.raw());
        
        // Planes always use Derived orientation because the polygon winding (determined by
        // space origin) encodes the correct outward direction.
        let interface = PhysicalInterface::new(
            interface_id,
            pseudo_component_id,
            geometry,
            smallvec![],
            intent,
            hwc_engine::geometry_router::connection_interface::Orientation::Derived,
            &db,
            trace_width_nm,
            clearance_nm * 2,
        );
        
        space.entity_graph.register_space_entity_interface(
            plane.name.base.clone(),
            interface,
        );
    }

    // Note: Substrate layer registration happens after netlist processing (see below)
    // to ensure we have the correct resolved net_id

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

    if let Some(substrate_bbox) = &space.substrate_bbox {
        if bbox.intersects(substrate_bbox) && space.substrate_material_id != material_id {
            let is_conductor = space.material_registry.is_conductor(material_id);
            let is_substrate_insulator = space
                .material_registry
                .is_insulator(space.substrate_material_id)
                || space
                    .material_registry
                    .is_semiconductor(space.substrate_material_id);

            if is_conductor && is_substrate_insulator {
                let plane_net_id = if let Some(net_name) = &plane.net {
                    space
                        .netlist
                        .get_net_by_name(net_name.base.as_str())
                        .unwrap_or(hwc_engine::netlist::NetId::new(0))
                } else {
                    hwc_engine::netlist::NetId::new(0)
                };
                space
                    .entity_graph
                    .drill_hole(bbox, None, plane_net_id);
                println!(
                    "   ├─ Auto-carved substrate for plane '{}' ({})",
                    plane.name, plane.material
                );
            } else {
                let substrate_material_name = space
                    .material_registry
                    .get_name(space.substrate_material_id)
                    .unwrap_or("Unknown");

                return Err(IrError::PlacementConstraint {
                    message: format!(
                        "Substrate interpenetration detected: Plane '{}' ({}) overlaps with the base substrate ({}). \
                         Use the same material as the substrate, or place the plane outside the substrate bounds.",
                        plane.name,
                        plane.material,
                        substrate_material_name
                    ),
                    component: plane.name.to_string().into(),
                });
            }
        }
    }

    for existing in &space.pours {
        if let Some(existing_bbox) = &existing.bbox {
            if bbox.intersects(existing_bbox) {
                let z_overlap =
                    bbox.max.z > existing_bbox.min.z && existing_bbox.max.z > bbox.min.z;
                if z_overlap && existing.material_name != plane.material {
                    return Err(IrError::MaterialInterpenetration {
                        pour_a: existing.name.clone(),
                        mat_a: existing.material_name.clone(),
                        pour_b: plane.name.to_string(),
                        mat_b: plane.material.clone(),
                        z_nm: z_start_nm,
                    });
                }
            }
        }
    }

    let resolved_net_name = plane.net.as_ref().map(|n| n.base.clone());

    space.pours.push(PourMetadata {
        name: plane.name.to_string(),
        material_name: plane.material.clone(),
        z_bottom_nm: z_start_nm,
        net: resolved_net_name.clone(),
        area_nm2,
        bbox: Some(bbox),
        device_binding: None,
        merged_region_id: None,
        waivers: Default::default(),
    });

    let net_id = if let Some(net_name) = resolved_net_name.as_ref() {
        let center_x = (start_with_z.x + end_with_z.x) / 2;
        let center_y = (start_with_z.y + end_with_z.y) / 2;
        let center_z = (start_with_z.z + end_with_z.z) / 2;

        let plane_component_id = space.netlist.add_component(
            plane.name.to_string(),
            format!("Plane({})", plane.material).into(),
            (center_x, center_y, center_z),
        );

        // v0.1.9: Use __virtual_ naming convention for routing compatibility
        let virtual_pin_name = format!("__virtual_{}", plane.name);
        let anchor_pin_id = space.netlist.add_pin(
            plane_component_id,
            virtual_pin_name.clone().into(),
            (0, 0, 0),
            None,
        );

        let net_id_handle =
            if let Some(existing_net) = space.netlist.get_net_by_name(net_name.as_str()) {
                existing_net
            } else {
                space
                    .netlist
                    .add_net(net_name.clone(), 100_000, material_id)
            };

        space.netlist.connect_pin(anchor_pin_id, net_id_handle);

        space.entity_graph.add_component_pin(
            center_x,
            center_y,
            center_z,
            plane.name.to_string(),
            virtual_pin_name.into(),
            Some(net_name.clone()),
        );

        // println!(
        //     "   ├─ Registered anchor point for plane '{}' at ({:.3}mm, {:.3}mm, {:.3}mm) on net '{}'",
        //     plane.name,
        //     center_x as f64 / 1_000_000.0,
        //     center_y as f64 / 1_000_000.0,
        //     center_z as f64 / 1_000_000.0,
        //     net_name
        // );

        net_id_handle.raw()
    } else {
        0
    };

    // v0.2.0: Register plane surface in layer connection database
    // Planes exist on a single Z plane, so they register as PlaneSurface type
    {
        let plane_center_x = (start_with_z.x + end_with_z.x) / 2;
        let plane_center_y = (start_with_z.y + end_with_z.y) / 2;
        
        // v0.2.1 FIX: Use the routing layer's official routing_z, not start_with_z.z
        // Base layers (polyres, active) route at z_top, interconnect (metal1+) at z_bottom
        let routing_z = match space.routing_layer_db.get_routing_z(&layer_name) {
            Ok(z) => z,
            Err(_) => {
                // Layer not found or not routable - use z_bottom as fallback
                eprintln!(
                    "[PLACE_PLANE] WARNING: Layer '{}' not found in routing database, using z_bottom={}nm",
                    layer_name, start_with_z.z
                );
                start_with_z.z
            }
        };

        if let Err(e) = space.layer_connection_db.register_surface(
            &plane.name.base,
            &layer_name,
            routing_z,
            (plane_center_x, plane_center_y),
            material_id,
            hwc_engine::layer_connection_database::ConnectionType::PourSurface,
        ) {
            eprintln!(
                "[PLACE_PLANE] WARNING: Failed to register plane '{}' connection: {}",
                plane.name.base, e
            );
        } else {
            eprintln!(
                "[PLACE_PLANE] Registered plane '{}' surface on layer '{}' at routing Z={}nm (routing layer elevation)",
                plane.name.base, layer_name, routing_z
            );
        }
    }

    // v0.1.9: Register as substrate layer so routing can see it as an obstacle
    // Planes with net_id are conductive pours, planes without net_id are keepout zones
    let bbox = hwc_engine::geometry::BoundingBox::new(start_with_z, end_with_z);
    eprintln!(
        "[SUBSTRATE DEBUG] Adding plane '{}' as substrate layer: material_id={}, net={}, bbox=({},{},{}) to ({},{},{})",
        plane.name,
        material_id,
        net_id,
        bbox.min.x, bbox.min.y, bbox.min.z,
        bbox.max.x, bbox.max.y, bbox.max.z
    );
    space.entity_graph.add_substrate_layer(
        material_id,
        hwc_engine::NetId::new(net_id),
        bbox,
        hwc_engine::geometry_router::entity_graph::SubstrateLayerType::Pour,
    );

    for rc in resolved_cutouts {
        if let (Some(w), Some(h)) = (rc.width_nm, rc.height_nm) {
            let cutout_start = Point3D::new(rc.at_pt.x, rc.at_pt.y, z_start_nm);
            let cutout_end = Point3D::new(rc.at_pt.x + w, rc.at_pt.y + h, z_end_nm);
            let cutout_bbox = hwc_engine::geometry::BoundingBox::new(cutout_start, cutout_end);
            space.entity_graph.drill_hole(cutout_bbox, None, hwc_engine::NetId::UNCONNECTED);
        } else if let Some(r) = rc.radius_nm {
            let rf = r as f64;
            let cutout_start =
                Point3D::new(rc.at_pt.x - rf as i64, rc.at_pt.y - rf as i64, z_start_nm);
            let cutout_end = Point3D::new(rc.at_pt.x + rf as i64, rc.at_pt.y + rf as i64, z_end_nm);
            let cutout_bbox = hwc_engine::geometry::BoundingBox::new(cutout_start, cutout_end);
            space
                .entity_graph
                .add_circle_substrate_layer(0, hwc_engine::NetId::UNCONNECTED, cutout_bbox, r);
        }
    }

    Ok(())
}
