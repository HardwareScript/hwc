//! Contact (via) placement functionality.
//!
//! This enables vertical connections between layers (e.g., metal-to-gate contacts).

use super::super::errors::IrError;
use super::super::stackup_manager::StackupManager;
use hwc_engine::{ComponentPlacer, HardwareSpace, Point3D};

/// Place a contact in the voxel grid.
pub fn place_contact(
    space: &mut HardwareSpace,
    contact: &hwc_parser::ContactPlacement,
    origin: hwc_parser::OriginPoint,
    symbol_table: &crate::SymbolTable,
    eval_context: &hwc_parser::EvaluationContext,
    stackup_manager: &StackupManager,
    profile: Option<&hwc_parser::ProfileDefinition>,
) -> Result<(), IrError> {
    // Get or register the contact material in the material registry
    let material_id = space.material_registry.get_or_register(&contact.material);

    // XY from coordinate; Z span from elevations via StackupManager
    // Extract x and y expressions from the coordinate
    let (_x_expr, _y_expr) = match &contact.position {
        hwc_parser::Coordinate::Positional { x, y, .. }
        | hwc_parser::Coordinate::Declarative { x, y, .. } => (x, y),
        hwc_parser::Coordinate::Relative(_) => {
            return Err(IrError::PlacementError(
                "Relative coordinates are not supported for contact placement".to_string(),
            ));
        }
    };

    // v0.1.7: Use canonical coordinate conversion to respect origin (Fixes XY separation)
    let ctx = crate::ir::conversions::CoordinateContext {
        voxel_size: &space.voxel_size,
        grid_size: &space.grid,
        origin,
        space_dimensions: &space.dimensions,
        symbol_table,
        eval_context,
        bbox_tracker: None,
        stackup_manager,
        profile,
    };
    let xy_point = crate::ir::conversions::coordinate_to_point(&contact.position, &ctx);

    // Calculate via diameter (use default if not specified)
    // v0.1.7: Support explicit drill_diameter vs legacy diameter
    let diameter_nm = if let Some(drill_dia) = &contact.drill_diameter {
        crate::ir::conversions::measurement_to_nm(drill_dia, symbol_table)
    } else if let Some(diameter_measurement) = &contact.diameter {
        // Use the canonical conversion method
        crate::ir::conversions::measurement_to_nm(diameter_measurement, symbol_table)
    } else {
        100_000 // Default 100um
    };
    let radius_nm = diameter_nm / 2;

    let from_bottom_nm = stackup_manager.resolve_elevation(&contact.from_elevation, symbol_table)?;
    let to_bottom_nm = stackup_manager.resolve_elevation(&contact.to_elevation, symbol_table)?;
    let from_top_nm = stackup_manager.resolve_elevation_top(
        &contact.from_elevation,
        symbol_table,
        space.voxel_size.z_nm,
    )?;
    let to_top_nm = stackup_manager.resolve_elevation_top(
        &contact.to_elevation,
        symbol_table,
        space.voxel_size.z_nm,
    )?;

    // v0.1.8 FIXED: Net-Aware Z-Span Calculation for Blind/Buried Vias
    // Vias should not cut through inner layers; they should stop at the nearest boundary.
    // 1. Top Layer: Span from Top Surface.
    // 2. Bottom Layer: Span from Bottom Surface.
    // 3. Inner Layer (Upper): Span from Bottom Surface (contact from below).
    // 4. Inner Layer (Lower): Span from Top Surface (contact from above).
    //
    // **MICRO-SINKING (v0.1.8)**: To prevent coplanar Z-fighting at the interface, 
    // we sink the via 500nm into the target copper layer. This ensures volumetric 
    // overlap, which will eventually be unified into a single manifold mesh 
    // via Strategy A (2D Co-Union).
    let (start_z, end_z) = if let (Some(from_name), Some(to_name)) = (
        stackup_manager.get_layer_name(&contact.from_elevation),
        stackup_manager.get_layer_name(&contact.to_elevation),
    ) {
        // Semantic mode: Use boundary rules
        let (lower_name, lower_bottom, lower_top, upper_name, upper_bottom, upper_top) = 
            if from_bottom_nm < to_bottom_nm {
                (from_name, from_bottom_nm, from_top_nm, to_name, to_bottom_nm, to_top_nm)
            } else {
                (to_name, to_bottom_nm, to_top_nm, from_name, from_bottom_nm, from_top_nm)
            };

        let via_bottom = if stackup_manager.is_bottom_layer(&lower_name) {
            lower_bottom // Bottom surface of board
        } else {
            lower_top - 500 // v0.1.8: Sink 500nm into target trace to prevent Z-fighting
        };

        let via_top = if stackup_manager.is_top_layer(&upper_name) {
            upper_top // Top surface of board
        } else {
            upper_bottom + 500 // v0.1.8: Sink 500nm into source trace to prevent Z-fighting
        };
        
        (via_bottom, via_top)
    } else {
        // Physical mode: Fallback to inclusive min/max
        (from_bottom_nm.min(to_bottom_nm), from_top_nm.max(to_top_nm))
    };

    // Create a cylindrical via by filling voxels within radius
    let placer = ComponentPlacer::new();

    // v0.1.7: The Unified Contact Placement
    // They connect pours at Z-boundaries (face-to-face contact).
    let start_point = Point3D::new(xy_point.x - radius_nm, xy_point.y - radius_nm, start_z);
    let end_point = Point3D::new(xy_point.x + radius_nm, xy_point.y + radius_nm, end_z);

    // v0.1.7 FIXED: The drill bbox must have XY area to intersect substrate layers.
    let contact_bbox = hwc_engine::geometry::BoundingBox::new(
        Point3D::new(xy_point.x - radius_nm, xy_point.y - radius_nm, start_z.min(end_z)),
        Point3D::new(xy_point.x + radius_nm, xy_point.y + radius_nm, start_z.max(end_z)),
    );

    // --- P43: MATERIAL INTEGRITY CHECK FOR CONTACTS ---
    //
    // ARCHITECTURAL PRINCIPLE: "Surface Contact Rule"
    // Contacts are vertical interconnects that span multiple Z-layers.
    // They connect pours at Z-boundaries (face-to-face contact).
    //
    // EXEMPTION: Contacts are exempt from horizontal interpenetration checks
    // because they are fundamentally vertical structures. The real physics
    // constraint is: "No two pours on the same layer can interpenetrate."
    //
    // Contact-vs-contact collisions are still checked (same material OK, different materials ERROR)

    // Check for contact-vs-contact collisions
    for existing_contact in &space.contacts {
        if let Some(existing_bbox) = &existing_contact.bbox {
            // If the boxes overlap in 3D space
            if contact_bbox.intersects(existing_bbox) {
                // If they have different materials, this is interpenetration
                if existing_contact.material_name != contact.material {
                    let contact_name: compact_str::CompactString = contact.name.as_ref().map(|n| n.to_string().into()).unwrap_or_else(|| {
                        format!("Via_{}_{}", from_bottom_nm, to_bottom_nm).into()
                    });

                    return Err(IrError::PlacementError(format!(
                        "Material interpenetration detected: Contact '{}' ({}) overlaps with contact '{}' ({}) in 3D space. \
                         Different materials cannot occupy the same volume.",
                        contact_name,
                        contact.material,
                        existing_contact.name,
                        existing_contact.material_name
                    )));
                }
            }
        }
    }
    // -----------------------------------------

    // Resolve net name to net ID for connectivity checking
    let net_id = if let Some(net_name) = &contact.net {
        // Get or create the net in the netlist
        if let Some(existing_net) = space.netlist.get_net_by_name(net_name.base.as_str()) {
            existing_net.raw()
        } else {
            // Create the net if it doesn't exist yet
            let new_net = space.netlist.add_net(
                net_name.to_string(),
                100_000, // Default 0.1mm trace width
                material_id,
            );
            new_net.raw()
        }
    } else {
        0 // Unassigned
    };

    let bridge_material_id = contact.bridge.as_ref().map(|b| space.material_registry.get_or_register(b));

    // v0.1.7: Register via for Excellon drill export
    // This ensures that ALL contacts (vias, THT pins, TSVs) are registered for the drill file.
    let board_max_z_nm = (space.grid.z_layers as i64) * space.voxel_size.z_nm;
    let via_net_id = hwc_engine::netlist::NetId::new(net_id);

    let via = hwc_engine::geometry_router::Via::new(
        (xy_point.x, xy_point.y),
        start_z,
        end_z,
        diameter_nm,
        via_net_id,
        0,              // min_z
        board_max_z_nm, // max_z
        space.voxel_size.z_nm,
    );
    space.add_vias(vec![via]);

    // v0.1.7: TSV (Through-Silicon Via) support
    if let Some(liner_material_name) = &contact.liner {
        let liner_material_id = space.material_registry.get_or_register(liner_material_name);
        let liner_thickness_nm = if let Some(lt) = &contact.liner_thickness {
            crate::ir::conversions::measurement_to_nm(lt, symbol_table)
        } else {
            5_000 // Default 5um liner
        };

        let bridge_thickness_nm = if contact.bridge.is_some() {
            1_000 // Default 1um bridge if specified
        } else {
            0
        };

        let koz_multiplier = if let Some(k) = &contact.koz {
            k.evaluate(eval_context).and_then(|v| v.as_number()).unwrap_or(3.0) as f32
        } else {
            3.0 // Default 3x diameter
        };

        let stack = hwc_engine::voxel_grid::LinerStack::new(
            liner_material_id,
            liner_thickness_nm,
            bridge_material_id,
            bridge_thickness_nm,
            material_id,
        );

        let params = hwc_engine::voxel_grid::TSVParams {
            diameter_nm,
            stack,
            koz_multiplier,
        };

        // Coordination: Drill, Stamp, and Register TSV
        space.voxel_grid.add_tsv_stack(
            xy_point.x,
            xy_point.y,
            start_z,
            end_z,
            params,
            hwc_engine::netlist::NetHandle::new(net_id),
        );
    } else if let Some(bridge_mat) = bridge_material_id {
        // Compound via (Phase 1/2)
        // Interface layer is the bottom layer
        let interface_end_z = start_z + space.voxel_size.z_nm;
        
        let interface_end_point = Point3D::new(end_point.x, end_point.y, interface_end_z);
        let fill_start_point = Point3D::new(start_point.x, start_point.y, interface_end_z);

        // Place bridge interface (e.g., Silicide) - use cylindrical placement
        placer
            .place_cylinder_substrate(
                &mut space.voxel_grid,
                bridge_mat,
                start_point,
                interface_end_point,
                net_id,
                diameter_nm,
            )
            .map_err(|e| {
                IrError::PlacementError(format!(
                    "Failed to place contact bridge '{}': {}",
                    contact.name.as_ref().map(|n| n.to_string()).unwrap_or_else(|| "<unnamed>".into()),
                    e
                ))
            })?;

        // Place via fill (e.g., Tungsten) - use cylindrical placement
        if interface_end_z < end_z {
            placer
                .place_cylinder_substrate(
                    &mut space.voxel_grid,
                    material_id,
                    fill_start_point,
                    end_point,
                    net_id,
                    diameter_nm,
                )
                .map_err(|e| {
                    IrError::PlacementError(format!(
                        "Failed to place contact fill '{}': {}",
                        contact.name.as_ref().map(|n| n.to_string()).unwrap_or_else(|| "<unnamed>".into()),
                        e
                    ))
                })?;
        }
    } else {
        // v0.1.7: Unified Manufacturing Process Logic
        // We now check the material's declared process DNA in the symbol table
        let mut process = hwc_engine::ManufacturingProcess::Deposited;
        if let Some(material_def) = symbol_table.materials().get(&contact.material) {
            process = match material_def.process {
                hwc_parser::ManufacturingProcess::DrilledPlated => hwc_engine::ManufacturingProcess::DrilledPlated,
                hwc_parser::ManufacturingProcess::Etched => hwc_engine::ManufacturingProcess::Etched,
                hwc_parser::ManufacturingProcess::Deposited => hwc_engine::ManufacturingProcess::Deposited,
            };
        }
        
        // Also ensure the engine's registry knows about this process for downstream checks
        space.material_registry.set_process(material_id, process);
        
        let clearance_nm = space.fabrication_constraints.as_ref()
            .map(|c| c.trace.min_spacing_nm)
            .unwrap_or(150_000); // Default 150um

        if process == hwc_engine::ManufacturingProcess::DrilledPlated {
            // 1. ACTION: Auto-Drill (v0.1.7)
            // We use drill_via_hole to ensure we carve the substrate and OTHER-NET pours
            // while maintaining connectivity to the target net pours.
            // v0.1.7: Added clearance_nm for different-net anti-pads.
            space.voxel_grid.drill_via_hole(contact_bbox, diameter_nm, net_id, clearance_nm);

            // 2. ACTION: Calculate Unified Via parameters (Annular Ring & Plating)
            let min_annular_ring_nm = if let Some(ring_measurement) = &contact.annular_ring {
                crate::ir::conversions::measurement_to_nm(ring_measurement, symbol_table)
            } else {
                space.fabrication_constraints.as_ref()
                    .map(|c| c.via.min_annular_ring_nm)
                    .unwrap_or(150_000) // Default 150um
            };

            let pad_diameter_nm = diameter_nm + (2 * min_annular_ring_nm);
            let plating_thickness_nm = if let Some(pt) = &contact.plating_thickness {
                crate::ir::conversions::measurement_to_nm(pt, symbol_table)
            } else {
                25_000 // Standard 1-mil plating
            };
            let inner_diameter_nm = diameter_nm - (2 * plating_thickness_nm);

            let bottom_diameter_nm = contact.bottom_diameter.as_ref()
                .map(|d| crate::ir::conversions::measurement_to_nm(d, symbol_table));

            // Determine Cap Types (v0.1.7 Unified Parametric Interconnect)
            // 1. Check explicit top/bottom caps (User/Stdlib Space)
            // 2. Fallback to legacy 'caps' boolean
            // 3. Default to Annular
            let top_cap = match contact.top_cap {
                Some(hwc_parser::CapType::None) => hwc_engine::voxel_grid::CapType::None,
                Some(hwc_parser::CapType::Annular) => hwc_engine::voxel_grid::CapType::Annular,
                Some(hwc_parser::CapType::Solid) => hwc_engine::voxel_grid::CapType::Solid,
                None => {
                    if contact.caps.unwrap_or(true) {
                        hwc_engine::voxel_grid::CapType::Annular
                    } else {
                        hwc_engine::voxel_grid::CapType::None
                    }
                }
            };

            let bottom_cap = match contact.bottom_cap {
                Some(hwc_parser::CapType::None) => hwc_engine::voxel_grid::CapType::None,
                Some(hwc_parser::CapType::Annular) => hwc_engine::voxel_grid::CapType::Annular,
                Some(hwc_parser::CapType::Solid) => hwc_engine::voxel_grid::CapType::Solid,
                None => {
                    if contact.caps.unwrap_or(true) {
                        hwc_engine::voxel_grid::CapType::Annular
                    } else {
                        hwc_engine::voxel_grid::CapType::None
                    }
                }
            };

            // 4. ACTION: Add ONE Unified Via Layer (Tube + Flanges)
            space.voxel_grid.add_tube_substrate_layer(
                material_id,
                net_id,
                contact_bbox,
                diameter_nm as u32,       // Outer Plating Dia
                inner_diameter_nm as u32, // Void Hole Dia
                pad_diameter_nm as u32,   // Flange/Pad Dia
                64,                       // segments
                top_cap,
                bottom_cap,
                bottom_diameter_nm.map(|d| d as u32),
            );
        } else if process == hwc_engine::ManufacturingProcess::Etched {
            // v0.1.7: Mechanical/Subtractive Logic
            // 1. ACTION: Auto-Drill (NPTH logic: Net 0 always drills everything)
            // v0.1.7: Added clearance_nm (though for NPTH it usually drills everything)
            space.voxel_grid.drill_via_hole(contact_bbox, diameter_nm, 0, clearance_nm);

            // NO cylinder/tube is added. The space remains empty (Void).
        } else {
            // v0.1.7: Auto-Drill for deposited vias
            // Even if not "drilled" in manufacturing, it must physically displace
            // the substrate and clear different-net pours.
            // v0.1.7: Added clearance_nm for different-net anti-pads.
            space.voxel_grid.drill_via_hole(contact_bbox, diameter_nm, net_id, clearance_nm);

            // Simple via - use cylindrical placement (deposited, not drilled)
            placer
                .place_cylinder_substrate(
                    &mut space.voxel_grid,
                    material_id,
                    start_point,
                    end_point,
                    net_id,
                    diameter_nm,
                )
                .map_err(|e| {
                    IrError::PlacementError(format!(
                        "Failed to place contact '{}': {}",
                        contact.name.as_ref().map(|n| n.to_string()).unwrap_or_else(|| "<unnamed>".into()),
                        e
                    ))
                })?;
        }
    }

    // If contact has a net, register it as a virtual component for netlist
    if let Some(net_name) = &contact.net {
        let contact_name: compact_str::CompactString = contact
            .name
            .as_ref()
            .map(|n| n.to_string().into())
            .unwrap_or_else(|| format!("Via_{}_{}", from_bottom_nm, to_bottom_nm).into());

        // Register contact as a virtual component
        let contact_component_id = space.netlist.add_component(
            contact_name.clone(),
            format!("Contact({})", contact.material).into(),
            (xy_point.x, xy_point.y, (start_z + end_z) / 2),
        );

        // Add a virtual pin at the contact center
        let contact_pin_id = space.netlist.add_pin(
            contact_component_id,
            "via".into(),
            (0, 0, 0), // Pin is at component position
            None,
        );

        // Connect to net
        let net_id =
            if let Some(existing_net) = space.netlist.get_net_by_name(net_name.base.as_str()) {
                existing_net
            } else {
                space.netlist.add_net(
                    net_name.to_string(),
                    diameter_nm, // Use via diameter as trace width
                    material_id,
                )
            };

        space.netlist.connect_pin(contact_pin_id, net_id);

        // Verbose logging removed - via arrays are summarized by auto_via_inserter
    }

    // Store contact metadata for connectivity checking
    let contact_name: compact_str::CompactString = contact
        .name
        .as_ref()
        .map(|n| n.to_string().into())
        .unwrap_or_else(|| format!("Via_{}_{}", from_bottom_nm, to_bottom_nm).into());

    // Task 4.2: Store via geometry as analytic primitive (bounding box only)
    // PRIMITIVES OVER PIXELS: No voxel collection needed - DRC uses bounding boxes directly
    space.contacts.push(hwc_engine::ContactMetadata {
        name: contact_name,
        material_name: contact.material.clone(),
        z_start_nm: from_bottom_nm,
        z_end_nm: to_bottom_nm,
        net: contact.net.as_ref().map(|n| n.to_string().into()),
        bridge: contact.bridge.clone(),
        bbox: Some(hwc_engine::geometry::BoundingBox {
            min: hwc_engine::geometry::Point3D::new(
                xy_point.x - radius_nm,
                xy_point.y - radius_nm,
                start_z,
            ),
            max: hwc_engine::geometry::Point3D::new(
                xy_point.x + radius_nm,
                xy_point.y + radius_nm,
                end_z,
            ),
        }),
        voxels: Vec::new(), // Empty - analytic geometry only
    });

    Ok(())
}
