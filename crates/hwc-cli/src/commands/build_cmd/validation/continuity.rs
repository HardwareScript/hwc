use crate::commands::build_cmd::BuildConfig;
use hwc_engine::HardwareSpace;
use hwc_physics::error_mapping::PhysicsError;
use miette::Result;
use std::time::Instant;

pub fn run_physical_continuity_check(
    space: &HardwareSpace,
    physics_substrate_layers: &[hwc_physics::connectivity::SubstrateLayerMetadata],
    physics_route_segments: &[hwc_physics::RouteSegmentMetadata],
    config: &BuildConfig,
    start_time: Instant,
) -> Result<Vec<PhysicsError>> {
    let continuity_start = Instant::now();

    if config.verbose {
        println!("\n🔍 Running Physical Continuity Validation...");
    }

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

    let mut material_mapping = rustc_hash::FxHashMap::default();
    for (id, name) in space.material_registry.all_materials() {
        material_mapping.insert(compact_str::CompactString::from(name), id);
    }

    let physical_continuity_checker = hwc_physics::PhysicalContinuityChecker::new(
        physics_substrate_layers,
        physics_route_segments,
        &bridge_rules,
        material_mapping,
    );

    let pin_positions = extract_pin_positions(space, config);

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

    let bindings = physical_continuity_checker.bind_nets_to_islands(&islands);

    if config.verbose {
        println!("  Mapped {} nets to islands", bindings.len());
    }

    let enable_p43 = !pin_positions.is_empty();

    if !enable_p43 && config.verbose {
        println!("  ℹ️  P43 check skipped: Module has no pins defined");
    }

    let continuity_violations =
        physical_continuity_checker.validate_continuity(&islands, &bindings, enable_p43);

    let mut errors = Vec::new();

    if !continuity_violations.is_empty() {
        print_continuity_violations(&continuity_violations);
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

pub fn extract_pin_positions(
    space: &HardwareSpace,
    config: &BuildConfig,
) -> Vec<hwc_physics::PinPosition> {
    let mut pin_positions = Vec::new();

    let component_pins = space.entity_graph.get_component_pins();

    if config.verbose {
        println!(
            "  Extracting {} component pins for P43 validation",
            component_pins.len()
        );
    }

    for pin in component_pins.iter() {
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

pub fn print_continuity_violations(violations: &[hwc_physics::PhysicalContinuityViolation]) {
    println!("\n❌ PHYSICAL CONTINUITY VIOLATIONS:");
    for violation in violations {
        match violation {
            hwc_physics::PhysicalContinuityViolation::DisconnectedNet {
                net_name,
                island_count,
                islands,
                suggested_fix,
            } => {
                println!(
                    "  P41: Net '{}' has {} disconnected islands",
                    net_name, island_count
                );
                for island in islands {
                    println!(
                        "    Island {} at ({:.3}, {:.3}, {:.3})-({:.3}, {:.3}, {:.3}) mm ({} nodes, {} pins)",
                        island.id,
                        island.bbox.min_x as f64 / 1_000_000.0,
                        island.bbox.min_y as f64 / 1_000_000.0,
                        island.bbox.min_z as f64 / 1_000_000.0,
                        island.bbox.max_x as f64 / 1_000_000.0,
                        island.bbox.max_y as f64 / 1_000_000.0,
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
                println!("  P42: Island {}: Short circuit", island_id);
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
                println!("  P43: Island {}: Floating conductor ({})", island_id, material_name);
                println!(
                    "    Location: ({:.3}, {:.3}, {:.3})-({:.3}, {:.3}, {:.3}) mm",
                    bbox.min_x as f64 / 1_000_000.0,
                    bbox.min_y as f64 / 1_000_000.0,
                    bbox.min_z as f64 / 1_000_000.0,
                    bbox.max_x as f64 / 1_000_000.0,
                    bbox.max_y as f64 / 1_000_000.0,
                    bbox.max_z as f64 / 1_000_000.0,
                );
                println!("    💡 FIX: {}", suggested_fix);
            }
        }
    }
}
