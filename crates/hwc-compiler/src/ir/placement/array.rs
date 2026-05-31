//! Array placement functionality.
//!
//! Sprint 3, Task 3.2: Array Flows (Transistor Fingers)

use super::super::conversions::CoordinateContext;
use super::super::errors::IrError;
use super::super::stackup_manager::StackupManager;
use super::component::place_component;
use super::coordinate_evaluation::evaluate_measurement_to_nm;
use super::helpers::offset_coordinate;
use crate::SymbolTable;
use hwc_engine::HardwareSpace;

/// Context for component array placement operations.
/// Groups related parameters to avoid exceeding Clippy's argument limit.
pub struct ArrayPlacementContext<'a> {
    pub origin: hwc_parser::OriginPoint,
    pub symbol_table: &'a SymbolTable,
    pub layouts: &'a [hwc_parser::ModuleLayoutBlock],
    pub bbox_tracker: &'a mut crate::bounding_box_tracker::BoundingBoxTracker,
    pub eval_context: &'a hwc_parser::EvaluationContext,
    pub collector: &'a hwc_diagnostics::DiagnosticCollector,
    pub stackup_manager: &'a StackupManager,
    pub profile: Option<&'a hwc_parser::ProfileDefinition>,
}

/// Place a component array by unrolling it into individual instances
///
/// This function expands an array placement like:
/// ```hw
/// add TransistorFinger[4] named M1_Array at [x: 10um, y: 20um, z: 1]:
///     layout: horizontal_stack
///     pitch: 3um
///     shared_terminals: [source, drain]
/// ```
///
/// Into 4 individual component placements with calculated positions.
pub fn place_component_array(
    space: &mut HardwareSpace,
    component: &hwc_parser::ComponentPlacement,
    array_config: &hwc_parser::ArrayConfig,
    ctx: &mut ArrayPlacementContext,
) -> Result<(), IrError> {
    // println!($3"[DEBUG] Unrolling array: {} instances of {} (layout: {:?}, pitch: {:?})",
    // array_config.count,
    // component.component_type,
    // array_config.layout,
    //    array_config.pitch
    //   );

    // Evaluate pitch to nanometers
    let pitch_nm = evaluate_measurement_to_nm(&array_config.pitch, ctx.symbol_table)?;

    // For each instance in the array
    for i in 0..array_config.count {
        // Calculate offset based on layout strategy
        let (offset_x_nm, offset_y_nm) = match array_config.layout {
            hwc_parser::ArrayLayout::HorizontalStack => {
                // Stack along X-axis
                (i as i64 * pitch_nm, 0)
            }
            hwc_parser::ArrayLayout::VerticalStack => {
                // Stack along Y-axis
                (0, i as i64 * pitch_nm)
            }
            hwc_parser::ArrayLayout::Grid { rows: _, cols: _ } => {
                // TODO: Implement 2D grid layout
                return Err(IrError::PlacementError(
                    "Grid layout not yet implemented for arrays".into(),
                ));
            }
        };

        // Create a new position by adding the offset to the base position
        let instance_position = offset_coordinate(&component.position, offset_x_nm, offset_y_nm)?;

        // Generate instance name: ArrayName[i]
        let instance_name = component
            .name
            .as_ref()
            .map(|n| format!("{}[{}]", n, i))
            .or_else(|| Some(format!("{}[{}]", component.component_type, i)));

        // Create a new component placement for this instance
        let instance_component = hwc_parser::ComponentPlacement {
            component_type: component.component_type.clone(),
            parameters: component.parameters.clone(),
            name: instance_name.map(|s| hwc_parser::ComponentName {
                base: s.into(),
                index: None,
                span: component.span,
            }),
            position: instance_position,
            rotation: component.rotation.clone(),
            elevation: component.elevation.clone(),
            array_config: None, // Don't recursively unroll
            pin_net_bindings: component.pin_net_bindings.clone(), // v0.1.6 Item #13: Preserve net bindings
            // v0.1.7: Unified waivers (removed legacy boolean flags)
            waivers: hwc_parser::Waivers {
                merge: if !array_config.merge_terminals.is_empty() {
                    hwc_parser::MergeWaiver::Specific(array_config.merge_terminals.clone())
                } else {
                    component.waivers.merge.clone()
                },
                floating: component.waivers.floating,
                isolated: component.waivers.isolated,
                snap_to_surface: component.waivers.snap_to_surface,
                virtual_component: component.waivers.virtual_component,
                locked: component.waivers.locked,
            },
            span: component.span,
        };

        // Place this instance (recursive call without array_config)
        place_component(
            space,
            &instance_component,
            ctx.origin,
            ctx.symbol_table,
            ctx.layouts,
            ctx.bbox_tracker,
            ctx.eval_context,
            ctx.collector,
            ctx.stackup_manager,
            ctx.profile,
        )?;
    }

    // v0.1.7: Register array instances for topological routing
    // This handles unrolling the array into individual component bboxes
    // so that other components can reference M1_Array[2].left correctly.
    validate_array_collisions(
        space,
        component,
        array_config,
        pitch_nm,
        ctx.origin,
        ctx.symbol_table,
        ctx.stackup_manager,
        ctx.profile,
    )?;

    // v0.1.7: Merge explicit terminals
    if !array_config.merge_terminals.is_empty() {
        merge_explicit_terminals(
            space,
            component,
            array_config,
            ctx.origin,
            ctx.symbol_table,
            ctx.layouts,
            ctx.stackup_manager,
            ctx.profile,
        )?;
    }

    Ok(())
}

