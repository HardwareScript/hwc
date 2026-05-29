//! Component placement functionality.

use super::super::conversions::{coordinate_to_point, CoordinateContext};
use super::super::errors::IrError;
use super::array::place_component_array;
use super::coordinate_evaluation::{evaluate_coordinate_to_nm, CoordinateAxis};
use super::helpers::parse_rectangle_dimensions;
use super::module::place_module_instance;
use super::pour::place_pour;
use super::super::stackup_manager::StackupManager;
use crate::SymbolTable;
use hwc_engine::{ComponentPlacer, HardwareSpace, PlacementParams};
use hwc_parser::Coordinate;

/// Place a component in the voxel grid.
///
/// This function checks if the component_type is actually a module.
/// If it is, it flattens the module and places all internal components.
/// Otherwise, it places the component directly.
///
/// Sprint 3, Task 3.1: Added bbox_tracker parameter for relative positioning support
/// Sprint 3, Task 3.2: Added array unrolling support for transistor fingers
/// v0.1.6: Added eval_context parameter for Universal Context (eliminates Initialization Storm)
pub fn place_component(
    space: &mut HardwareSpace,
    component: &hwc_parser::ComponentPlacement,
    origin: hwc_parser::OriginPoint,
    symbol_table: &SymbolTable,
    layouts: &[hwc_parser::ModuleLayoutBlock],
    bbox_tracker: &mut crate::bounding_box_tracker::BoundingBoxTracker,
    eval_context: &hwc_parser::EvaluationContext,
    collector: &hwc_diagnostics::DiagnosticCollector,
    stackup_manager: &super::super::stackup_manager::StackupManager,
) -> Result<(), IrError> {
    // Sprint 3, Task 3.2: Check if this is an array placement
    if let Some(array_config) = &component.array_config {
        let mut array_ctx = super::array::ArrayPlacementContext {
            origin,
            symbol_table,
            layouts,
            bbox_tracker,
            eval_context,
            collector,
            stackup_manager: &super::super::stackup_manager::StackupManager::new(None, symbol_table, space.voxel_size.z_nm, origin.z).expect("temp manager"),
        };
        return place_component_array(space, component, array_config, &mut array_ctx);
    }

    // Check if this is a module instantiation
    if symbol_table.has_module(component.component_type.as_str()) {
        // This is a module - we need to flatten it
        return place_module_instance(
            space,
            component,
            origin,
            symbol_table,
            layouts,
            bbox_tracker,
            eval_context,
            collector,
            stackup_manager,
        );
    }

    // Sprint 3, Task 3.1: Resolve relative coordinates to absolute
    // UNIVERSAL CONTEXT: Pass the pre-built eval_context to avoid rebuilding it
    let resolved_position = if component.position.is_relative() {
        let solver = crate::constraint_solver::ConstraintSolver::new(bbox_tracker, eval_context);
        solver.resolve_position(&component.position).map_err(|e| {
            IrError::PlacementError(format!("Failed to resolve relative position: {}", e))
        })?
    } else {
        component.position.clone()
    };

    // Regular component placement
    let ctx = CoordinateContext {
        voxel_size: &space.voxel_size,
        grid_size: &space.grid,
        origin,
        space_dimensions: &space.dimensions,
        symbol_table,
        eval_context,                     // Pass the universal context
        bbox_tracker: Some(bbox_tracker), // Pass bbox_tracker for anchor references in expressions
        stackup_manager,
    };
    let mut position = coordinate_to_point(&resolved_position, &ctx);

    // v0.1.7: Resolve elevation from 'on layer:' or 'on z:' prepositional syntax
    if let Some(elevation) = &component.elevation {
        let z_user_nm = stackup_manager.resolve_elevation(elevation, symbol_table)?;
        let final_z = crate::ir::conversions::apply_z_origin_physical(z_user_nm, origin.z, space.dimensions.depth_nm);
        eprintln!("[DEBUG elevation] Resolved '{}' to {} nm (physical: {} nm)", 
            match elevation { hwc_parser::Elevation::Semantic(id) => id.name.as_str(), _ => "physical" },
            z_user_nm, final_z);
        position.z = final_z;
    }

    // v0.1.7: Implement snap_to_surface (Limitation 5)
    if component.waivers.snap_to_surface {
        // Find the highest substrate/pour at this location
        // We use the specified (x, y) as the probe point
        let (vx, vy, _) = hwc_engine::VoxelGrid::nm_to_voxel(position, &space.voxel_size);

        // Safety: Clamp vx, vy to grid bounds
        let vx = vx.min(space.grid.x_cols - 1);
        let vy = vy.min(space.grid.y_rows - 1);

        let mut highest_z_nm = 0;

        // Check substrate bounding box (the sparse substrate)
        if let Some(bbox) = &space.substrate_bbox {
            if position.x >= bbox.min.x
                && position.x <= bbox.max.x
                && position.y >= bbox.min.y
                && position.y <= bbox.max.y
            {
                highest_z_nm = highest_z_nm.max(bbox.max.z);
            }
        }

        // Check voxel grid for pours and other filled voxels
        for vz in (0..space.grid.z_layers).rev() {
            if !space.voxel_grid.is_empty(vx, vy, vz) {
                // The surface is the TOP of the highest occupied voxel
                let surface_z_nm = (vz as i64 + 1) * space.voxel_size.z_nm;
                highest_z_nm = highest_z_nm.max(surface_z_nm);
                break;
            }
        }

        position.z = highest_z_nm;
    }

    // v0.1.7 CRITICAL: Store the UNTRANSFORMED position as the origin
    // This is the coordinate the user specified, before origin transformation
    // Extract raw coordinate values WITHOUT origin transformation
    let mut untransformed_origin = {
        let (x_expr, y_expr, z_expr) = match &resolved_position {
            Coordinate::Positional { x, y, z, .. } | Coordinate::Declarative { x, y, z, .. } => {
                (x, y, z)
            }
            Coordinate::Relative(_) => {
                panic!("Relative coordinates should be resolved before this point");
            }
        };

        // Check if expressions contain anchor references
        let has_anchor_refs = x_expr.contains_anchor_reference()
            || y_expr.contains_anchor_reference()
            || z_expr.contains_anchor_reference();

        // Evaluate to nanometers without origin transformation
        let x_nm = if let Ok(hwc_parser::Value::Percentage(pct)) = x_expr.evaluate(eval_context) {
            ((pct / 100.0) * space.dimensions.width_nm as f64) as i64
        } else if has_anchor_refs && x_expr.contains_anchor_reference() {
                crate::ir::placement::coordinate_evaluation::evaluate_coordinate_with_anchors(
                    x_expr,
                    symbol_table,
                    bbox_tracker,
                    CoordinateAxis::X,
                    origin.z,
                )
            .expect("Failed to evaluate X coordinate with anchor references")
        } else {
            crate::ir::conversions::evaluate_expression_to_nm(x_expr, symbol_table)
                .expect("Failed to evaluate X coordinate")
        };

        let y_nm = if let Ok(hwc_parser::Value::Percentage(pct)) = y_expr.evaluate(eval_context) {
            ((pct / 100.0) * space.dimensions.height_nm as f64) as i64
        } else if has_anchor_refs && y_expr.contains_anchor_reference() {
                crate::ir::placement::coordinate_evaluation::evaluate_coordinate_with_anchors(
                    y_expr,
                    symbol_table,
                    bbox_tracker,
                    CoordinateAxis::Y,
                    origin.z,
                )
            .expect("Failed to evaluate Y coordinate with anchor references")
        } else {
            crate::ir::conversions::evaluate_expression_to_nm(y_expr, symbol_table)
                .expect("Failed to evaluate Y coordinate")
        };

        let z_ctx = crate::ir::conversions::CoordinateContext {
            voxel_size: &space.voxel_size,
            grid_size: &space.grid,
            origin,
            space_dimensions: &space.dimensions,
            symbol_table,
            eval_context,
            bbox_tracker: Some(bbox_tracker),
            stackup_manager,
        };
        let z_nm = crate::ir::conversions::resolve_coordinate_z_nm(z_expr, &z_ctx, has_anchor_refs)
            .map_err(IrError::PlacementError)?;

        // Sprint 5.5: Validate Z coordinate bounds before creating point
        if z_nm < 0 {
            let z_span = z_expr.span();
            return Err(IrError::NegativeLayerIndex {
                value: z_nm,
                span: (z_span.start, z_span.end - z_span.start).into(),
            });
        }

        hwc_engine::geometry::Point3D::new(x_nm, y_nm, z_nm)
    };

    // v0.1.7 FIX: If elevation is provided, update untransformed_origin.z so that pins
    // and internal pours are correctly positioned relative to the elevation.
    if component.elevation.is_some() || component.waivers.snap_to_surface {
        untransformed_origin.z = position.z;
    }

    // v0.1.7: Coordinate Origin Alignment
    // Ensure untransformed_origin reflects the final world position before origin-flipping
    // This ensures internal unrolling uses consistent world coordinates.
    let _component_x_nm = position.x;
    let _component_y_nm = position.y;
    let _component_z_nm = position.z;

    let rotation_deg = component.rotation.as_ref().map(|r| r.angle).unwrap_or(0.0);

    let z_val = untransformed_origin.z / space.voxel_size.z_nm.max(1);
    let name = component
        .name
        .as_ref()
        .map(|n| n.to_string())
        .unwrap_or_else(|| format!("{}_{}", component.component_type, z_val).into());

    let placer = ComponentPlacer::new();
    placer
        .place_component(PlacementParams {
            grid: &mut space.voxel_grid,
            voxel_size: &space.voxel_size,
            arena: &mut space.netlist,
            symbol_table,
            material_registry: &mut space.material_registry,
            name: name.clone(),
            component_type: component.component_type.to_string().into(),
            position,
            rotation_deg,
            // v0.1.7: Pass unified merge waiver to engine (Boolean or List)
            merge_waiver: component.waivers.merge.clone(),
            // Pass diagnostic collector for waiver reporting via adapter
            collector: Some(&crate::DiagnosticReporterAdapter(collector)),
        })
        .map_err(|e| IrError::PlacementError(e.to_string()))?;

    // Sprint 2.2: Unroll internal pours from component definition
    // OPTIMIZED: Direct nanometer calculation (5-10× faster than AST construction)
    // Transform relative coordinates to absolute and add to space's substrate layers
    if let Ok(component_def) = symbol_table.get_component(component.component_type.as_str()) {
        if let Some(layout) = &component_def.layout {
            if !layout.internal_pours.is_empty() {
                // println!(
                //     "[DEBUG] Unrolling {} internal pours for component '{}' (optimized path)",
                //     layout.internal_pours.len(),
                //     name
                // );

                // Use the aligned world coordinates we just calculated
                // This fixes the "floating connection" by syncing internal unrolling with world placement
                let abs_x_nm = position.x;
                let abs_y_nm = position.y;
                let abs_z_nm = position.z;
                
                eprintln!("[DEBUG unroll] Component '{}' at pos.z: {} nm", name, position.z);

                for pour in &layout.internal_pours {
                    // Transform relative coordinates to absolute
                    // Component position is the origin, pour boundary is relative to that
                    if let Some((from, to)) = &pour.boundary {
                        // Direct nanometer calculation (no AST construction)
                        let pour_from_x_nm = evaluate_coordinate_to_nm(from.x(), symbol_table)?;
                        let pour_from_y_nm = evaluate_coordinate_to_nm(from.y(), symbol_table)?;
                        let pour_to_x_nm = evaluate_coordinate_to_nm(to.x(), symbol_table)?;
                        let pour_to_y_nm = evaluate_coordinate_to_nm(to.y(), symbol_table)?;

                        // Add component position to pour offsets (simple addition)
                        let absolute_from_x_nm = abs_x_nm + pour_from_x_nm;
                        let absolute_from_y_nm = abs_y_nm + pour_from_y_nm;
                        let absolute_to_x_nm = abs_x_nm + pour_to_x_nm;
                        let absolute_to_y_nm = abs_y_nm + pour_to_y_nm;

                        // Create literal expressions (no evaluation needed later)
                        let absolute_from = hwc_parser::Coordinate::Declarative {
                            x: hwc_parser::Expression::Measurement {
                                value: absolute_from_x_nm as f64 / 1_000_000.0,
                                unit: hwc_parser::Unit::Millimeter,
                                span: hwc_parser::Span { start: 0, end: 0 },
                            },
                            y: hwc_parser::Expression::Measurement {
                                value: absolute_from_y_nm as f64 / 1_000_000.0,
                                unit: hwc_parser::Unit::Millimeter,
                                span: hwc_parser::Span { start: 0, end: 0 },
                            },
                            z: hwc_parser::Expression::Measurement {
                                value: (abs_z_nm + evaluate_coordinate_to_nm(from.z(), symbol_table).unwrap_or(0)) as f64 / 1_000_000.0,
                                unit: hwc_parser::Unit::Millimeter,
                                span: hwc_parser::Span { start: 0, end: 0 },
                            },
                            span: hwc_parser::Span { start: 0, end: 0 },
                        };

                        let absolute_to = hwc_parser::Coordinate::Declarative {
                            x: hwc_parser::Expression::Measurement {
                                value: absolute_to_x_nm as f64 / 1_000_000.0,
                                unit: hwc_parser::Unit::Millimeter,
                                span: hwc_parser::Span { start: 0, end: 0 },
                            },
                            y: hwc_parser::Expression::Measurement {
                                value: absolute_to_y_nm as f64 / 1_000_000.0,
                                unit: hwc_parser::Unit::Millimeter,
                                span: hwc_parser::Span { start: 0, end: 0 },
                            },
                            z: hwc_parser::Expression::Measurement {
                                value: (abs_z_nm + evaluate_coordinate_to_nm(to.z(), symbol_table).unwrap_or(0)) as f64 / 1_000_000.0,
                                unit: hwc_parser::Unit::Millimeter,
                                span: hwc_parser::Span { start: 0, end: 0 },
                            },
                            span: hwc_parser::Span { start: 0, end: 0 },
                        };

                        // ALIGNMENT CORRECTNESS FIX (v0.1.6 Item #1): Assign net from pin-to-net bindings
                        //
                        // The Problem:
                        // - Component internal pours were registered with net: None
                        // - Physical Continuity checker only processes pours with net assignments
                        // - Result: Component pours were invisible to continuity validation
                        // - Ring oscillator showed 3 disconnected inverters but passed validation
                        //
                        // The Fix:
                        // - Component placements MUST provide explicit pin-to-net bindings
                        // - Component definitions define STRUCTURE (geometry, pins)
                        // - Space placements define CONNECTIVITY (which pins connect to which nets)
                        // - This is the correct separation of concerns
                        let _net_assignment = if let Some(device_binding) = &pour.device {
                            // Get the net binding for this terminal/pin
                            component.pin_net_bindings.get(device_binding.terminal.as_str())
                                .and_then(|binding| match binding {
                                    hwc_parser::NetBinding::Simple(net_name) => {
                                        Some(hwc_parser::NetName {
                                            base: net_name.clone(),
                                            index: None,
                                            span: hwc_parser::Span { start: 0, end: 0 },
                                        })
                                    }
                                    hwc_parser::NetBinding::Conditional { .. } => {
                                        // Conditional bindings should have been resolved during unrolling
                                        eprintln!("⚠️  WARNING: Conditional net binding found during pour unrolling (should have been resolved)");
                                        None
                                    }
                                })
                        } else {
                            None
                        };

                        // Create a new pour with absolute coordinates
                        // v0.1.7: The internal pour's elevation must be resolved relative to the component base Z
                        let pour_z_start_nm = stackup_manager.resolve_elevation(&pour.elevation, symbol_table).unwrap_or(0);
                        let pour_z_end_nm = stackup_manager.resolve_elevation_top(&pour.elevation, symbol_table, space.voxel_size.z_nm).unwrap_or(pour_z_start_nm + space.voxel_size.z_nm);

                        let absolute_z_start_nm = abs_z_nm + pour_z_start_nm;
                        let absolute_z_end_nm = abs_z_nm + pour_z_end_nm;
                        eprintln!("[DEBUG unroll-pour] '{}' absolute z: {} nm to {} nm", pour.name, absolute_z_start_nm, absolute_z_end_nm);

                        let absolute_pour = hwc_parser::PourPlacement {
                            material: pour.material.clone(),
                            name: hwc_parser::ComponentName::simple(format!("{}_{}", name, pour.name).into(), pour.span),
                            elevation: hwc_parser::Elevation::Physical {
                                start: hwc_parser::Expression::Measurement {
                                    value: absolute_z_start_nm as f64 / 1_000_000.0,
                                    unit: hwc_parser::Unit::Millimeter,
                                    span: hwc_parser::Span { start: 0, end: 0 },
                                },
                                end: Some(hwc_parser::Expression::Measurement {
                                    value: absolute_z_end_nm as f64 / 1_000_000.0,
                                    unit: hwc_parser::Unit::Millimeter,
                                    span: hwc_parser::Span { start: 0, end: 0 },
                                }),
                            },
                            boundary: Some((absolute_from, absolute_to)),
                            net: None, // FIX: Don't assign net directly to internal pours.
                                       // The 'device' binding below already handles logical connectivity.
                                       // Assigning 'net' here triggers redundant "Virtual Anchor" generation
                                       // in place_pour, which causes the out-of-bounds ghost traces.
                            device: pour.device.as_ref().map(|d| hwc_parser::DeviceBinding {
                                device_name: name.clone(),
                                terminal: d.terminal.clone(),
                                span: d.span,
                            }),
                            thermal_relief: pour.thermal_relief,
                            waivers: pour.waivers.clone(),
                            span: pour.span,
                        };

                        // Place the unrolled pour
                        // v0.1.7: Create a fallback StackupManager for this internal unrolling path
                        let temp_manager = StackupManager::new(None, symbol_table, space.voxel_size.z_nm, origin.z)
                            .expect("Failed to create temp StackupManager");
                        place_pour(space, &absolute_pour, origin, symbol_table, bbox_tracker, eval_context, collector, &temp_manager)?;

                        // println!(
                        //     "[DEBUG]   Unrolled pour '{}' -> '{}' at ({:.3}mm, {:.3}mm) net: {:?}",
                        //     pour.name,
                        //     absolute_pour.name,
                        //     absolute_from_x_nm as f64 / 1_000_000.0,
                        //     absolute_from_y_nm as f64 / 1_000_000.0,
                        //     absolute_pour.net.as_ref().map(|n| n.to_string())
                        // );
                    }
                }
            }
        }
    }

    // Sprint 3, Task 3.1: Register bounding box for relative positioning
    // Get component dimensions from symbol table
    if let Ok(component_def) = symbol_table.get_component(component.component_type.as_str()) {
        if let Some(layout) = &component_def.layout {
            if let Some(shape_str) = &layout.shape {
                // Parse shape to get dimensions (simple Rectangle parsing)
                // Format: "Rectangle(4mm, 4mm, 0.5mm)"
                if let Some(dims) = parse_rectangle_dimensions(shape_str) {
                    let (width_nm, height_nm, depth_nm) = dims;

                    // GAP1 + ROTATION FIX (v0.1.7 Anchor Realization): Use untransformed_origin + compute post-rotation AABB
                    //
                    // - `untransformed_origin` = USER-SPECIFIED top-left (for correct 'last'/'anchor' inheritance)
                    // - If rotation, rotate the 4 corners around component CENTER exactly as engine::calculate_global_bounding_box does.
                    // - This bakes the final transformed geometry into BBoxTracker BEFORE any dependent pours/traces query anchors.
                    // - Combined with two-pass in ir/mod.rs: eliminates wedge/stretched artifacts from stale pre-rotation bboxes.
                    let bbox = if rotation_deg.abs() < 0.001 {
                        hwc_engine::geometry::BoundingBox::new(
                            hwc_engine::geometry::Point3D::new(
                                untransformed_origin.x,
                                untransformed_origin.y,
                                untransformed_origin.z,
                            ),
                            hwc_engine::geometry::Point3D::new(
                                untransformed_origin.x + width_nm,
                                untransformed_origin.y + height_nm,
                                untransformed_origin.z + depth_nm,
                            ),
                        )
                    } else {
                        // Replicate engine rotation math (center-based 2D XY rotation, Z unchanged)
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
                            let gx = center_x + rx;
                            let gy = center_y + ry;
                            min_x = min_x.min(gx);
                            max_x = max_x.max(gx);
                            min_y = min_y.min(gy);
                            max_y = max_y.max(gy);
                        }
                        hwc_engine::geometry::BoundingBox::new(
                            hwc_engine::geometry::Point3D::new(min_x, min_y, untransformed_origin.z),
                            hwc_engine::geometry::Point3D::new(max_x, max_y, untransformed_origin.z + depth_nm),
                        )
                    };

                    // GAP2: Check for substrate overlap BEFORE registering bbox
                    // v0.1.7: merge: true waives substrate overlap (P42)
                    let skip_substrate_check = component.waivers.merge == hwc_parser::MergeWaiver::All;

                    if let Some(substrate_bbox) = &space.substrate_bbox {
                            // Check if component overlaps with substrate in Z-axis
                            // Component overlaps if its Z range intersects substrate's Z range
                            let component_min_z = untransformed_origin.z;
                            let component_max_z = untransformed_origin.z + depth_nm;
                            let substrate_min_z = substrate_bbox.min.z;
                            let substrate_max_z = substrate_bbox.max.z;

                            // Calculate layer indices for error messages
                            let component_z_layer =
                                (component_min_z / space.voxel_size.z_nm) as usize;
                            let substrate_min_layer =
                                (substrate_min_z / space.voxel_size.z_nm) as usize;
                            let substrate_max_layer =
                                (substrate_max_z / space.voxel_size.z_nm) as usize;

                            // Extract actual source line for beautiful suggestions
                            let source = collector.source.as_str();
                            let original_line = source.get(component.span.start as usize..component.span.end as usize)
                                .unwrap_or("add ...");

                            // Construct group context for pattern detection (strip indices like [0] or trailing digits)
                            let group_context = if let Some(n) = &component.name {
                                let name_str = n.base.as_str();
                                if let Some(idx) = name_str.find('[') {
                                    &name_str[..idx]
                                } else {
                                    // Strip trailing digits for grouping (e.g. Adder0 -> Adder)
                                    name_str.trim_end_matches(|c: char| c.is_ascii_digit())
                                }
                            } else {
                                component.component_type.as_str()
                            };

                            // Sprint 5.5: Check for floating component (above substrate)
                            if component_min_z > substrate_max_z {
                                let gap_nm = component_min_z - substrate_max_z;
                                let gap_mm = gap_nm as f64 / 1_000_000.0;

                                // v0.1.7: merge: true DOES NOT waive floating errors (P44)
                                // Only explicit floating: true can waive this.
                                if !component.waivers.floating {
                                    // Sprint 9 (Task 9.1): Report and CONTINUE — don't abort.
                                    // This allows the compiler to collect ALL floating violations
                                    // before closing the Commit Gate (max 50, like rustc).
                                    
                                    // Construct dynamic suggestion
                                    let suggestion = format!(
                                        "To fix:\n- Place component at z:{substrate_max_layer} (substrate surface)\n- Corrected: {}",
                                        original_line.replace(&format!("z: {}", component_z_layer), &format!("z: {}", substrate_max_layer))
                                    );

                                    // Sprint 9: Feed both systems
                                    // 1. DiagnosticCollector (for the first 50 snippets)
                                    let ir_x_mm = untransformed_origin.x as f64 / 1_000_000.0;
                                    let ir_y_mm = untransformed_origin.y as f64 / 1_000_000.0;
                                    let ir_z_mm = untransformed_origin.z as f64 / 1_000_000.0;
                                    collector.report(IrError::ComponentFloatingInAir {
                                        component: name.clone(),
                                        component_z_layer,
                                        component_z_mm: component_min_z as f64 / 1_000_000.0,
                                        substrate_max_layer,
                                        substrate_max_mm: substrate_max_z as f64 / 1_000_000.0,
                                        gap_mm,
                                        x_mm: ir_x_mm,
                                        y_mm: ir_y_mm,
                                        z_mm: ir_z_mm,
                                        span: (component.span.start, component.span.end - component.span.start).into(),
                                        suggestion,
                                    });

                                    // 2. ViolationCollector (for pattern detection)
                                    collector.report_violation("P44", "floating in air above substrate", group_context);

                                    return Ok(()); // Skip bbox registration — component is invalid
                                } else {
                                    collector.report(hwc_diagnostics::WaiverApplied::new(&format!("Component '{}' allowed to float in air", name)));
                                }
                            }

                            // Sprint 5.5: Check for buried component (below substrate)
                            if component_max_z < substrate_min_z {
                                let gap_nm = substrate_min_z - component_max_z;
                                let gap_mm = gap_nm as f64 / 1_000_000.0;

                                // Construct dynamic suggestion
                                let suggestion = format!(
                                    "To fix:\n- Place component at z:{substrate_max_layer} or higher (above substrate base)\n- Corrected: {}",
                                    original_line.replace(&format!("z: {}", component_z_layer), &format!("z: {}", substrate_max_layer))
                                );

                                // Sprint 9: Feed both systems
                                let ir_x_mm = untransformed_origin.x as f64 / 1_000_000.0;
                                let ir_y_mm = untransformed_origin.y as f64 / 1_000_000.0;
                                let ir_z_mm = untransformed_origin.z as f64 / 1_000_000.0;
                                collector.report(IrError::ComponentBuriedInSubstrate {
                                    component: name.clone(),
                                    component_z_layer,
                                    component_z_mm: component_min_z as f64 / 1_000_000.0,
                                    substrate_min_layer,
                                    substrate_min_mm: substrate_min_z as f64 / 1_000_000.0,
                                    substrate_max_layer,
                                    substrate_max_mm: substrate_max_z as f64 / 1_000_000.0,
                                    gap_mm,
                                    x_mm: ir_x_mm,
                                    y_mm: ir_y_mm,
                                    z_mm: ir_z_mm,
                                    span: (component.span.start, component.span.end - component.span.start).into(),
                                    suggestion,
                                });
                                collector.report_violation("P44", "buried below substrate base", group_context);

                                return Ok(()); // Skip bbox registration — component is invalid
                            }

                            // Original check: Overlap (component intersects substrate)
                            if component_min_z < substrate_max_z && component_max_z > substrate_min_z {
                                if skip_substrate_check {
                                    collector.report(hwc_diagnostics::WaiverApplied::new(&format!("Component '{}' allowed to overlap substrate", name)));
                                } else {
                                    let suggested_z_layer = substrate_max_layer + 1;

                                    // Construct dynamic suggestion
                                    let suggestion = format!(
                                        "To fix:\n- Place component at z:{suggested_z_layer} or higher (above substrate)\n- Corrected: {}\n\nAdvanced: Use 'merge: true' waiver if this is intentional.",
                                        original_line.replace(&format!("z: {}", component_z_layer), &format!("z: {}", suggested_z_layer))
                                    );

                                    // Sprint 9: Feed both systems
                                    let ir_x_mm = untransformed_origin.x as f64 / 1_000_000.0;
                                    let ir_y_mm = untransformed_origin.y as f64 / 1_000_000.0;
                                    let ir_z_mm = untransformed_origin.z as f64 / 1_000_000.0;
                                    collector.report(IrError::SubstrateOverlap {
                                        component: name.clone(),
                                        component_z_layer,
                                        component_z_mm: component_min_z as f64 / 1_000_000.0,
                                        substrate_min_layer,
                                        substrate_max_layer,
                                        substrate_min_mm: substrate_min_z as f64 / 1_000_000.0,
                                        substrate_max_mm: substrate_max_z as f64 / 1_000_000.0,
                                        suggested_z_layer,
                                        x_mm: ir_x_mm,
                                        y_mm: ir_y_mm,
                                        z_mm: ir_z_mm,
                                        span: (component.span.start, component.span.end - component.span.start).into(),
                                        suggestion,
                                    });
                                    collector.report_violation("P44", "overlaps with substrate material", group_context);

                                    return Ok(()); // Skip bbox registration — component is invalid
                                }
                            }
                        }

                    // Register bounding box for collision detection and 'last' keyword resolution
                    bbox_tracker.register(name.clone(), bbox, untransformed_origin);

                    // **Sprint 3.10: NATIVE ARCHITECTURE - Register bbox with HardwareSpace**
                    // This enables the SDF generator to access component bboxes directly
                    // without parameter threading through 15 function calls
                    // FIX: Must use ENGINE coordinates (position) not USER coordinates (untransformed_origin)
                    
                    // v0.1.7 FIX: Coordinate System Alignment
                    // For TL/TR origins, position.y is the TOP of the component.
                    // The engine's BoundingBox expects min/max where min < max.
                    let (min_y, max_y) = match origin.xy {
                        hwc_parser::OriginXY::TL | hwc_parser::OriginXY::TR => (position.y - height_nm, position.y),
                        hwc_parser::OriginXY::BL | hwc_parser::OriginXY::BR => (position.y, position.y + height_nm),
                    };

                    let (min_x, max_x) = match origin.xy {
                        hwc_parser::OriginXY::TL | hwc_parser::OriginXY::BL => (position.x, position.x + width_nm),
                        hwc_parser::OriginXY::TR | hwc_parser::OriginXY::BR => (position.x - width_nm, position.x),
                    };

                    let engine_bbox = if rotation_deg.abs() < 0.001 {
                        hwc_engine::geometry::BoundingBox::new(
                            hwc_engine::geometry::Point3D::new(min_x, min_y, position.z),
                            hwc_engine::geometry::Point3D::new(max_x, max_y, position.z + depth_nm),
                        )
                    } else {
                        let (center_x, center_y) = match origin.xy {
                            hwc_parser::OriginXY::TL => (position.x + width_nm / 2, position.y - height_nm / 2),
                            hwc_parser::OriginXY::TR => (position.x - width_nm / 2, position.y - height_nm / 2),
                            hwc_parser::OriginXY::BL => (position.x + width_nm / 2, position.y + height_nm / 2),
                            hwc_parser::OriginXY::BR => (position.x - width_nm / 2, position.y + height_nm / 2),
                        };
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
                        let mut final_min_x = i64::MAX;
                        let mut final_max_x = i64::MIN;
                        let mut final_min_y = i64::MAX;
                        let mut final_max_y = i64::MIN;
                        for (cx, cy) in corners.iter() {
                            let rx = (*cx as f64 * cos_theta - *cy as f64 * sin_theta) as i64;
                            let ry = (*cx as f64 * sin_theta + *cy as f64 * cos_theta) as i64;
                            let gx = center_x + rx;
                            let gy = match origin.xy {
                                hwc_parser::OriginXY::TL | hwc_parser::OriginXY::TR => center_y - ry,
                                hwc_parser::OriginXY::BL | hwc_parser::OriginXY::BR => center_y + ry,
                            };
                            final_min_x = final_min_x.min(gx);
                            final_max_x = final_max_x.max(gx);
                            final_min_y = final_min_y.min(gy);
                            final_max_y = final_max_y.max(gy);
                        }
                        hwc_engine::geometry::BoundingBox::new(
                            hwc_engine::geometry::Point3D::new(final_min_x, final_min_y, position.z),
                            hwc_engine::geometry::Point3D::new(final_max_x, final_max_y, position.z + depth_nm),
                        )
                    };

                    // v0.1.7: Resolve component body material
                    let material_id = space.material_registry.get_or_register("Component");
                    
                    space.register_component_bbox(
                        name.clone(), 
                        engine_bbox, 
                        material_id, 
                        component.component_type.name.clone(),
                        smallvec::SmallVec::new()
                    );
                }
            }

            // v0.1.6 Sprint 3: Register component pins for P43 validation
            // Transform pin positions from component-relative to absolute coordinates
            if !layout.pin_positions.is_empty() {
                // Fetch dimensions for rotation math
                let (width_nm, height_nm, _depth_nm) = layout.shape.as_ref()
                    .and_then(|s| parse_rectangle_dimensions(s))
                    .unwrap_or((1_000_000, 1_000_000, 1_000_000)); // Default 1mm if unknown

                // Get the component ID from the netlist
                 let component_id = space
                     .netlist
                     .get_component_by_name(&name)
                     .expect("Component should exist in netlist after placement");

                for (pin_name, pin_pos) in &layout.pin_positions {
                    // v0.1.6 Item #13: Get net assignment from pin_net_bindings (if provided)
                    // Priority:
                    // 1. Explicit net binding from component placement (net: [pin: NetName])
                    // 2. Internal pour net assignment (legacy behavior)
                    let net_assignment = if let Some(binding) =
                        component.pin_net_bindings.get(pin_name.as_str())
                    {
                        // Use explicit net binding from component placement
                        match binding {
                            hwc_parser::NetBinding::Simple(net_name) => Some(net_name.clone()),
                            hwc_parser::NetBinding::Conditional { .. } => {
                                // Conditional bindings should have been resolved during unrolling
                                eprintln!("⚠️  WARNING: Conditional net binding found during placement (should have been resolved during unrolling)");
                                None
                            }
                        }
                    } else {
                        // Fall back to internal pour net assignment (legacy behavior)
                        layout
                            .internal_pours
                            .iter()
                            .find(|pour| {
                                // Check if pour's net assignment exists and if pin is within pour bounds
                                pour.net.is_some()
                            })
                            .and_then(|pour| pour.net.as_ref())
                            .map(|net_id| net_id.base.clone())
                    };

                    // CRITICAL FIX (Sprint 3.9): Don't add pins here - ComponentPlacer already did that!
                    // We just need to find the existing pin and connect it to the net.

                    // Find the pin that ComponentPlacer already added
                    let pins = space.netlist.get_component_pins(component_id);
                    let pin_id = pins
                        .iter()
                        .find(|&&pid| {
                            if let Some(p) = space.netlist.get_pin(pid) {
                                p.name == *pin_name
                            } else {
                                false
                            }
                        })
                        .copied()
                        .unwrap_or_else(|| {
                            panic!(
                                "Pin '{}' should exist in netlist (added by ComponentPlacer)",
                                pin_name
                            )
                        });

                    // CRITICAL FIX (Sprint 3.9): NET MATERIALIZATION
                    if let Some(ref net_name_str) = net_assignment {
                        let net_id = if let Some(existing_net_id) =
                            space.netlist.get_net_by_name(net_name_str)
                        {
                            existing_net_id
                        } else {
                            space.netlist.add_net(
                                net_name_str.clone(),
                                100_000, // 100μm = 0.1mm trace width
                                2,       // Copper (MaterialId 2)
                            )
                        };

                        // Connect the pin to the net
                        space.netlist.connect_pin(pin_id, net_id);
                    }

                    // v0.1.7: ROTATION FIX for pins
                    // We must calculate the absolute position using the same logic as the engine's ComponentPlacer.
                    // This ensures the VoxelGrid pins match the physical component geometry.
                    
                    // CRITICAL FIX: The 'position' from coordinate_to_point is the anchor point.
                    // For OriginXY::TL, position.y is the MAX Y (top) of the component in engine space.
                    // To get the center, we must shift INWARD based on the origin type.
                    let (center_x, center_y) = match origin.xy {
                        hwc_parser::OriginXY::TL => (position.x + width_nm / 2, position.y - height_nm / 2),
                        hwc_parser::OriginXY::TR => (position.x - width_nm / 2, position.y - height_nm / 2),
                        hwc_parser::OriginXY::BL => (position.x + width_nm / 2, position.y + height_nm / 2),
                        hwc_parser::OriginXY::BR => (position.x - width_nm / 2, position.y + height_nm / 2),
                    };
                    
                    let half_w = width_nm / 2;
                    let half_h = height_nm / 2;
                    let angle_rad = (rotation_deg as f64).to_radians();
                    let cos_theta = angle_rad.cos();
                    let sin_theta = angle_rad.sin();

                    // Pin offset from component center in user-space coordinates
                    // (where X is right, Y is down)
                    let lx = (pin_pos.x * 1_000_000.0) as i64 - half_w;
                    let ly = (pin_pos.y * 1_000_000.0) as i64 - half_h;

                    // Apply rotation
                    let rx = (lx as f64 * cos_theta - ly as f64 * sin_theta) as i64;
                    let ry = (lx as f64 * sin_theta + ly as f64 * cos_theta) as i64;

                    // Apply to center. 
                    // Note: ry must be inverted if the coordinate system Y is inverted.
                    // For TL origin, Engine Y = SpaceHeight - User Y. So User Y increasing (down) 
                    // means Engine Y decreasing.
                    let absolute_x_nm = center_x + rx;
                    let absolute_y_nm = match origin.xy {
                        hwc_parser::OriginXY::TL | hwc_parser::OriginXY::TR => center_y - ry,
                        hwc_parser::OriginXY::BL | hwc_parser::OriginXY::BR => center_y + ry,
                    };
                    let absolute_z_nm = position.z + (pin_pos.z.unwrap_or(0.0) * 1_000_000.0) as i64;

                    // v0.1.7: Auto-Stitching (Limitation 7) - Drill and plate through-holes
                    let is_tht = component_def.render.as_ref().map(|r| r.shape.as_deref() == Some("tht_package")).unwrap_or(false);
                    let pad_shape = layout.pad_shapes.get(pin_name);
                    
                    if is_tht || pad_shape.is_some() {
                        let drill_diameter_nm = if let Some(ps) = pad_shape {
                            if ps.starts_with("Circle(") {
                                 let val_str = ps.trim_start_matches("Circle(").trim_end_matches(")");
                                 (val_str.trim_end_matches("mm").parse::<f64>().unwrap_or(1.0) * 1_000_000.0) as i64
                            } else {
                                1_000_000
                            }
                        } else {
                            1_000_000
                        };

                        if let Some(substrate_bbox) = space.substrate_bbox {
                            let hole_bbox = hwc_engine::geometry::BoundingBox::new(
                                hwc_engine::geometry::Point3D::new(absolute_x_nm - drill_diameter_nm / 2, absolute_y_nm - drill_diameter_nm / 2, substrate_bbox.min.z),
                                hwc_engine::geometry::Point3D::new(absolute_x_nm + drill_diameter_nm / 2, absolute_y_nm + drill_diameter_nm / 2, substrate_bbox.max.z),
                            );

                            space.drill_hole(hole_bbox, Some(drill_diameter_nm));

                            let copper_material_id = space.material_registry.get_id("Copper").unwrap_or(2);
                            let plating_thickness_nm = 25_000;
                            let outer_diameter_nm = drill_diameter_nm;
                            let inner_diameter_nm = drill_diameter_nm - (2 * plating_thickness_nm);
                            
                            let min_annular_ring_nm = space.fabrication_constraints.as_ref()
                                .map(|c| c.via.min_annular_ring_nm)
                                .unwrap_or(150_000);
                            
                            let pad_diameter_nm = drill_diameter_nm + (2 * min_annular_ring_nm);

                            // Get net_id for the via/pads
                            let via_net_id = if let Some(ref net_name_str) = net_assignment {
                                space.netlist.get_net_by_name(net_name_str).unwrap_or(hwc_engine::netlist::NetId::new(0))
                            } else {
                                hwc_engine::netlist::NetId::new(0)
                            };

                            space.voxel_grid.add_tube_substrate_layer(
                                copper_material_id,
                                via_net_id.raw(),
                                hole_bbox,
                                outer_diameter_nm as u32,
                                inner_diameter_nm as u32,
                                pad_diameter_nm as u32,
                                16,
                                true
                            );

                            let pad_half_nm = pad_diameter_nm / 2;
                            let start_z_nm = (substrate_bbox.min.z / space.voxel_size.z_nm) * space.voxel_size.z_nm;
                            let pad_bbox_start = hwc_engine::geometry::BoundingBox::new(
                                hwc_engine::geometry::Point3D::new(absolute_x_nm - pad_half_nm, absolute_y_nm - pad_half_nm, start_z_nm),
                                hwc_engine::geometry::Point3D::new(absolute_x_nm + pad_half_nm, absolute_y_nm + pad_half_nm, start_z_nm + space.voxel_size.z_nm),
                            );
                            space.voxel_grid.add_cylinder_substrate_layer(
                                copper_material_id,
                                via_net_id.raw(),
                                pad_bbox_start,
                                pad_diameter_nm,
                                16,
                                0
                            );

                            let end_z_nm = (substrate_bbox.max.z / space.voxel_size.z_nm - 1) * space.voxel_size.z_nm;
                            let pad_bbox_end = hwc_engine::geometry::BoundingBox::new(
                                hwc_engine::geometry::Point3D::new(absolute_x_nm - pad_half_nm, absolute_y_nm - pad_half_nm, end_z_nm),
                                hwc_engine::geometry::Point3D::new(absolute_x_nm + pad_half_nm, absolute_y_nm + pad_half_nm, end_z_nm + space.voxel_size.z_nm),
                            );
                            space.voxel_grid.add_cylinder_substrate_layer(
                                copper_material_id,
                                via_net_id.raw(),
                                pad_bbox_end,
                                pad_diameter_nm,
                                16,
                                0
                            );
                            
                            let board_max_z_nm = (space.grid.z_layers as i64).saturating_sub(1) * space.voxel_size.z_nm;
                            let via = hwc_engine::geometry_router::Via::new(
                                (absolute_x_nm, absolute_y_nm),
                                substrate_bbox.min.z,
                                board_max_z_nm,
                                drill_diameter_nm,
                                via_net_id,
                                0,
                                board_max_z_nm,
                                space.voxel_size.z_nm,
                            );
                            space.add_vias(vec![via]);

                            space.contacts.push(hwc_engine::space::ContactMetadata {
                                name: format!("{}_{}_via", name, pin_name).into(),
                                material_name: "Copper".into(),
                                z_start_nm: substrate_bbox.min.z,
                                z_end_nm: substrate_bbox.max.z,
                                net: net_assignment.clone(),
                                bridge: None,
                                bbox: Some(hole_bbox),
                                voxels: Vec::new(),
                            });
                        }
                    }

                    // v0.1.7: DELETED redundant add_component_pin call.
                    // The engine's ComponentPlacer already registers these in the Netlist.
                    // Double-registration was causing "Ghost Nets" and out-of-bounds traces.
                    
                    // RE-RE-FIX: voxel_grid.get_component_pins() depends on this registration 
                    // for the Global Router to find nets. We MUST register it here, but
                    // ensure place_pour doesn't double-register it.
                    space.voxel_grid.add_component_pin(
                        absolute_x_nm,
                        absolute_y_nm,
                        absolute_z_nm,
                        name.clone().into(),
                        pin_name.clone(),
                        net_assignment.clone()
                    );

                    // Debug output removed for production - pin registration is working correctly
                }
            }
        }
    }

    Ok(())
}
