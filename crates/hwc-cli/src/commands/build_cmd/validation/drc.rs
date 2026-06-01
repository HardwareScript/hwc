use crate::commands::build_cmd::BuildConfig;
use hwc_engine::HardwareSpace;
use miette::Result;
use std::time::Instant;

/// Run Design Rule Check (DRC)
pub fn run_drc_check(space: &HardwareSpace, config: &BuildConfig, _start_time: Instant) -> Result<()> {
    // Skip DRC if no fabrication constraints are defined
    // DRC requires a profile with min_trace_width, min_spacing, etc.
    if space.fabrication_constraints.is_none() {
        if config.verbose {
            println!("ℹ️  DRC skipped: No fabrication profile defined");
            println!("   Add a 'profile:' clause to your space to enable DRC");
        }
        return Ok(());
    }

    if config.verbose {
        println!("🔍 Running Design Rule Check (DRC)...");
    }

    use hwc_engine::constraint_manager::ConstraintRulebook;
    use hwc_engine::design_rule_check::{DesignRuleChecker, NetVoxels};
    use rustc_hash::FxHashMap;

    let mut current_net_voxels: FxHashMap<
        hwc_engine::netlist::NetHandle,
        Vec<hwc_engine::Point3D>,
    > = FxHashMap::default();

    // Collect voxels from main grid
    for (x, y, z, _material, net_id) in space.voxel_grid.iter_occupied() {
        let pos = space.voxel_to_position(x, y, z);
        current_net_voxels.entry(net_id).or_default().push(pos);
    }

    // PRIMITIVES OVER PIXELS: Use substrate layer bounding boxes directly (analytic geometry)
    let substrate_layers = space.voxel_grid.get_substrate_layers();

    // Track geometry types for each net (for proper thermal analysis)
    use hwc_engine::design_rule_check::GeometryType;
    let mut net_geometry_types: FxHashMap<hwc_engine::netlist::NetHandle, GeometryType> =
        FxHashMap::default();

    for layer in substrate_layers.iter() {
        // Get net handle from net ID
        let net_handle = hwc_engine::netlist::NetHandle::new(layer.net);

        // Determine geometry type from substrate layer properties
        let z_span = layer.bbox.max.z - layer.bbox.min.z;
        let xy_area = (layer.bbox.max.x - layer.bbox.min.x) * (layer.bbox.max.y - layer.bbox.min.y);

        // Heuristic for geometry type classification (physical Z only):
        let geometry_type = if z_span > 2 * space.voxel_size.z_nm && xy_area < 10_000_000_000 {
            // Vertical structure with small footprint = Contact/Via
            GeometryType::Contact
        } else {
            // Horizontal structure or large area = Pour/Pad
            GeometryType::Pour
        };

        // Store geometry type for this net
        net_geometry_types.insert(net_handle, geometry_type);

        // ANALYTIC APPROACH: Store just the bounding box center point as a representative
        let center = hwc_engine::Point3D::new(
            (layer.bbox.min.x + layer.bbox.max.x) / 2,
            (layer.bbox.min.y + layer.bbox.max.y) / 2,
            (layer.bbox.min.z + layer.bbox.max.z) / 2,
        );
        current_net_voxels
            .entry(net_handle)
            .or_default()
            .push(center);
    }

    // Task 4.2: Add via geometry from ContactMetadata for DRC validation
    for contact in space.contacts.iter() {
        // Get net handle from contact's net name
        if let Some(ref net_name) = contact.net {
            // Find the net in the netlist
            if let Some(net_data) = space.netlist.get_net_by_name(net_name.as_str()) {
                let net_handle = hwc_engine::netlist::NetHandle::new(net_data.raw());

                // ANALYTIC APPROACH: Store just the bounding box center point as a representative
                if let Some(ref bbox) = contact.bbox {
                    let center = hwc_engine::Point3D::new(
                        (bbox.min.x + bbox.max.x) / 2,
                        (bbox.min.y + bbox.max.y) / 2,
                        (bbox.min.z + bbox.max.z) / 2,
                    );
                    current_net_voxels
                        .entry(net_handle)
                        .or_default()
                        .push(center);

                    // Mark this net as having Contact geometry (for proper DRC)
                    net_geometry_types.insert(net_handle, GeometryType::Contact);
                }
            }
        }
    }

    // Convert to NetVoxels
    let nets: Vec<NetVoxels> = current_net_voxels
        .into_iter()
        .map(|(net_id, voxels)| {
            // Get net name from netlist
            let net_name = space
                .netlist
                .get_net(hwc_engine::netlist::NetId::new(net_id.raw()))
                .map(|net_data| net_data.name.clone())
                .unwrap_or_else(|| format!("net_{}", net_id.raw()).into());

            // Get geometry type for this net (default to Trace for routed nets)
            let geometry_type = net_geometry_types
                .get(&net_id)
                .copied()
                .unwrap_or(GeometryType::Trace);

            let classification = space.net_classifications.get(&net_name)
                .copied()
                .unwrap_or(hwc_engine::space::NetClassification::Unclassified);

            NetVoxels {
                net_name,
                voxels,
                geometry_type,
                classification,
            }
        })
        .collect();

    if config.verbose {
        println!(
            "   Found {} nets with {} total voxels",
            nets.len(),
            nets.iter().map(|n| n.voxels.len()).sum::<usize>()
        );
    }

    // Create constraint rulebook
    let mut constraint_rulebook = ConstraintRulebook::new(space.voxel_size.x_nm);

    // Load fabrication constraints from profile (v0.1.6: DRC Integration)
    if let Some(ref constraints) = space.fabrication_constraints {
        use hwc_engine::constraint_manager::{FabricationConstraints, StackupInfo};

        // Convert stackup constraints if available
        let stackup = constraints.stackup.as_ref().map(|s| StackupInfo {
            dielectric_height_nm: s.dielectric_height_nm,
            copper_thickness_nm: s.copper_thickness_nm,
            relative_permittivity: s.relative_permittivity,
            default_impedance_ohm: s.default_impedance_ohm,
        });

        let fab_constraints = FabricationConstraints {
            min_trace_width_nm: constraints.trace.min_width_nm,
            min_trace_spacing_nm: constraints.trace.min_spacing_nm,
            min_via_diameter_nm: constraints.via.min_diameter_nm,
            default_via_diameter_nm: constraints.via.default_diameter_nm,
            min_annular_ring_nm: constraints.via.min_annular_ring_nm,
            min_spacing_nm: constraints.via.min_spacing_nm,
            high_voltage_clearance_nm: Some(constraints.clearance.high_voltage_nm),
            safety_factor: constraints.clearance.safety_factor,
            stackup,
        };

        constraint_rulebook.set_fabrication_constraints(fab_constraints);

        // Load thermal constraints from profile (v0.1.6: Thermal Integration)
        if let Some(ref thermal) = constraints.thermal {
            constraint_rulebook.max_temp_rise_c = Some(thermal.max_temp_rise_c);
        } else {
            // No thermal constraints in profile - use safe defaults
            constraint_rulebook.default_current_ma = Some(20); // 20mA for signal traces
            constraint_rulebook.max_temp_rise_c = Some(10.0); // 10°C rise (conservative)
        }
    }

    // Run DRC
    let drc_checker = DesignRuleChecker::default();
    let mut drc_report = drc_checker.check(&nets, &constraint_rulebook, space.voxel_size.x_nm);

    // Task 4.2: Run analytic via checks (Primitives Over Pixels)
    let via_diameter_report = hwc_engine::design_rule_check::validate_via_diameters_analytic(
        &space.contacts,
        &constraint_rulebook,
    );

    // Merge via diameter violations into main report
    for violation in via_diameter_report.violations {
        drc_report.add_violation(violation);
    }

    // Run analytic via enclosure check
    let substrate_layers = space.voxel_grid.get_substrate_layers();
    let via_enclosure_report = hwc_engine::design_rule_check::validate_via_enclosure_analytic(
        &space.contacts,
        substrate_layers,
        &constraint_rulebook,
        &space.netlist,
        &space.material_registry,
    );

    // Merge via enclosure violations into main report
    for violation in via_enclosure_report.violations {
        drc_report.add_violation(violation);
    }

    // v0.1.7: Run analytic drill-to-drill clearance check (Primitives Over Pixels)
    let drill_clearance_report = hwc_engine::design_rule_check::validate_drill_to_drill_clearance(
        &space.contacts,
        &constraint_rulebook,
    );

    // Merge drill clearance violations into main report
    for violation in drill_clearance_report.violations {
        drc_report.add_violation(violation);
    }

    // v0.1.7: Run God-Tier Physics Validator (Bit-Parallel)
    let silicon_id = space.material_registry.get_id("Silicon");
    let physics_validator = hwc_engine::physics_validator::PhysicsValidator::new();
    let physics_report = physics_validator.validate_parallel(&space.voxel_grid, silicon_id);

    // Merge physics violations into DRC report
    for violation in physics_report.violations {
        match violation {
            hwc_engine::physics_validator::PhysicsViolation::SubstrateShortCircuit {
                net,
                substrate_material,
                location,
            } => {
                let net_name = space
                    .netlist
                    .get_net(hwc_engine::netlist::NetId::new(net))
                    .map(|n| n.name.clone())
                    .unwrap_or_else(|| format!("net_{}", net).into());
                let mat_name: compact_str::CompactString = space
                    .material_registry
                    .get_name(substrate_material)
                    .map(Into::into)
                    .unwrap_or_else(|| format!("material_{}", substrate_material).into());

                drc_report.add_violation(
                    hwc_engine::design_rule_check::DrcViolation::SubstrateShortCircuit {
                        net: net_name,
                        substrate_material: mat_name,
                        location,
                    },
                );
            }
            hwc_engine::physics_validator::PhysicsViolation::KozViolation {
                net,
                location,
                reason,
            } => {
                let net_name = space
                    .netlist
                    .get_net(hwc_engine::netlist::NetId::new(net))
                    .map(|n| n.name.clone())
                    .unwrap_or_else(|| format!("net_{}", net).into());

                drc_report.add_violation(
                    hwc_engine::design_rule_check::DrcViolation::KozViolation {
                        net: net_name,
                        location,
                        reason,
                    },
                );
            }
            _ => {} // Standard short circuits and clearances are handled by existing DRC
        }
    }

    if !drc_report.is_valid() {
        println!("\n❌ DRC VIOLATIONS DETECTED:");
        
        // Group violations by type for cleaner output
        use rustc_hash::FxHashMap;
        let mut grouped: FxHashMap<String, Vec<String>> = FxHashMap::default();
        
        for violation in &drc_report.violations {
            let violation_str = violation.to_string();
            
            // Extract generic violation type for grouping (v0.1.7: Improved grouping)
            let violation_type = if violation_str.starts_with("Drill clearance:") {
                "Drill clearance violation".to_string()
            } else if violation_str.starts_with("Clearance violation") {
                "Clearance violation".to_string()
            } else if let Some(pos) = violation_str.find(" at ") {
                violation_str[..pos].to_string()
            } else {
                violation_str.clone()
            };
            
            grouped.entry(violation_type).or_insert_with(Vec::new).push(violation_str);
        }
        
        // Print grouped violations
        for (_violation_type, instances) in grouped.iter() {
            if instances.len() == 1 {
                // Single violation: print normally
                println!("  • {}", instances[0]);
            } else if instances.len() <= 3 {
                // Few violations: print all
                for instance in instances {
                    println!("  • {}", instance);
                }
            } else {
                // Many violations: print first 2 and summarize
                println!("  • {}", instances[0]);
                println!("  • {}", instances[1]);
                println!("  • ... and {} more similar violations", instances.len() - 2);
            }
        }
        
        return Err(miette::miette!(
            "Design rule check failed with {} violation(s)",
            drc_report.violations.len()
        ));
    }

    if config.verbose {
        println!("✅ DRC passed - no violations detected");
    }

    Ok(())
}