/// v0.1.7: P12 Collision Detector for Component Arrays
///
/// Ensures that transistor fingers (or other arrayed components) do not overlap
/// their internal terminal pours (e.g. source/drain) unless explicitly waived.
///
/// This catches pitch miscalculations BEFORE they hit the engine, providing
/// beautiful hwc-style suggestions for the correct pitch.
fn validate_array_collisions(
    space: &HardwareSpace,
    component: &hwc_parser::ComponentPlacement,
    array_config: &hwc_parser::ArrayConfig,
    pitch_nm: i64,
    origin: hwc_parser::OriginPoint,
    symbol_table: &SymbolTable,
    stackup_manager: &StackupManager,
    profile: Option<&hwc_parser::ProfileDefinition>,
) -> Result<(), IrError> {
    // Get component definition
    let comp_def = symbol_table.get_component(&component.component_type.name)?;
    let layout = comp_def
        .layout
        .as_ref()
        .ok_or_else(|| IrError::PlacementError(format!("Component '{}' missing layout", comp_def.name)))?;

    // Check each internal pour for potential overlaps
    for pour in &layout.internal_pours {
        // Skip pours that are meant to be shared (merging)
        let terminal_name = pour
            .device
            .as_ref()
            .map(|d| d.terminal.as_str())
            .unwrap_or("");
        if array_config.merge_terminals.iter().any(|t| t == terminal_name) {
            continue;
        }

        // Calculate bounding boxes for this pour across all instances
        let instance_bboxes = calculate_pour_bboxes_for_array(
            space,
            component,
            array_config,
            pour,
            pitch_nm,
            origin,
            symbol_table,
            stackup_manager,
            profile,
        )?;

        // Check for overlaps between adjacent instances
        for i in 0..instance_bboxes.len() {
            for j in (i + 1)..instance_bboxes.len() {
                let bbox_a = &instance_bboxes[i].1;
                let bbox_b = &instance_bboxes[j].1;

                if bbox_a.intersects(bbox_b) {
                    // COLLISION DETECTED! Throw P12 error
                    let array_name = component
                        .name
                        .as_ref()
                        .map(|n| n.base.as_str())
                        .unwrap_or(&component.component_type.name);

                    // Calculate suggested pitch (current pour width + 10% safety margin)
                    let pour_width = (bbox_a.max.x - bbox_a.min.x).max(bbox_a.max.y - bbox_a.min.y);
                    let suggested_pitch_nm = (pour_width as f64 * 1.1) as i64;

                    return Err(IrError::GeometricCollision(Box::new(
                        crate::ir::errors::GeometricCollisionDetails {
                            array_name: array_name.into(),
                            instance_a: i,
                            instance_b: j,
                            pour_name: pour.name.to_string(),
                            terminal_name: terminal_name.into(),
                            bbox_a_min_x: bbox_a.min.x as f64 / 1_000_000.0,
                            bbox_a_min_y: bbox_a.min.y as f64 / 1_000_000.0,
                            bbox_a_max_x: bbox_a.max.x as f64 / 1_000_000.0,
                            bbox_a_max_y: bbox_a.max.y as f64 / 1_000_000.0,
                            bbox_b_min_x: bbox_b.min.x as f64 / 1_000_000.0,
                            bbox_b_min_y: bbox_b.min.y as f64 / 1_000_000.0,
                            bbox_b_max_x: bbox_b.max.x as f64 / 1_000_000.0,
                            bbox_b_max_y: bbox_b.max.y as f64 / 1_000_000.0,
                            current_pitch: pitch_nm as f64 / 1_000_000.0,
                            suggested_pitch: suggested_pitch_nm as f64 / 1_000_000.0,
                        },
                    )));
                }
            }
        }
    }

    Ok(())
}

