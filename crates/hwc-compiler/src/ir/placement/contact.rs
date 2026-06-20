//! Contact (via) placement functionality.
//!
//! This enables vertical connections between layers (e.g., metal-to-gate contacts).

use super::super::errors::IrError;
use super::super::stackup_manager::StackupManager;
use compact_str::CompactString;
use hwc_engine::{ComponentPlacer, HardwareSpace, Point3D};

fn get_prop_nm(
    contact: &hwc_parser::ContactPlacement,
    name: &str,
    _symbol_table: &crate::SymbolTable,
    eval_context: &hwc_parser::EvaluationContext,
) -> Option<i64> {
    contact.properties.get(name).and_then(|expr| {
        expr.evaluate(eval_context)
            .ok()
            .and_then(|val| val.to_nanometers().ok())
    })
}

fn get_prop_bool(
    contact: &hwc_parser::ContactPlacement,
    name: &str,
    eval_context: &hwc_parser::EvaluationContext,
) -> Option<bool> {
    contact.properties.get(name).and_then(|expr| {
        if let hwc_parser::Expression::Variable { name, .. } = expr {
            match name.as_str() {
                "true" => return Some(true),
                "false" => return Some(false),
                _ => {}
            }
        }
        expr.evaluate(eval_context)
            .ok()
            .and_then(|val| val.as_integer().ok().map(|i| i != 0))
    })
}

fn get_prop_string(
    contact: &hwc_parser::ContactPlacement,
    name: &str,
    _eval_context: &hwc_parser::EvaluationContext,
) -> Option<CompactString> {
    contact.properties.get(name).and_then(|expr| match expr {
        hwc_parser::Expression::Variable { name, .. } => Some(name.clone()),
        _ => None,
    })
}

