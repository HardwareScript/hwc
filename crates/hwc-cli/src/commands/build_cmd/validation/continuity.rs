use crate::commands::build_cmd::BuildConfig;
use hwc_engine::HardwareSpace;
use hwc_physics::error_mapping::PhysicsError;
use miette::Result;
use std::time::Instant;

/// Run Layer 3: Physical Continuity Validation
pub fn run_physical_continuity_check(
    space: &HardwareSpace,
    physics_pours: &[hwc_physics::connectivity::PourMetadata],
    physics_contacts: &[hwc_physics::connectivity::ContactMetadata],
    physics_substrate_layers: &[hwc_physics::connectivity::SubstrateLayerMetadata],
    config: &BuildConfig,
    start_time: Instant,
) -> Result<Vec<PhysicsError>> {
    let continuity_start = Instant::now();

    if config.verbose {
        println!("\n🔍 Running Layer 3: Physical Continuity Validation (Conductive Walk)...");
    }

    // Get bridge rules from profile
    let bridge_rules: Vec<hwc_physics::BridgeRule> =
        if let Some(ref constraints) = space.fabrication_constraints {
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
        physics_pours,
        physics_contacts,
        physics_substrate_layers,
        &bridge_rules,
        material_mapping,
    );

    // Extract pin positions from real components only
    let pin_positions = extract_pin_positions(space, config);

    // Build conductive islands using flood-fill
    let islands = physical_continuity_checker.build_conductive_islands(Some(&pin_positions));

    if config.verbose {
        println!("  Built {} conductive islands", islands.len());

        let total_pins: usize = islands.iter().map(|i| i.pins.len()).sum();
        let islands_with_pins = islands.iter().filter(|i| !i.pins.is_empty()).count();
        println!(
            "  Found {} pins touching {} islands",
            total_pins, islands_with_pins
        );
    }

    // Bind logical nets to physical islands
    let bindings = physical_continuity_checker.bind_nets_to_islands(&islands);

    if config.verbose {
        println!("  Mapped {} nets to islands", bindings.len());
    }

    // Validate physical continuity
    // P43 (Floating Conductor) check should only run if the module has pins defined
    let enable_p43 = !pin_positions.is_empty();

    if !enable_p43 && config.verbose {
        println!("  ℹ️  P43 check skipped: Module has no pins defined");
    }

    let continuity_violations =
        physical_continuity_checker.validate_continuity(&islands, &bindings, enable_p43);

    let mut errors = Vec::new();

    if !continuity_violations.is_empty() {
        print_continuity_violations(&continuity_violations);
        // Convert to PhysicsError
        for violation in &continuity_violations {
            errors.push(hwc_physics::error_mapping::physical_continuity_to_error(
                violation,
            ));
        }
    }

    if config.verbose && continuity_violations.is_empty() {
        println!("✅ Physical continuity check passed - all islands are properly connected");
    }
    println!(
        "[{:>8.2}ms] Physical continuity check completed in {:.2}ms",
        start_time.elapsed().as_secs_f64() * 1000.0,
        continuity_start.elapsed().as_secs_f64() * 1000.0
    );

    Ok(errors)
}

/// Extract pin positions from real components (not virtual anchors)
/// v0.1.6 Sprint 3: Now uses component_pins from VoxelGrid instead of netlist
pub fn extract_pin_positions(
    space: &HardwareSpace,
    config: &BuildConfig,
) -> Vec<hwc_physics::PinPosition> {
    let mut pin_positions = Vec::new();

    // v0.1.8: Get component pins from entity graph
    let component_pins = space.entity_graph.get_component_pins();

    if config.verbose {
        println!(
            "  Extracting {} component pins for P43 validation",
            component_pins.len()
        );
    }

    // Convert ComponentPin to PinPosition format
    for pin in component_pins.iter() {
        // Generate unique IDs from names (simple hash)
        let component_id = pin
            .component_name
            .as_bytes()
            .iter()
            .fold(0u32, |acc, &b| acc.wrapping_mul(31).wrapping_add(b as u32));
        let pin_id = pin
            .pin_name
            .as_bytes()
            .iter()
            .fold(0u32, |acc, &b| acc.wrapping_mul(31).wrapping_add(b as u32));

        pin_positions.push(hwc_physics::PinPosition {
            component_id,
            pin_id,
            x_nm: pin.x_nm,
            y_nm: pin.y_nm,
            z_nm: pin.z_nm,
        });
    }

    pin_positions
}

/// Print physical continuity violations
pub fn print_continuity_violations(violations: &[hwc_physics::PhysicalContinuityViolation]) {
    println!("\n❌ PHYSICAL CONTINUITY VIOLATIONS DETECTED:");
    for violation in violations {
        match violation {
            hwc_physics::PhysicalContinuityViolation::DisconnectedNet {
                net_name,
                island_count,
                islands,
                suggested_fix,
            } => {
                println!(
                    "  • Net '{}': {} disconnected islands (P41: Physical Disconnection)",
                    net_name, island_count
                );
                for island in islands {
                    println!(
                        "    - Island {} at z:{:.3}-{:.3} mm ({} nodes, {} pins)",
                        island.id,
                        island.bbox.min_z as f64 / 1_000_000.0,
                        island.bbox.max_z as f64 / 1_000_000.0,
                        island.node_count,
                        island.pin_count
                    );
                }
                println!("    💡 FIX: {}", suggested_fix);
            }
            hwc_physics::PhysicalContinuityViolation::ShortCircuit {
                island_id,
                net_names,
                overlap_location,
                suggested_fix,
            } => {
                println!(
                    "  • Island {}: Short circuit detected (P42: Short Circuit)",
                    island_id
                );
                println!("    Nets: {}", net_names.join(", "));
                println!("    Location: {}", overlap_location);
                println!("    💡 FIX: {}", suggested_fix);
            }
            hwc_physics::PhysicalContinuityViolation::FloatingConductor {
                island_id,
                material_name,
                bbox,
                suggested_fix,
            } => {
                println!(
                    "  • Island {}: Floating conductor (P43: Floating Conductor)",
                    island_id
                );
                println!("    Material: {}", material_name);
                println!(
                    "    Location: x:{:.3}-{:.3}, y:{:.3}-{:.3}, z:{:.3}-{:.3} mm",
                    bbox.min_x as f64 / 1_000_000.0,
                    bbox.max_x as f64 / 1_000_000.0,
                    bbox.min_y as f64 / 1_000_000.0,
                    bbox.max_y as f64 / 1_000_000.0,
                    bbox.min_z as f64 / 1_000_000.0,
                    bbox.max_z as f64 / 1_000_000.0
                );
                println!("    💡 FIX: {}", suggested_fix);
            }
        }
    }
}
