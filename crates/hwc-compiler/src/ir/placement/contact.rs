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
    _origin: hwc_parser::OriginPoint,
    symbol_table: &crate::SymbolTable,
    eval_context: &hwc_parser::EvaluationContext,
    stackup_manager: &StackupManager,
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
        origin: _origin,
        space_dimensions: &space.dimensions,
        symbol_table,
        eval_context,
        bbox_tracker: None,
        stackup_manager,
    };
    let xy_point = crate::ir::conversions::coordinate_to_point(&contact.position, &ctx);

    // Calculate via diameter (use default if not specified)
    // Default via diameter: 100um (0.1mm) = 100,000nm
    let diameter_nm = if let Some(diameter_measurement) = &contact.diameter {
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

    let start_z = from_bottom_nm.min(to_bottom_nm);
    let end_z = from_top_nm.max(to_top_nm);

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
        // We now check the material's declared process DNA instead of guessing based on category.
        let process = space.material_registry.get_process(material_id)
            .unwrap_or(hwc_engine::ManufacturingProcess::Deposited);
        
        if process == hwc_engine::ManufacturingProcess::DrilledPlated {
            // 1. ACTION: Drill hole through all substrate layers (carves Pour/FR4)
            space.voxel_grid.drill_hole(contact_bbox, Some(diameter_nm));

            // 2. ACTION: Register as a manufacturing drill (for .drl and mesh cutout)
            let via = hwc_engine::geometry_router::Via::new(
                (xy_point.x, xy_point.y),
                start_z,
                end_z,
                diameter_nm,
                hwc_engine::netlist::NetId::new(net_id),
                0,                          // board_min_z_nm
                space.dimensions.depth_nm,  // board_max_z_nm
                space.voxel_size.z_nm,
            );
            space.vias.push(via);

            // 3. ACTION: Calculate Unified Via parameters (Annular Ring & Plating)
            let min_annular_ring_nm = if let Some(ring_measurement) = &contact.annular_ring {
                crate::ir::conversions::measurement_to_nm(ring_measurement, symbol_table)
            } else {
                space.fabrication_constraints.as_ref()
                    .map(|c| c.via.min_annular_ring_nm)
                    .unwrap_or(150_000) // Default 150um
            };

            let pad_diameter_nm = diameter_nm + (2 * min_annular_ring_nm);
            let plating_thickness_nm = 25_000; // Standard 1-mil plating
            let inner_diameter_nm = diameter_nm - (2 * plating_thickness_nm);

            // 4. ACTION: Add ONE Unified Via Layer (Tube + Flanges)
            space.voxel_grid.add_tube_substrate_layer(
                material_id,
                net_id,
                contact_bbox,
                diameter_nm as u32,       // Outer Plating Dia
                inner_diameter_nm as u32, // Void Hole Dia
                pad_diameter_nm as u32,   // Flange/Pad Dia
                64,                       // segments
                contact.caps.unwrap_or(true)
            );
        } else if process == hwc_engine::ManufacturingProcess::Etched {
            // v0.1.7: Mechanical/Subtractive Logic
            // 1. ACTION: Drill hole through all substrate layers
            space.voxel_grid.drill_hole(contact_bbox, Some(diameter_nm));

            // 2. ACTION: Register as a Non-Plated Through Hole (NPTH) for the drill file
            let via = hwc_engine::geometry_router::Via::new(
                (xy_point.x, xy_point.y),
                start_z,
                end_z,
                diameter_nm,
                hwc_engine::netlist::NetId::new(0), // No Net = NPTH
                0,
                space.dimensions.depth_nm,
                space.voxel_size.z_nm,
            );
            space.vias.push(via);
            
            // NO cylinder/tube is added. The space remains empty (Void).
        } else {
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