/// Calculate bounding boxes for a pour across all array instances.
///
/// Helper function used by both collision detection and merging logic.
fn calculate_pour_bboxes_for_array(
    space: &HardwareSpace,
    _component: &hwc_parser::ComponentPlacement,
    array_config: &hwc_parser::ArrayConfig,
    pour: &hwc_parser::PourPlacement,
    pitch_nm: i64,
    origin: hwc_parser::OriginPoint,
    symbol_table: &SymbolTable,
    stackup_manager: &StackupManager,
    profile: Option<&hwc_parser::ProfileDefinition>,
) -> Result<Vec<(usize, hwc_engine::geometry::BoundingBox)>, IrError> {
    use crate::ir::conversions::spanning_coordinate_to_point;
    use hwc_engine::geometry::{BoundingBox, Point3D};

    // Get pour boundary
    let (from, to) = pour
        .boundary
        .as_ref()
        .ok_or_else(|| IrError::PlacementError(format!("Pour '{}' missing boundary", pour.name)))?;

    let mut instance_bboxes = Vec::new();

    for i in 0..array_config.count {
        // Calculate offset for this instance
        let (offset_x_nm, offset_y_nm) = match array_config.layout {
            hwc_parser::ArrayLayout::HorizontalStack => (i as i64 * pitch_nm, 0),
            hwc_parser::ArrayLayout::VerticalStack => (0, i as i64 * pitch_nm),
            hwc_parser::ArrayLayout::Grid { .. } => {
                return Err(IrError::PlacementError(
                    "Grid layout not yet implemented for collision detection".into(),
                ));
            }
        };

        // Convert pour boundary to absolute coordinates
        let ctx = CoordinateContext {
            voxel_size: &space.voxel_size,
            grid_size: &space.grid,
            origin,
            space_dimensions: &space.dimensions,
            symbol_table,
            eval_context: &hwc_parser::EvaluationContext::default(),
            bbox_tracker: None, // array pours don't use anchor references
            stackup_manager,
            profile,
        };
        let start = spanning_coordinate_to_point(from, &ctx, false)
            .map_err(|e| IrError::PlacementError(e))?;

        let end = spanning_coordinate_to_point(to, &ctx, true)
            .map_err(|e| IrError::PlacementError(e))?;

        let z_bottom_nm = stackup_manager.resolve_elevation(&pour.elevation, symbol_table)?;
        let z_top_nm = stackup_manager.resolve_elevation_top(
            &pour.elevation,
            symbol_table,
            space.voxel_size.z_nm,
        )?;

        let instance_start = Point3D::new(
            start.x + offset_x_nm,
            start.y + offset_y_nm,
            z_bottom_nm,
        );

        let instance_end = Point3D::new(
            end.x + offset_x_nm,
            end.y + offset_y_nm,
            z_top_nm,
        );

        let bbox = BoundingBox::new(instance_start, instance_end);
        instance_bboxes.push((i, bbox));
    }

    Ok(instance_bboxes)
}

