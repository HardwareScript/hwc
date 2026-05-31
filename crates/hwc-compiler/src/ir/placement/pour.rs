//! Pour (material region) placement functionality.

use super::super::conversions::{spanning_coordinate_to_point, CoordinateContext};
use super::super::errors::IrError;
use crate::ir::stackup_manager::StackupManager;
use hwc_engine::space::PourMetadata;
use hwc_engine::{HardwareSpace, Point3D};

/// Place a pour (material region) in the voxel grid.
pub fn place_pour(
    space: &mut HardwareSpace,
    pour: &hwc_parser::PourPlacement,
    origin: hwc_parser::OriginPoint,
    symbol_table: &crate::SymbolTable,
    bbox_tracker: &mut crate::bounding_box_tracker::BoundingBoxTracker,
    eval_context: &hwc_parser::EvaluationContext,
    collector: &hwc_diagnostics::DiagnosticCollector,
    stackup_manager: &StackupManager,
    profile: Option<&hwc_parser::ProfileDefinition>,
) -> Result<(), IrError> {
    // Get or register the pour material in the material registry
    let material_id = space.material_registry.get_or_register(&pour.material);

    // Get the boundary coordinates
    let (from_raw, to_raw) = pour
        .boundary
        .as_ref()
        .ok_or_else(|| IrError::PlacementError(format!("Pour '{}' missing boundary", pour.name)))?;

    // Sprint 3, Task 3.1: Resolve relative coordinates to absolute
    let solver = crate::constraint_solver::ConstraintSolver::new(bbox_tracker, eval_context);
    
    let from = if from_raw.is_relative() {
        solver.resolve_position(from_raw).map_err(|e| {
            IrError::PlacementError(format!("Failed to resolve relative 'from' position for pour '{}': {}", pour.name, e))
        })?
    } else {
        from_raw.clone()
    };

    let to = if to_raw.is_relative() {
        solver.resolve_position(to_raw).map_err(|e| {
            IrError::PlacementError(format!("Failed to resolve relative 'to' position for pour '{}': {}", pour.name, e))
        })?
    } else {
        to_raw.clone()
    };

    let z_start_nm = stackup_manager.resolve_elevation(&pour.elevation, symbol_table)?;
    let z_end_nm = stackup_manager.resolve_elevation_top(
        &pour.elevation,
        symbol_table,
        space.voxel_size.z_nm,
    )?;

    // Create 3D coordinates by combining boundary x,y with elevation z
    let ctx = CoordinateContext {
        voxel_size: &space.voxel_size,
        grid_size: &space.grid,
        origin,
        space_dimensions: &space.dimensions,
        symbol_table,
        eval_context,
        bbox_tracker: Some(bbox_tracker), // Pass bbox_tracker for anchor references in pour boundaries
        stackup_manager,
        profile,
    };
    let start = spanning_coordinate_to_point(&from, &ctx, false)
        .map_err(|e| IrError::PlacementError(e))?;
    let end = spanning_coordinate_to_point(&to, &ctx, true)
        .map_err(|e| IrError::PlacementError(e))?;

    // Z from elevation (physical or semantic); XY from pour boundary
    let start_with_z = Point3D::new(start.x, start.y, z_start_nm);
    let end_with_z = Point3D::new(end.x, end.y, z_end_nm);

    // Calculate area for BOM
    let width_nm = (end.x - start.x).abs();
    let height_nm = (end.y - start.y).abs();
    let area_nm2 = width_nm * height_nm;

    // Create bounding box for geometric overlap detection
    let bbox = hwc_engine::geometry::BoundingBox::new(start_with_z, end_with_z);

    // Sprint 3, Task 7: Register pour bounding box for relative positioning
    // This enables syntax like: `at PourName.right + 1mm`
    bbox_tracker.register(pour.name.to_string(), bbox, start_with_z);

    println!(
        "   ├─ Registered pour '{}' bbox: min=({:.3}, {:.3}, {:.3}) max=({:.3}, {:.3}, {:.3})",
        pour.name,
        start_with_z.x as f64 / 1_000_000.0,
        start_with_z.y as f64 / 1_000_000.0,
        start_with_z.z as f64 / 1_000_000.0,
        end_with_z.x as f64 / 1_000_000.0,
        end_with_z.y as f64 / 1_000_000.0,
        end_with_z.z as f64 / 1_000_000.0,
    );

    // FIX A.2: SUBSTRATE INTERPENETRATION DETECTION
    // Check if this pour overlaps with the base substrate
    // v0.1.7: merge: true waives substrate interpenetration
    let skip_substrate_check = pour.waivers.merge == hwc_parser::MergeWaiver::All;

    if let Some(substrate_bbox) = &space.substrate_bbox {
        if bbox.intersects(substrate_bbox) && !skip_substrate_check {
            // This pour overlaps with the substrate - check if it's the same material
            if space.substrate_material_id != material_id {
                // AUTO-CARVE (v0.1.7): If a conductor (Copper) overlaps an insulator (FR4) or 
                // semiconductor (Silicon), automatically carve the substrate instead of erroring.
                let is_conductor = space.material_registry.is_conductor(material_id);
                let is_substrate_insulator = space.material_registry.is_insulator(space.substrate_material_id) 
                                           || space.material_registry.is_semiconductor(space.substrate_material_id);

                if is_conductor && is_substrate_insulator {
                    // Carve the substrate!
                    // Memory usage: O(N) where N is number of substrate layers
                    // v0.1.7: Drill is net-aware. Substrate has net 0, so it will always be carved.
                    // If we overlap another pour on the same net, it will NOT be carved.
                    let pour_net_id = if let Some(net_name) = &pour.net {
                        space.netlist.get_net_by_name(net_name.base.as_str()).unwrap_or(hwc_engine::netlist::NetId::new(0))
                    } else {
                        hwc_engine::netlist::NetId::new(0)
                    };
                    space.voxel_grid.drill_hole(bbox, None, pour_net_id.raw());
                    println!("   ├─ Auto-carved substrate for pour '{}' ({})", pour.name, pour.material);
                } else {
                    let substrate_material_name = space
                        .material_registry
                        .get_name(space.substrate_material_id)
                        .unwrap_or("Unknown");

                    return Err(IrError::PlacementError(format!(
                        "Substrate interpenetration detected: Pour '{}' ({}) overlaps with the base substrate ({}). \
                         Use the same material as the substrate, or place the pour outside the substrate bounds.",
                        pour.name,
                        pour.material,
                        substrate_material_name
                    )));
                }
            }
        }
    }

    // Check for pour-vs-pour interpenetration
    for existing in &space.pours {
        if let Some(existing_bbox) = &existing.bbox {
            if bbox.intersects(existing_bbox) {
                let z_overlap = bbox.max.z > existing_bbox.min.z
                    && existing_bbox.max.z > bbox.min.z;
                if z_overlap {
                    // If they have different materials, this is a physical violation
                    // v0.1.7: merge: true waives pour-vs-pour interpenetration
                    let is_waived = pour.waivers.merge == hwc_parser::MergeWaiver::All;

                    if existing.material_name != pour.material {
                        if is_waived {
                            collector.report(hwc_diagnostics::WaiverApplied::new(&format!("Pour '{}' (mat: {}) allowed to overlap '{}' (mat: {})", 
                                pour.name, pour.material, existing.name, existing.material_name)));
                        } else {
                            return Err(IrError::MaterialInterpenetration {
                                pour_a: existing.name.clone(),
                                mat_a: existing.material_name.clone(),
                                pour_b: pour.name.to_string(),
                                mat_b: pour.material.clone(),
                                z_nm: z_start_nm,
                            });
                        }
                    }
                }
            }
        }
    }
    // -----------------------------------------

    // Phase 4: Convert AST device binding to engine device binding
    let device_binding = pour
        .device
        .as_ref()
        .map(|binding| hwc_engine::space::DeviceBinding {
            device_name: binding.device_name.clone(),
            terminal: binding.terminal.clone(),
        });

    // 1. Resolve net name for unrolled component pours (v0.1.7)
    let mut resolved_net_name = pour.net.as_ref().map(|n| n.base.clone());

    if let Some(binding) = &pour.device {
        let resolved_opt = (|| {
            let netlist = &space.netlist;
            let comp_id = netlist.get_component_by_name(binding.device_name.as_str())?;
            let pins = netlist.get_component_pins(comp_id);

            pins.iter().find_map(|&pin_id| {
                let pin_data = netlist.get_pin(pin_id)?;
                if pin_data.name == binding.terminal {
                    let net_id = pin_data.connected_net?;
                    let net_data = netlist.get_net(net_id)?;
                    Some(net_data.name.to_string())
                } else {
                    None
                }
            })
        })();

        if let Some(net_name) = resolved_opt {
            resolved_net_name = Some(net_name.into());
        }
    }

    // Register pour metadata for BOM and netlist generation
    space.pours.push(PourMetadata {
        name: pour.name.to_string(),
        material_name: pour.material.clone(),
        z_bottom_nm: z_start_nm,
        net: resolved_net_name.clone(),
        area_nm2,
        bbox: Some(bbox),
        device_binding,
        merged_region_id: None, // Regular pours are not merged
        waivers: pour.waivers.clone(), // v0.1.7: Pass intent waivers to engine
    });

    // GAP 1.5 FIX: ANCHOR POINT GENERATION FOR ROUTER CONNECTION
    // Calculate center-of-mass of the pour and register as a virtual pin
    // This gives the router a target coordinate for connecting traces to pours
    let net_id = if let Some(net_name) = resolved_net_name.as_ref() {
        // Calculate center point of pour (center-of-mass)
        let center_x = (start_with_z.x + end_with_z.x) / 2;
        let center_y = (start_with_z.y + end_with_z.y) / 2;
        let center_z = (start_with_z.z + end_with_z.z) / 2;

        // Register pour as a virtual component in the netlist
        let pour_component_id = space.netlist.add_component(
            pour.name.to_string(),
            format!("Pour({})", pour.material).into(),
            (center_x, center_y, center_z),
        );

        // Add a virtual pin at the center point for routing
        let anchor_pin_id = space.netlist.add_pin(
            pour_component_id,
            "anchor".into(),
            (0, 0, 0), // Pin is at component position (center)
            None,      // Pours don't have physical pads
        );

        // Connect the anchor pin to the net
        // First, ensure the net exists in the netlist
        let net_id_handle =
            if let Some(existing_net) = space.netlist.get_net_by_name(net_name.as_str()) {
                existing_net
            } else {
                space.netlist.add_net(
                    net_name.clone(),
                    100_000, // Default 0.1mm trace width
                    material_id,
                )
            };

        // Connect the anchor pin to the net
        space.netlist.connect_pin(anchor_pin_id, net_id_handle);

        // v0.1.7: Logical Device Binding - Connect the bound pin to the net
        if let Some(binding) = &pour.device {
            if let Some(target_comp_id) = space.netlist.get_component_by_name(&binding.device_name) {
                if let Some(target_pin_id) = space.netlist.get_pin_by_name(target_comp_id, &binding.terminal) {
                    space.netlist.connect_pin(target_pin_id, net_id_handle);
                    println!(
                        "   ├─ Bound logical pin '{}.{}' to net '{}'",
                        binding.device_name, binding.terminal, net_name
                    );

                    // Also synchronize VoxelGrid metadata for the bound pin
                    // This ensures the component is exempt from collision during routing
                    space.voxel_grid.set_pin_net(&binding.device_name, &binding.terminal, net_name.as_str());
                }
            }
        }

        // v0.1.7: Register anchor pin in VoxelGrid for Global Router discovery
        // This ensures analyze_nets() can find the pour as a routing target
        space.voxel_grid.add_component_pin(
            center_x,
            center_y,
            center_z,
            pour.name.to_string().into(),
            "anchor".into(),
            Some(net_name.clone())
        );

        println!(
            "   ├─ Registered anchor point for pour '{}' at ({:.3}mm, {:.3}mm, {:.3}mm) on net '{}'",
            pour.name,
            center_x as f64 / 1_000_000.0,
            center_y as f64 / 1_000_000.0,
            center_z as f64 / 1_000_000.0,
            net_name
        );

        // Return the net ID for substrate layer creation
        net_id_handle.raw()
    } else {
        0 // Unassigned
    };

    // GAP 1 FIX: NATIVE SPARSE-AWARE ARCHITECTURE
    // Instead of forcing data into voxels (Density Bomb risk), keep everything sparse
    // and make the Router/DRC smart enough to see sparse layers.
    //
    // PHILOSOPHY: Don't move the data to the algorithm; make the algorithm see the data.
    //
    // ALL pours use sparse substrate layers (O(1) memory, instant placement)
    // Router collision detection will be updated to check sparse layers (Gap 1.5)

    // Reduced debug output - only print anomalies or errors
    // Full details available in substrate layer inspection if needed

    // v0.1.7 FIXED: Pours must be registered as SubstrateLayerType::Pour
    // to prevent the Auto-Drill system from carving holes through them.
    let bbox = hwc_engine::geometry::BoundingBox::new(start_with_z, end_with_z);
    space.voxel_grid.add_substrate_layer(
        material_id,
        net_id,
        bbox,
        hwc_engine::voxel_grid::SubstrateLayerType::Pour,
    );

    Ok(())
}