fn get_prop_cap_type(
    contact: &hwc_parser::ContactPlacement,
    name: &str,
    _eval_context: &hwc_parser::EvaluationContext,
) -> Option<hwc_engine::geometry_router::entity_graph::CapType> {
    contact.properties.get(name).and_then(|expr| match expr {
        hwc_parser::Expression::Variable { name, .. } => match name.as_str() {
            "none" => Some(hwc_engine::geometry_router::entity_graph::CapType::None),
            "annular" => Some(hwc_engine::geometry_router::entity_graph::CapType::Annular),
            "solid" => Some(hwc_engine::geometry_router::entity_graph::CapType::Solid),
            _ => None,
        },
        _ => None,
    })
}

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
    let diameter_nm = get_prop_nm(contact, "drill_diameter", symbol_table, eval_context)
        .or_else(|| get_prop_nm(contact, "diameter", symbol_table, eval_context))
        .unwrap_or(100_000);
    let radius_nm = diameter_nm / 2;

    let from_bottom_nm =
        stackup_manager.resolve_elevation(&contact.from_elevation, symbol_table)?;
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

    let contact_name_debug = contact
        .name
        .as_ref()
        .map(|n| n.to_string())
        .unwrap_or_else(|| "<unnamed>".into());
    println!("[PLACE_CONTACT] '{}' material='{}' dia={}nm from_z={}nm to_z={}nm from_top={}nm to_top={}nm",
        contact_name_debug, contact.material, diameter_nm,
        from_bottom_nm, to_bottom_nm, from_top_nm, to_top_nm);

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
        let (_lower_name, lower_bottom, _lower_top, _upper_name, _upper_bottom, upper_top) =
            if from_bottom_nm < to_bottom_nm {
                (
                    from_name,
                    from_bottom_nm,
                    from_top_nm,
                    to_name,
                    to_bottom_nm,
                    to_top_nm,
                )
            } else {
                (
                    to_name,
                    to_bottom_nm,
                    to_top_nm,
                    from_name,
                    from_bottom_nm,
                    from_top_nm,
                )
            };

        // v0.1.9: FULL-THICKNESS PENETRATION
        // Strategy A (2D Co-Union) handles Z-fighting by merging meshes, so we no longer
        // need the 500nm "Micro-Sinking" workaround. Vias now span the entire thickness
        // of their start and end layers to ensure physical continuity for stacked vias.
        let via_bottom = lower_bottom;
        let via_top = upper_top;

        (via_bottom, via_top)
    } else {
        // Physical mode: Fallback to inclusive min/max
        (from_bottom_nm.min(to_bottom_nm), from_top_nm.max(to_top_nm))
    };

    println!(
        "[PLACE_CONTACT] '{}' final span: start_z={}nm end_z={}nm ({}nm tall)",
        contact_name_debug,
        start_z,
        end_z,
        end_z - start_z
    );

    // Create a cylindrical via by filling voxels within radius
    let placer = ComponentPlacer::new();

    // v0.1.7: The Unified Contact Placement
    // They connect pours at Z-boundaries (face-to-face contact).
    let start_point = Point3D::new(xy_point.x - radius_nm, xy_point.y - radius_nm, start_z);
    let end_point = Point3D::new(xy_point.x + radius_nm, xy_point.y + radius_nm, end_z);

    // v0.1.7 FIXED: The drill bbox must have XY area to intersect substrate layers.
    let contact_bbox = hwc_engine::geometry::BoundingBox::new(
        Point3D::new(
            xy_point.x - radius_nm,
            xy_point.y - radius_nm,
            start_z.min(end_z),
        ),
        Point3D::new(
            xy_point.x + radius_nm,
            xy_point.y + radius_nm,
            start_z.max(end_z),
        ),
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
                    let contact_name: compact_str::CompactString = contact
                        .name
                        .as_ref()
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| {
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

    let bridge_material_name = get_prop_string(contact, "bridge", eval_context);
    let bridge_material_id = bridge_material_name
        .as_ref()
        .map(|b| space.material_registry.get_or_register(b));

    // v0.2.0: Resolve shape from properties if not already set as a contour
    let mut contour = contact.contour.clone();
    if contour.is_none() {
        if let Some(shape_name) = get_prop_string(contact, "shape", eval_context) {
            if let Some(shape_def) = symbol_table.get_shape(shape_name.as_str()) {
                let constants = symbol_table.get_all_constants();
                contour = Some(crate::auto_via_inserter::library::evaluate_shape_points(
                    shape_def,
                    diameter_nm,
                    &constants,
                ));
                let contour_len = contour.as_ref().map_or(0, |c| c.len());
                println!(
                    "[PLACE_CONTACT] Resolved shape '{}' to {} vertices",
                    shape_name, contour_len
                );
            }
        }
    }

    // Compute pad bbox (drill + annular ring) for contact metadata and substrate layers
    let annular_ring_nm =
        if let Some(nm) = get_prop_nm(contact, "annular_ring", symbol_table, eval_context) {
            nm
        } else {
            space
                .fabrication_constraints
                .as_ref()
                .map(|c| c.via.min_annular_ring_nm)
                .unwrap_or(150_000)
        };

    // v0.1.7: Register via for Excellon drill export
    // This ensures that ALL contacts (vias, THT pins, TSVs) are registered for the drill file.
    let board_max_z_nm = (space.grid_cells().z_layers as i64) * space.voxel_size.z_nm;
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
        annular_ring_nm,
    );
    space.add_vias(vec![via]);

    // v0.1.7: Read solder mask properties (needed by all process branches and ContactMetadata)
    let is_tented = get_prop_bool(contact, "is_tented", eval_context).unwrap_or(false);
    let pad_diameter_nm = diameter_nm + (2 * annular_ring_nm);
    let pad_radius_nm = pad_diameter_nm / 2;
    let pad_bbox = hwc_engine::geometry::BoundingBox::new(
        Point3D::new(
            xy_point.x - pad_radius_nm,
            xy_point.y - pad_radius_nm,
            start_z.min(end_z),
        ),
        Point3D::new(
            xy_point.x + pad_radius_nm,
            xy_point.y + pad_radius_nm,
            start_z.max(end_z),
        ),
    );

    // v0.1.7: TSV (Through-Silicon Via) support
    let liner_material_name = get_prop_string(contact, "liner", eval_context);
    if let Some(liner_material_name) = &liner_material_name {
        let liner_material_id = space.material_registry.get_or_register(liner_material_name);
        let liner_thickness_nm =
            get_prop_nm(contact, "liner_thickness", symbol_table, eval_context).unwrap_or(5_000);

        let bridge_thickness_nm = if bridge_material_name.is_some() {
            1_000 // Default 1um bridge if specified
        } else {
            0
        };

        let koz_multiplier = if let Some(expr) = contact.properties.get("koz") {
            expr.evaluate(eval_context)
                .and_then(|v| v.as_number())
                .unwrap_or(3.0) as f32
        } else {
            3.0 // Default 3x diameter
        };

        let stack = hwc_engine::geometry_router::entity_graph::LinerStack::new(
            liner_material_id,
            liner_thickness_nm,
            bridge_material_id,
            bridge_thickness_nm,
            material_id,
        );

        let params = hwc_engine::geometry_router::entity_graph::TSVParams {
            diameter_nm,
            stack,
            koz_multiplier,
        };

        // Coordination: Drill, Stamp, and Register TSV
        space.entity_graph.add_tsv_stack(
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

        // v0.2.0: Use polygon contour if available, fallback to cylinder
        if let Some(ref contour) = contour {
            placer
                .place_polygon_substrate(
                    &mut space.entity_graph,
                    bridge_mat,
                    start_point,
                    interface_end_point,
                    net_id,
                    contour,
                )
                .map_err(|e| {
                    IrError::PlacementError(format!(
                        "Failed to place contact bridge '{}': {}",
                        contact
                            .name
                            .as_ref()
                            .map(|n| n.to_string())
                            .unwrap_or_else(|| "<unnamed>".into()),
                        e
                    ))
                })?;
        } else {
            placer
                .place_cylinder_substrate(
                    &mut space.entity_graph,
                    bridge_mat,
                    start_point,
                    interface_end_point,
                    net_id,
                    diameter_nm,
                )
                .map_err(|e| {
                    IrError::PlacementError(format!(
                        "Failed to place contact bridge '{}': {}",
                        contact
                            .name
                            .as_ref()
                            .map(|n| n.to_string())
                            .unwrap_or_else(|| "<unnamed>".into()),
                        e
                    ))
                })?;
        }

        // Place via fill (e.g., Tungsten) - use contour-aware placement
        if interface_end_z < end_z {
            if let Some(ref contour) = contour {
                placer
                    .place_polygon_substrate(
                        &mut space.entity_graph,
                        material_id,
                        fill_start_point,
                        end_point,
                        net_id,
                        contour,
                    )
                    .map_err(|e| {
                        IrError::PlacementError(format!(
                            "Failed to place contact fill '{}': {}",
                            contact
                                .name
                                .as_ref()
                                .map(|n| n.to_string())
                                .unwrap_or_else(|| "<unnamed>".into()),
                            e
                        ))
                    })?;
            } else {
                placer
                    .place_cylinder_substrate(
                        &mut space.entity_graph,
                        material_id,
                        fill_start_point,
                        end_point,
                        net_id,
                        diameter_nm,
                    )
                    .map_err(|e| {
                        IrError::PlacementError(format!(
                            "Failed to place contact fill '{}': {}",
                            contact
                                .name
                                .as_ref()
                                .map(|n| n.to_string())
                                .unwrap_or_else(|| "<unnamed>".into()),
                            e
                        ))
                    })?;
            }
        }
    } else {
        // v0.1.7: Unified Manufacturing Process Logic
        // We now check the material's declared process DNA in the symbol table
        let mut process = hwc_engine::ManufacturingProcess::Deposited;
        if let Some(material_def) = symbol_table.materials().get(&contact.material) {
            process = match material_def.process {
                hwc_parser::ManufacturingProcess::DrilledPlated => {
                    hwc_engine::ManufacturingProcess::DrilledPlated
                }
                hwc_parser::ManufacturingProcess::Etched => {
                    hwc_engine::ManufacturingProcess::Etched
                }
                hwc_parser::ManufacturingProcess::Deposited => {
                    hwc_engine::ManufacturingProcess::Deposited
                }
            };
        }

        // Also ensure the engine's registry knows about this process for downstream checks
        space.material_registry.set_process(material_id, process);

        let clearance_nm = space
            .fabrication_constraints
            .as_ref()
            .map(|c| c.trace.min_spacing_nm)
            .unwrap_or(150_000); // Default 150um

        println!(
            "[PLACE_CONTACT] '{}' process={:?}, net_id={}, material_id={}",
            contact_name_debug, process, net_id, material_id
        );

        if process == hwc_engine::ManufacturingProcess::DrilledPlated {
            // 1. pad_bbox already computed above

            // v0.1.7: Profile-driven solder mask expansion (Zero Implicit Magic)
            let solder_mask_expansion_nm = space
                .fabrication_constraints
                .as_ref()
                .map(|c| c.solder_mask_expansion_nm)
                .unwrap_or(75_000);

            // 2. ACTION: Auto-Drill (v0.1.7)
            // We use drill_via_hole to ensure we carve the substrate, OTHER-NET pours,
            // and solder mask layers while maintaining connectivity to target net pours.
            space.entity_graph.drill_via_hole(
                contact_bbox,
                diameter_nm,
                net_id,
                clearance_nm,
                is_tented,
                pad_diameter_nm,
                solder_mask_expansion_nm,
            );

            // 3. ACTION: Calculate remaining Unified Via parameters (Plating)
            let plating_thickness_nm =
                get_prop_nm(contact, "plating_thickness", symbol_table, eval_context)
                    .unwrap_or(25_000);
            let inner_diameter_nm = diameter_nm - (2 * plating_thickness_nm);

            let bottom_diameter_nm =
                get_prop_nm(contact, "bottom_diameter", symbol_table, eval_context);

            // Determine Cap Types (v0.1.7 Unified Parametric Interconnect)
            // 1. Check explicit top/bottom caps (User/Stdlib Space)
            // 2. Fallback to legacy 'caps' boolean
            // 3. Default to Annular
            let top_cap = match get_prop_cap_type(contact, "top_cap", eval_context) {
                Some(cap) => cap,
                None => {
                    if get_prop_bool(contact, "caps", eval_context).unwrap_or(true) {
                        hwc_engine::geometry_router::entity_graph::CapType::Annular
                    } else {
                        hwc_engine::geometry_router::entity_graph::CapType::None
                    }
                }
            };

            let bottom_cap = match get_prop_cap_type(contact, "bottom_cap", eval_context) {
                Some(cap) => cap,
                None => {
                    if get_prop_bool(contact, "caps", eval_context).unwrap_or(true) {
                        hwc_engine::geometry_router::entity_graph::CapType::Annular
                    } else {
                        hwc_engine::geometry_router::entity_graph::CapType::None
                    }
                }
            };

            // 4. ACTION: Add ONE Unified Via Layer (Tube + Flanges)
            println!("[PLACE_CONTACT] '{}' Adding tube substrate: pad_bbox=({},{}-{},{}), outer_dia={}, inner_dia={}, pad_dia={}, top_cap={:?}, bottom_cap={:?}",
                contact_name_debug,
                contact_bbox.min.x, contact_bbox.min.y, contact_bbox.max.x, contact_bbox.max.y,
                diameter_nm, inner_diameter_nm, pad_diameter_nm, top_cap, bottom_cap);
            space.entity_graph.add_tube_substrate_layer(
                material_id,
                net_id,
                pad_bbox,
                diameter_nm as u32,       // Outer Plating Dia
                inner_diameter_nm as u32, // Void Hole Dia
                pad_diameter_nm as u32,   // Flange/Pad Dia
                64,                       // segments
                top_cap,
                bottom_cap,
                bottom_diameter_nm.map(|d| d as u32),
            );

            // 5. ACTION: Handle Filled Vias (v0.1.9: VIPPO)
            if get_prop_bool(contact, "filled", eval_context).unwrap_or(false) {
                let fill_material_name = get_prop_string(contact, "fill_material", eval_context);
                let fill_material_id = if let Some(fill_mat_name) = &fill_material_name {
                    space.material_registry.get_or_register(fill_mat_name)
                } else {
                    // Default to non-conductive epoxy if not specified
                    space.material_registry.get_or_register("Epoxy")
                };

                let fill_net_id = if let Some(fill_mat_name) = &fill_material_name {
                    if let Some(mat_def) = symbol_table.materials().get(fill_mat_name) {
                        if mat_def.category.is_conductive() {
                            net_id
                        } else {
                            0
                        }
                    } else {
                        0
                    }
                } else {
                    0
                };

                // Add a solid cylinder of fill material inside the tube
                // The fill diameter matches the inner diameter of the tube.
                space.entity_graph.add_cylinder_substrate_layer(
                    fill_material_id,
                    fill_net_id,
                    contact_bbox,
                    inner_diameter_nm,
                    16, // Simpler segments for fill
                    0,  // No extra KOZ for fill
                );
            }
        } else if process == hwc_engine::ManufacturingProcess::Etched {
            // v0.1.7: Mechanical/Subtractive Logic
            // 1. ACTION: Auto-Drill (NPTH logic: Net 0 always drills everything)
            // Etched vias are NPTH - no solder mask opening needed (is_tented=true for NPTH)
            space.entity_graph.drill_via_hole(
                contact_bbox,
                diameter_nm,
                0,
                clearance_nm,
                true,
                diameter_nm,
                75_000,
            );

            // NO cylinder/tube is added. The space remains empty (Void).
        } else {
            // v0.1.7: Auto-Drill for deposited vias
            // Even if not "drilled" in manufacturing, it must physically displace
            // the substrate and clear different-net pours.
            // Deposited vias use drill_diameter as pad (no annular ring by default)
            let solder_mask_expansion_nm = space
                .fabrication_constraints
                .as_ref()
                .map(|c| c.solder_mask_expansion_nm)
                .unwrap_or(75_000);
            println!("[PLACE_CONTACT] '{}' Deposited path: drilling via hole at bbox=({},{}-{},{}) dia={}",
                contact_name_debug,
                contact_bbox.min.x, contact_bbox.min.y, contact_bbox.max.x, contact_bbox.max.y,
                diameter_nm);
            space.entity_graph.drill_via_hole(
                contact_bbox,
                diameter_nm,
                net_id,
                clearance_nm,
                is_tented,
                diameter_nm,
                solder_mask_expansion_nm,
            );

            // Simple via - use contour-aware placement (deposited, not drilled)
            // v0.2.0: Use polygon contour if available, fallback to cylinder
            if let Some(ref contour) = contour {
                println!("[PLACE_CONTACT] '{}' Placing polygon via: mat={}, net={}, start=({},{},{}) end=({},{},{}) dia={}",
                    contact_name_debug, contact.material, net_id,
                    start_point.x, start_point.y, start_point.z,
                    end_point.x, end_point.y, end_point.z, diameter_nm);
                placer
                    .place_polygon_substrate(
                        &mut space.entity_graph,
                        material_id,
                        start_point,
                        end_point,
                        net_id,
                        contour,
                    )
                    .map_err(|e| {
                        IrError::PlacementError(format!(
                            "Failed to place contact '{}': {}",
                            contact
                                .name
                                .as_ref()
                                .map(|n| n.to_string())
                                .unwrap_or_else(|| "<unnamed>".into()),
                            e
                        ))
                    })?;
            } else {
                println!("[PLACE_CONTACT] '{}' Placing cylinder via: mat={}, net={}, start=({},{},{}) end=({},{},{}) dia={}",
                    contact_name_debug, contact.material, net_id,
                    start_point.x, start_point.y, start_point.z,
                    end_point.x, end_point.y, end_point.z, diameter_nm);
                placer
                    .place_cylinder_substrate(
                        &mut space.entity_graph,
                        material_id,
                        start_point,
                        end_point,
                        net_id,
                        diameter_nm,
                    )
                    .map_err(|e| {
                        IrError::PlacementError(format!(
                            "Failed to place contact '{}': {}",
                            contact
                                .name
                                .as_ref()
                                .map(|n| n.to_string())
                                .unwrap_or_else(|| "<unnamed>".into()),
                            e
                        ))
                    })?;
            }
        }
    }

    // If contact has a net, register it as a virtual component for netlist
    if let Some(net_name) = &contact.net {
        let contact_name: compact_str::CompactString = contact
            .name
            .as_ref()
            .map(|n| n.to_string())
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
        .map(|n| n.to_string())
        .unwrap_or_else(|| format!("Via_{}_{}", from_bottom_nm, to_bottom_nm).into());

    // Task 4.2: Store via geometry as analytic primitive (bounding box only)
    // PRIMITIVES OVER PIXELS: No voxel collection needed - DRC uses bounding boxes directly
    println!(
        "[PLACE_CONTACT] '{}' Storing contact metadata: bbox=({},{}-{},{}), z={}→{}nm, net={:?}",
        contact_name_debug,
        pad_bbox.min.x,
        pad_bbox.min.y,
        pad_bbox.max.x,
        pad_bbox.max.y,
        from_bottom_nm,
        to_bottom_nm,
        contact.net
    );
    space.contacts.push(hwc_engine::ContactMetadata {
        name: contact_name,
        material_name: contact.material.clone(),
        z_start_nm: from_bottom_nm,
        z_end_nm: to_bottom_nm,
        net: contact.net.as_ref().map(|n| n.to_string()),
        bridge: bridge_material_name,
        bbox: Some(pad_bbox),
        voxels: Vec::new(), // Empty - analytic geometry only
        is_tented,
        mask_clearance_diameter_nm: get_prop_nm(
            contact,
            "mask_clearance_diameter",
            symbol_table,
            eval_context,
        ),
    });

    Ok(())
}