/// Merge overlapping pours for explicitly declared terminals in component arrays.
///
/// **Philosophy**: EXPLICIT INTENT (Hardware Script Manifesto - No Hidden Magic)
///
/// This function implements geometry merging ONLY when the user explicitly declares
/// `merge: [terminal_list]` in the array configuration. Without this declaration,
/// overlapping geometry triggers P12: Geometric Collision Error.
///
/// # Algorithm
/// 1. For each terminal in `merge:` list (e.g., "source", "drain"):
///    - Find all internal pours with device binding to that terminal
///    - Detect overlapping regions between adjacent instances
///    - Perform Bitwise-OR voxel melting (merge overlapping pours)
/// 2. For terminals NOT in `merge:` list:
///    - Normal collision detection applies (P12 error if overlap)
///
/// # Benefits of Explicit Intent
/// - **Documented Design**: Code shows this is a shared-junction design
/// - **No Accidental Merges**: Wrong pitch → compiler error (not silent fix)
/// - **Simulation Precision**: SPICE exporter generates single node for merged area
/// - **Physical Reality First**: User is master of the electrons
///
/// # Example
/// ```hw
/// add NMOS[4] named M_Array at [x: 10um, y: 10um, z: 1]:
///     layout: horizontal_stack
///     pitch: 800nm
///     merge: [source, drain]  # EXPLICIT: "I know these overlap. Melt them."
/// ```
/// Result: 4 transistors with merged source/drain regions (no P12 collision error)
fn merge_explicit_terminals(
    space: &mut HardwareSpace,
    component: &hwc_parser::ComponentPlacement,
    array_config: &hwc_parser::ArrayConfig,
    origin: hwc_parser::OriginPoint,
    symbol_table: &SymbolTable,
    _layouts: &[hwc_parser::ModuleLayoutBlock],
    stackup_manager: &StackupManager,
    profile: Option<&hwc_parser::ProfileDefinition>,
) -> Result<(), IrError> {
    // Get component definition to access internal pours
    let component_def = symbol_table
        .get_component(&component.component_type.name)?;

    let layout = component_def.layout.as_ref().ok_or_else(|| {
        IrError::PlacementError(format!(
            "Component '{}' has no layout block",
            component.component_type.name
        ))
    })?;

    // Evaluate pitch to nanometers
    let pitch_nm = evaluate_measurement_to_nm(&array_config.pitch, symbol_table)?;

    // For each explicitly merged terminal, find and merge overlapping pours
    for terminal_name in &array_config.merge_terminals {
        // Find all internal pours that match this terminal
        // Match by: device binding terminal OR pour name
        let terminal_pours: Vec<_> = layout
            .internal_pours
            .iter()
            .filter(|pour| {
                // Check device binding first
                if let Some(binding) = &pour.device {
                    binding.terminal == *terminal_name
                } else {
                    // Fall back to pour name
                    pour.name.as_str() == terminal_name
                }
            })
            .collect();

        if terminal_pours.is_empty() {
            continue;
        }

        // For each pour that binds to this terminal, merge across instances
        for pour in terminal_pours {
            merge_pour_across_instances(
                space,
                component,
                array_config,
                pour,
                origin,
                symbol_table,
                pitch_nm,
                stackup_manager,
                profile,
            )?;
        }
    }

    Ok(())
}

