use crate::commands::build_cmd::BuildConfig;
use hwc_compiler::{alignment::PhysicalNetlist, SymbolTable};
use hwc_engine::HardwareSpace;
use hwc_parser::Program;
use miette::Result;
use std::time::Instant;

/// Run alignment validation (Artist vs Professional mode)
/// Returns Some(PhysicalNetlist) in Professional mode, None in Artist mode
pub fn validate_alignment(
    ast: &Program,
    space: &mut HardwareSpace,
    symbol_table: &SymbolTable,
    config: &BuildConfig,
    start_time: Instant,
) -> Result<Option<PhysicalNetlist>> {
    // Extract space definition from AST
    let space_def = ast
        .definitions
        .iter()
        .find_map(|def| {
            if let hwc_parser::Definition::Space(space) = def {
                Some(space)
            } else {
                None
            }
        })
        .ok_or_else(|| miette::miette!("No space definition found in AST"))?;

    // Check for Artist Mode vs Professional Mode
    let is_artist_mode = space_def.implements_module.is_none();

    if is_artist_mode {
        println!("🎨 Artist Mode: No 'implements' clause - Alignment validation skipped");
        println!("   ℹ️  Building geometry without logic verification");
        println!(
            "[{:>8.2}ms] Artist Mode check complete",
            start_time.elapsed().as_secs_f64() * 1000.0
        );
        Ok(None)
    } else {
        println!("🔍 Professional Mode: Alignment validation enabled");
        // println!($3"[DEBUG] Starting device extraction at {:.2}ms",
        //      start_time.elapsed().as_secs_f64() * 1000.0
        //   );

        // **HANDSHAKE C: GEOMETRIC REALIZATION (Sprint 3.12 - Gap 3 Fix)**
        //
        // Realize analytic routes into voxel grid for geometric analysis.
        // This enables:
        // - Device extraction (silicon: copper-silicon contact detection)
        // - Parasitic extraction (both PCB and silicon)
        // - Physical verification (DRC, Alignment Layer)
        //
        // **Performance:** Called once at end (lazy realization pattern)
        // - 3 routes: ~0.01s (vs 13.44s if done during routing)
        // - Bulk operation with sparse chunk allocation
        // - Universal: works for both PCB and silicon designs
        if !space.analytic_routes.is_empty() {
            // println!($3"[DEBUG] Realizing {} analytic routes into voxel grid for geometric analysis...",
            //    space.analytic_routes.len()
            // );
            let realize_start = std::time::Instant::now();
            space.realize_analytic_routes();
            let _realize_duration = realize_start.elapsed();
            // println!($3"[DEBUG] Geometric realization complete in {:.6}s",
            //    realize_duration.as_secs_f64()
            //  );
        }

        // Extract physical netlist from geometry
        use compact_str::CompactString;
        use hwc_export::device_extractor::DeviceExtractor;
        let mut device_extractor = DeviceExtractor::new(space, symbol_table);

        // Extract module definition for intent-based device extraction
        let module_def = ast.definitions.iter().find_map(|def| {
            if let hwc_parser::Definition::Module(module) = def {
                Some(module)
            } else {
                None
            }
        });

        // println!($3"[DEBUG] About to call extract_devices_with_module at {:.2}ms",
        //    start_time.elapsed().as_secs_f64() * 1000.0
        //  );
        let extracted_netlist = device_extractor
            .extract_devices_with_module(module_def)
            .map_err(|errors| {
                let error_messages: Vec<CompactString> =
                    errors.iter().map(|e| e.to_string().into()).collect();
                miette::miette!("Device extraction failed:\n{}", error_messages.join("\n"))
            })?;
        // println!($3"[DEBUG] Device extraction complete at {:.2}ms",
        //     start_time.elapsed().as_secs_f64() * 1000.0
        //  );

        // Run alignment validation
        // println!($3"[DEBUG] Running alignment validation...");
        let alignment_result = hwc_compiler::AlignmentValidator::validate(
            space_def,
            &extracted_netlist,
            symbol_table,
            space,
            config.tolerance,
        )
        .map_err(|e| miette::miette!("Alignment validation error: {}", e))?;
        // println!($3"[DEBUG] Alignment validation complete at {:.2}ms",
        //      start_time.elapsed().as_secs_f64() * 1000.0
        //   );

        match &alignment_result {
            hwc_compiler::AlignmentResult::Skipped { reason } => {
                println!("   ⚠️  Unexpected: {}", reason);
            }
            hwc_compiler::AlignmentResult::Passed {
                physical_device_count,
                logical_device_count,
            } => {
                println!(
                    "   ✅ Physical netlist extracted: {} devices",
                    physical_device_count
                );
                println!(
                    "   ✅ Logical netlist synthesized: {} devices",
                    logical_device_count
                );
                println!("   ✅ Alignment validation passed: Layout matches schematic");
            }
            hwc_compiler::AlignmentResult::Failed { error } => {
                println!("   ❌ Alignment validation failed\n");
                eprintln!("❌ ALIGNMENT ERROR: {}", error);
                eprintln!("\nBuild failed. No exports generated.");
                eprintln!("Fix the alignment errors above and try again.");
                return Err(miette::miette!("Alignment validation failed"));
            }
        }

        // Sprint 4.1.1: Run Physical Continuity Check (Layer 2 of Triple-Check Architecture)
        // Physical continuity validates actual copper paths before parameter extraction
        if !config.skip_physical_continuity {
            println!("\n🔍 Running Physical Continuity Validation...");
            let continuity_start = std::time::Instant::now();

            // Convert metadata to physics format (reuse existing conversion logic)
            let physics_pours: Vec<hwc_physics::connectivity::PourMetadata> = space
                .pours
                .iter()
                .map(|pour| hwc_physics::connectivity::PourMetadata {
                    name: pour.name.clone(),
                    material_name: pour.material_name.clone(),
                    net: pour.net.clone(),
                    area_nm2: pour.area_nm2,
                    bbox: pour
                        .bbox
                        .as_ref()
                        .map(|bbox| hwc_physics::connectivity::BoundingBox {
                            min_x: bbox.min.x,
                            min_y: bbox.min.y,
                            min_z: bbox.min.z,
                            max_x: bbox.max.x,
                            max_y: bbox.max.y,
                            max_z: bbox.max.z,
                        }),
                })
                .collect();

            let physics_contacts: Vec<hwc_physics::connectivity::ContactMetadata> = space
                .contacts
                .iter()
                .map(|contact| hwc_physics::connectivity::ContactMetadata {
                    name: contact.name.clone(),
                    material_name: contact.material_name.clone(),
                    net: contact.net.clone(),
                    bbox: contact.bbox.as_ref().map(|bbox| {
                        hwc_physics::connectivity::BoundingBox {
                            min_x: bbox.min.x,
                            min_y: bbox.min.y,
                            min_z: bbox.min.z,
                            max_x: bbox.max.x,
                            max_y: bbox.max.y,
                            max_z: bbox.max.z,
                        }
                    }),
                })
                .collect();

            let physics_substrate_layers: Vec<hwc_physics::connectivity::SubstrateLayerMetadata> =
                space
                    .voxel_grid
                    .get_substrate_layers()
                    .iter()
                    .map(|layer| {
                        let net_name = if layer.net != 0 {
                            space
                                .netlist
                                .get_net(hwc_engine::netlist::NetId::new(layer.net))
                                .map(|net_data| net_data.name.clone())
                        } else {
                            None
                        };
                        let shape = match layer.shape {
                            hwc_engine::voxel_grid::SubstrateLayerShape::Rect => {
                                hwc_physics::connectivity::SubstrateLayerShapeMetadata::Rect
                            }
                            hwc_engine::voxel_grid::SubstrateLayerShape::Polygon { ref outer_contour, .. } => {
                                hwc_physics::connectivity::SubstrateLayerShapeMetadata::Polygon {
                                    outer_contour: outer_contour.iter().map(|p| (p.x, p.y)).collect(),
                                }
                            }
                            hwc_engine::voxel_grid::SubstrateLayerShape::Tube {
                                outer_diameter,
                                inner_diameter,
                                ..
                            } => hwc_physics::connectivity::SubstrateLayerShapeMetadata::Tube {
                                outer_diameter,
                                inner_diameter,
                            },
                            hwc_engine::voxel_grid::SubstrateLayerShape::Circle { .. } => {
                                hwc_physics::connectivity::SubstrateLayerShapeMetadata::Rect
                            },
                        };

                        let cutouts = layer
                            .cutouts
                            .iter()
                            .map(|c| hwc_physics::connectivity::CutoutMetadata {
                                bbox: hwc_physics::connectivity::BoundingBox {
                                    min_x: c.bbox.min.x,
                                    min_y: c.bbox.min.y,
                                    min_z: c.bbox.min.z,
                                    max_x: c.bbox.max.x,
                                    max_y: c.bbox.max.y,
                                    max_z: c.bbox.max.z,
                                },
                                shape: match c.shape {
                                    hwc_engine::voxel_grid::SubstrateLayerShape::Rect => {
                                        hwc_physics::connectivity::SubstrateLayerShapeMetadata::Rect
                                    }
                                    hwc_engine::voxel_grid::SubstrateLayerShape::Polygon { ref outer_contour, .. } => {
                                        hwc_physics::connectivity::SubstrateLayerShapeMetadata::Polygon {
                                            outer_contour: outer_contour.iter().map(|p| (p.x, p.y)).collect(),
                                        }
                                    }
                                    hwc_engine::voxel_grid::SubstrateLayerShape::Tube {
                                        outer_diameter,
                                        inner_diameter,
                                        ..
                                    } => hwc_physics::connectivity::SubstrateLayerShapeMetadata::Tube {
                                        outer_diameter,
                                        inner_diameter,
                                    },
                                    hwc_engine::voxel_grid::SubstrateLayerShape::Circle { .. } => {
                                        hwc_physics::connectivity::SubstrateLayerShapeMetadata::Rect
                                    }
                                },
                            })
                            .collect();

                        hwc_physics::connectivity::SubstrateLayerMetadata {
                            material: layer.material,
                            net: layer.net,
                            net_name,
                            bbox: hwc_physics::connectivity::BoundingBox {
                                min_x: layer.bbox.min.x,
                                min_y: layer.bbox.min.y,
                                min_z: layer.bbox.min.z,
                                max_x: layer.bbox.max.x,
                                max_y: layer.bbox.max.y,
                                max_z: layer.bbox.max.z,
                            },
                            shape,
                            cutouts,
                        }
                    })
                    .collect();

            // Get bridge rules from profile (v0.1.7)
            let bridge_rules: Vec<hwc_physics::BridgeRule> = if let Some(ref constraints) =
                space.fabrication_constraints
            {
                constraints
                    .bridges
                    .iter()
                    .map(|b| hwc_physics::BridgeRule {
                        from_material: b.from_material.clone(),
                        to_material: b.to_material.clone(),
                        interface_material: b.interface_material.clone(),
                        fill_material: b.fill_material.clone(),
                    })
                    .collect()
            } else {
                Vec::new()
            };

            // Get material name -> ID mapping for physics (v0.1.7)
            let mut material_mapping = rustc_hash::FxHashMap::default();
            for (id, name) in space.material_registry.all_materials() {
                material_mapping.insert(compact_str::CompactString::from(name), id);
            }

            let physical_continuity_checker = hwc_physics::PhysicalContinuityChecker::new(
                space.voxel_size.z_nm,
                &physics_pours,
                &physics_contacts,
                &physics_substrate_layers,
                &bridge_rules,
                material_mapping,
            );

            // Extract pin positions
            let pin_positions: Vec<hwc_physics::PinPosition> =
                {
                    let component_pins = space.voxel_grid.get_component_pins();
                    component_pins
                        .iter()
                        .map(|pin| {
                            let component_id =
                                pin.component_name.as_bytes().iter().fold(0u32, |acc, &b| {
                                    acc.wrapping_mul(31).wrapping_add(b as u32)
                                });
                            let pin_id =
                                pin.pin_name.as_bytes().iter().fold(0u32, |acc, &b| {
                                    acc.wrapping_mul(31).wrapping_add(b as u32)
                                });
                            hwc_physics::PinPosition {
                                component_id,
                                pin_id,
                                x_nm: pin.x_nm,
                                y_nm: pin.y_nm,
                                z_nm: pin.z_nm,
                            }
                        })
                        .collect()
                };

            // Build islands and validate
            let islands =
                physical_continuity_checker.build_conductive_islands(Some(&pin_positions));
            let bindings = physical_continuity_checker.bind_nets_to_islands(&islands);
            let continuity_violations =
                physical_continuity_checker.validate_continuity(&islands, &bindings, false);

            let continuity_duration = continuity_start.elapsed();
            println!(
                "[{:>8.2}ms] Physical continuity check completed in {:.2}ms",
                start_time.elapsed().as_secs_f64() * 1000.0,
                continuity_duration.as_secs_f64() * 1000.0
            );

            if !continuity_violations.is_empty() {
                println!("\n❌ PHYSICAL CONTINUITY VIOLATIONS - Cannot proceed to parameter validation:");
                for violation in &continuity_violations {
                    match violation {
                        hwc_physics::PhysicalContinuityViolation::DisconnectedNet {
                            net_name,
                            island_count,
                            islands,
                            suggested_fix,
                        } => {
                            println!("\n   P41: Disconnected Net '{}'", net_name);
                            println!("   → Net has {} disconnected islands", island_count);
                            for (i, island) in islands.iter().enumerate() {
                                println!(
                                    "      Island {}: {} nodes at ({:.1}, {:.1}, {:.1})mm",
                                    i + 1,
                                    island.node_count,
                                    island.bbox.min_x as f64 / 1e6,
                                    island.bbox.min_y as f64 / 1e6,
                                    island.bbox.min_z as f64 / 1e6
                                );
                            }
                            println!("   💡 {}", suggested_fix);
                        }
                        hwc_physics::PhysicalContinuityViolation::ShortCircuit {
                            net_names,
                            suggested_fix,
                            ..
                        } => {
                            println!("\n   P42: Short Circuit");
                            println!("   → Multiple nets: {:?}", net_names);
                            println!("   💡 {}", suggested_fix);
                        }
                        hwc_physics::PhysicalContinuityViolation::FloatingConductor {
                            material_name,
                            bbox,
                            suggested_fix,
                            ..
                        } => {
                            println!("\n   P43: Floating Conductor");
                            println!("   → Material: {}", material_name);
                            println!(
                                "   → Location: ({:.1}, {:.1}, {:.1})mm",
                                bbox.min_x as f64 / 1e6,
                                bbox.min_y as f64 / 1e6,
                                bbox.min_z as f64 / 1e6
                            );
                            println!("   💡 {}", suggested_fix);
                        }
                    }
                }
                
                // Task 5.3: Respect --force-export flag
                if config.force_export {
                    println!("\n   ⚠️  --force-export: Continuing despite {} physical continuity violation(s)", 
                        continuity_violations.len());
                } else {
                    return Err(miette::miette!(
                        "Physical continuity validation failed with {} violation(s). Alignment Layer cannot validate fragmented nets.",
                        continuity_violations.len()
                    ));
                }
            } else {
                println!("   ✅ Physical continuity validated - all nets are continuous");
            }
        } else {
            println!(
                "\n   ⚠️  Physical continuity check skipped (--skip-physical-continuity flag)"
            );
        }

        // Sprint 4.1: Run Alignment Layer validation (Triple-Check Architecture)
        // Layer 1: Symbolic Alignment (device names, types)
        // Layer 2: Physical Continuity (already validated above)
        // Layer 3: Device Extraction (parameter validation)
        
        // TODO: Re-enable when AlignmentValidator is available in hwc_compiler
        /*
        if !config.skip_alignment {
            println!("\n🔍 Running Alignment Layer Validation (Triple-Check Architecture)...");
            // println!("[DEBUG] Starting Alignment check at {:.2}ms",
            //       start_time.elapsed().as_secs_f64() * 1000.0
            //   );

            let alignment_start = std::time::Instant::now();

            // Get module definition for logical graph extraction
            let module_def = module_def.ok_or_else(|| {
                miette::miette!("Module definition required for Alignment Layer validation")
            })?;

            // Create Alignment Validator
            let alignment_validator = hwc_compiler::AlignmentValidator::new(
                extracted_netlist.clone(),
                module_def,
                Some(symbol_table),
            );

            // Run validation
            let alignment_report = alignment_validator.validate();

            let alignment_duration = alignment_start.elapsed();
            println!(
                "[{:>8.2}ms] Alignment validation completed in {:.2}ms",
                start_time.elapsed().as_secs_f64() * 1000.0,
                alignment_duration.as_secs_f64() * 1000.0
            );

            // Print report
            if alignment_report.passed {
                println!("   ✅ ALIGNMENT PASSED - Layout implements module correctly");
                println!(
                    "      Devices: {} physical == {} logical",
                    alignment_report.physical_device_count, alignment_report.logical_device_count
                );
                println!(
                    "      Nets: {} physical == {} logical",
                    alignment_report.physical_net_count, alignment_report.logical_net_count
                );
            } else {
                println!(
                    "   ❌ ALIGNMENT FAILED - {} violation(s) found\n",
                    alignment_report.violations.len()
                );
                println!("{}", alignment_report);
                return Err(miette::miette!(
                    "Alignment validation failed with {} violation(s)",
                    alignment_report.violations.len()
                ));
            }
        } else {
            println!("   ⚠️  Alignment validation skipped (--skip-alignment flag)");
        }
        */

        // Task 4.3: Run Bulk Connection Validation
        if !config.skip_bulk_validation {
            println!("\n🔍 Running Bulk Connection Validation...");
            let bulk_start = std::time::Instant::now();

            // Create material database for physics-driven validation
            let material_database = hwc_compiler::populate_material_database(symbol_table)
                .unwrap_or_else(|_| hwc_materials::MaterialDatabase::empty());

            // Create bulk validator
            let bulk_validator = hwc_engine::BulkValidator::new(material_database);

            // Prepare device data for validation (convert from PhysicalNetlist to simple tuples)
            type DeviceTuple = (
                CompactString,
                CompactString,
                rustc_hash::FxHashMap<CompactString, String>,
                rustc_hash::FxHashMap<CompactString, String>,
            );
            let devices: Vec<DeviceTuple> = extracted_netlist
                .devices
                .iter()
                .map(|device| {
                    let device_type_name = extracted_netlist
                        .device_registry
                        .get_name(device.device_type_id)
                        .unwrap_or("UNKNOWN");
                    (
                        device.name.clone(),
                        device_type_name.into(),
                        device.terminals.clone(),
                        device.terminal_pours.clone(),
                    )
                })
                .collect();

            // Run validation
            match bulk_validator.validate_bulk_connections(&devices, space) {
                Ok(()) => {
                    let bulk_duration = bulk_start.elapsed();
                    println!(
                        "[{:>8.2}ms] Bulk validation completed in {:.2}ms",
                        start_time.elapsed().as_secs_f64() * 1000.0,
                        bulk_duration.as_secs_f64() * 1000.0
                    );
                    println!("   ✅ All bulk connections validated - proper biasing confirmed");
                }
                Err(errors) => {
                    println!(
                        "   ❌ BULK CONNECTION VIOLATIONS - {} error(s) found\n",
                        errors.len()
                    );
                    for error in &errors {
                        println!("{}\n", error);
                    }
                    return Err(miette::miette!(
                        "Bulk connection validation failed with {} violation(s)",
                        errors.len()
                    ));
                }
            }
        } else {
            println!("\n   ⚠️  Bulk connection validation skipped (--skip-bulk-validation flag)");
        }

        Ok(Some(extracted_netlist))
    }
}
