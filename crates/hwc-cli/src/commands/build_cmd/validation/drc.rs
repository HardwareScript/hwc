use crate::commands::build_cmd::BuildConfig;
use hwc_engine::HardwareSpace;
use miette::Result;
use std::time::Instant;

/// Run Design Rule Check (DRC)
pub fn run_drc_check(
    space: &HardwareSpace,
    config: &BuildConfig,
    _start_time: Instant,
) -> Result<()> {
    // Skip DRC if no fabrication constraints are defined
    // DRC requires a profile with min_trace_width, min_spacing, etc.
    if space.fabrication_constraints.is_none() {
        eprintln!("[DRC DEBUG] Skipped - no fabrication constraints");
        if config.verbose {
            println!("ℹ️  DRC skipped: No fabrication profile defined");
            println!("   Add a 'profile:' clause to your space to enable DRC");
        }
        return Ok(());
    }

    eprintln!("[DRC DEBUG] Running DRC check...");
    eprintln!(
        "[DRC DEBUG] Spatial index: {} entities",
        space.entity_graph.spatial().len()
    );
    eprintln!(
        "[DRC DEBUG] Routed segments: {} nets",
        space.entity_graph.get_all_routes().len()
    );
    eprintln!(
        "[DRC DEBUG] Analytic routes: {} traces",
        space.analytic_routes.len()
    );
    eprintln!(
        "[DRC DEBUG] Substrate layers: {} pours",
        space.entity_graph.get_substrate_layers().len()
    );

    // Count route segments
    let mut total_route_segs = 0;
    for (_net_id, segments) in space.entity_graph.get_all_routes() {
        total_route_segs += segments.len();
    }
    eprintln!(
        "[DRC DEBUG] Total route segments across all nets: {}",
        total_route_segs
    );

    if config.verbose {
        println!("🔍 Running Design Rule Check (DRC)...");
    }

    use hwc_engine::constraint_manager::ConstraintRulebook;
    use hwc_engine::design_rule_check::DesignRuleChecker;

    // Create constraint rulebook from the fabrication profile
    let mut constraint_rulebook = ConstraintRulebook::new(space.manufacturing_grid_nm);

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
            min_enclosure_nm: constraints.via.min_enclosure_nm,
            min_spacing_nm: constraints.via.min_spacing_nm,
            low_voltage_clearance_nm: constraints.clearance.low_voltage_nm,
            medium_voltage_clearance_nm: constraints.clearance.medium_voltage_nm,
            high_voltage_clearance_nm: constraints.clearance.high_voltage_nm,
            safety_factor: constraints.clearance.safety_factor,
            stackup,
            technology: constraints.technology,
            layer_via_enclosures: constraints.via.layer_enclosures_nm.clone(),
            max_substrate_tap_distance_nm: constraints.clearance.max_substrate_tap_distance_nm,
            substrate_net: constraints.substrate_net.clone(),
        };

        constraint_rulebook.set_fabrication_constraints(fab_constraints);
    }

    // Run DRC — fully analytic (includes via checks inside validate_physics_parallel)
    let drc_checker = DesignRuleChecker::new();
    let drc_report = drc_checker
        .check(space, &constraint_rulebook)
        .map_err(|e| miette::miette!(e))?;

    // v0.1.7: Physics validator removed

    if !drc_report.is_valid() {
        println!("\n❌ DRC VIOLATIONS DETECTED:");

        // v0.1.9: DEBUG - Log spatial index state for diagnosis
        eprintln!(
            "[DRC DEBUG] Spatial index contains {} entities",
            space.entity_graph.spatial().len()
        );
        eprintln!(
            "[DRC DEBUG] Entity graph has {} routed segments",
            space.entity_graph.get_all_routes().len()
        );
        eprintln!(
            "[DRC DEBUG] Space has {} analytic routes",
            space.analytic_routes.len()
        );

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

            grouped
                .entry(violation_type)
                .or_default()
                .push(violation_str);
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
                println!(
                    "  • ... and {} more similar violations",
                    instances.len() - 2
                );
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