/// Merge a specific pour across array instances where it overlaps.
///
/// This performs the actual Bitwise-OR voxel melting for explicitly merged terminals.
fn merge_pour_across_instances(
    space: &mut HardwareSpace,
    component: &hwc_parser::ComponentPlacement,
    array_config: &hwc_parser::ArrayConfig,
    pour: &hwc_parser::PourPlacement,
    origin: hwc_parser::OriginPoint,
    symbol_table: &SymbolTable,
    pitch_nm: i64,
    stackup_manager: &StackupManager,
    profile: Option<&hwc_parser::ProfileDefinition>,
) -> Result<(), IrError> {
    // Calculate bounding boxes for this pour in each instance
    let instance_bboxes = calculate_pour_bboxes_for_array(
        space,
        component,
        array_config,
        pour,
        pitch_nm,
        origin,
        symbol_table,
        stackup_manager,
        profile,
    )?;

    // Detect overlapping regions and merge them
    let mut merged_regions = Vec::new();
    let mut merged_indices = rustc_hash::FxHashSet::default();

    for i in 0..instance_bboxes.len() {
        if merged_indices.contains(&i) {
            continue; // Already merged
        }

        let mut current_bbox = instance_bboxes[i].1;
        let mut merged_group = vec![i];

        // Check if next instance overlaps
        for (offset, (_, next_bbox)) in instance_bboxes.iter().enumerate().skip(i + 1) {
            let j = offset;
            if merged_indices.contains(&j) {
                continue;
            }

            if current_bbox.intersects(next_bbox) {
                // Merge the bounding boxes
                current_bbox = current_bbox.union(next_bbox);
                merged_group.push(j);
                merged_indices.insert(j);
            } else {
                break; // No more overlaps in this direction
            }
        }

        merged_indices.insert(i);
        merged_regions.push((merged_group, current_bbox));
    }

    // println!($3"[DEBUG]       Pour '{}': {} instances merged into {} region(s)",
    // pour.name,
    // array_config.count,
    //  merged_regions.len()
    //   );

    // Place the merged regions as substrate layers
    let material_id = space.material_registry.get_or_register(&pour.material);
    let net_id = if let Some(net_name) = &pour.net {
        if let Some(net) = space.netlist.get_net_by_name(net_name.base.as_str()) {
            net.raw()
        } else {
            // Create net if it doesn't exist
            let net = space
                .netlist
                .add_net(net_name.to_string(), 100_000, material_id);
            net.raw()
        }
    } else {
        0 // Unassigned
    };

    // Place each merged region
    for (group_indices, bbox) in merged_regions {
        let merged_name = if group_indices.len() == 1 {
            format!(
                "{}[{}].{}",
                component
                    .name
                    .as_ref()
                    .map(|n| n.base.as_str())
                    .unwrap_or(&component.component_type.name),
                group_indices[0],
                pour.name
            )
        } else {
            format!(
                "{}[{}-{}].{}",
                component
                    .name
                    .as_ref()
                    .map(|n| n.base.as_str())
                    .unwrap_or(&component.component_type.name),
                group_indices[0],
                group_indices[group_indices.len() - 1],
                pour.name
            )
        };

        // Create canonical merged region ID for parasitic extraction
        // All pours in this merged region will share this ID
        let merged_region_id = if group_indices.len() > 1 {
            Some(merged_name.clone().into())
        } else {
            None // Single instance, not actually merged
        };

        // println!($3"[DEBUG]         Merged region '{}': [{:.3}mm, {:.3}mm, {:.3}mm] to [{:.3}mm, {:.3}mm, {:.3}mm]",
        // merged_name,
        // bbox.min.x as f64 / 1_000_000.0,
        // bbox.min.y as f64 / 1_000_000.0,
        // bbox.min.z as f64 / 1_000_000.0,
        // bbox.max.x as f64 / 1_000_000.0,
        // bbox.max.y as f64 / 1_000_000.0,
        // bbox.max.z as f64 / 1_000_000.0,
        // );

        // Use ComponentPlacer to place the merged substrate
        use hwc_engine::ComponentPlacer;
        let placer = ComponentPlacer::new();
        placer
            .place_substrate(
                &mut space.voxel_grid,
                &space.voxel_size,
                material_id,
                bbox.min,
                bbox.max,
                net_id,
            )
            .map_err(|e| {
                IrError::PlacementError(format!(
                    "Failed to place merged pour '{}': {}",
                    merged_name, e
                ))
            })?;

        // Register pour metadata for BOM
        let area_nm2 = (bbox.max.x - bbox.min.x) * (bbox.max.y - bbox.min.y);
        space.pours.push(hwc_engine::space::PourMetadata {
            name: merged_name.into(),
            material_name: pour.material.clone(),
            z_bottom_nm: stackup_manager.resolve_elevation(&pour.elevation, symbol_table)?,
            net: pour.net.as_ref().map(|n| n.to_string()),
            area_nm2,
            bbox: Some(bbox),
            device_binding: pour
                .device
                .as_ref()
                .map(|binding| hwc_engine::space::DeviceBinding {
                    device_name: binding.device_name.clone(),
                    terminal: binding.terminal.clone(),
                }),
            merged_region_id,
            waivers: pour.waivers.clone(), // v0.1.7: Preserve waivers in merged region
        });
    }

    Ok(())
}
