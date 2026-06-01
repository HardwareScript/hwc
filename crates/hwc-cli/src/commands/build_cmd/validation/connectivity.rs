use crate::commands::build_cmd::BuildConfig;
use hwc_engine::HardwareSpace;
use hwc_physics::error_mapping::PhysicsError;
use miette::Result;
use std::io::Write;
use std::time::Instant;

use super::continuity::run_physical_continuity_check;
use super::utils::convert_metadata_to_physics;

/// Run connectivity check (Layer 2 and Layer 3)
pub fn run_connectivity_check(
    space: &HardwareSpace,
    config: &BuildConfig,
    start_time: Instant,
) -> Result<Vec<PhysicsError>> {
    // Pre-check: Error P43 - Unassigned Conductor Detection
    check_unassigned_conductors(space)?;

    println!("🔌 Running Connectivity Check (Electrical Borrow Checker)...");
    std::io::stdout().flush().unwrap();
    let connectivity_start = Instant::now();

    // Convert metadata to physics format
    let (physics_pours, physics_contacts, physics_substrate_layers) =
        convert_metadata_to_physics(space);

    // Create connectivity checker
    use hwc_physics::connectivity::ConnectivityChecker;
    let connectivity_checker = ConnectivityChecker::new(
        space.voxel_size.z_nm,
        &physics_pours,
        &physics_contacts,
        &physics_substrate_layers,
    );

    // Run Layer 2 connectivity validation
    let connectivity_violations = connectivity_checker.validate_all_nets();

    let mut all_errors = Vec::new();

    if !connectivity_violations.is_empty() {
        print_connectivity_violations(&connectivity_violations);
        // Convert to PhysicsError
        for violation in &connectivity_violations {
            all_errors.push(hwc_physics::error_mapping::connectivity_to_error(violation));
        }
    }

    if config.verbose {
        if connectivity_violations.is_empty() {
            println!("✅ Connectivity check passed - all nets are physically connected");
        } else {
            println!(
                "⚠️  Connectivity check found {} violation(s) - continuing to Layer 3",
                connectivity_violations.len()
            );
        }
    }
    println!(
        "[{:>8.2}ms] Connectivity check completed in {:.2}ms",
        start_time.elapsed().as_secs_f64() * 1000.0,
        connectivity_start.elapsed().as_secs_f64() * 1000.0
    );

    // Run Layer 3: Physical Continuity Validation
    let continuity_errors = run_physical_continuity_check(
        space,
        &physics_pours,
        &physics_contacts,
        &physics_substrate_layers,
        config,
        start_time,
    )?;

    all_errors.extend(continuity_errors);

    Ok(all_errors)
}

/// Check for unassigned conductors (P43 pre-check)
pub fn check_unassigned_conductors(space: &HardwareSpace) -> Result<()> {
    println!("🔍 Checking for unassigned conductors...");
    let mut floating_errors = Vec::new();

    let conductive_materials = ["Aluminum", "Copper", "Gold", "Silver", "Tungsten"];

    // Check pours
    for pour in &space.pours {
        if pour.net.is_none() && conductive_materials.contains(&pour.material_name.as_str()) {
            floating_errors.push(format!(
                "Pour '{}' ({}) has no 'net:' assignment",
                pour.name, pour.material_name
            ));
        }
    }

    // Check contacts
    for contact in &space.contacts {
        if contact.net.is_none() && conductive_materials.contains(&contact.material_name.as_str()) {
            floating_errors.push(format!(
                "Contact '{}' ({}) has no 'net:' assignment",
                contact.name, contact.material_name
            ));
        }
    }

    if !floating_errors.is_empty() {
        println!("\n❌ ERROR P43: Unassigned Conductor(s) Detected");
        println!("   Conductive geometry without net assignment can cause:");
        println!("   • EMI antenna effects");
        println!("   • Signal integrity issues");
        println!("   • Unpredictable coupling\n");

        for error in &floating_errors {
            println!("   • {}", error);
        }

        println!("\n   Required fix: Add 'net: NetName' to each conductor:");
        println!("   add contact(Aluminum) named Via_Gate net: VIN at [...]");
        println!("\n   If intentional (thermal via, dummy fill), add a comment:");
        println!("   add contact(...) named MyVia at [...] # Thermal via\n");

        return Err(miette::miette!(
            "Unassigned conductor check failed with {} error(s)",
            floating_errors.len()
        ));
    }

    Ok(())
}

/// Print connectivity violations
pub fn print_connectivity_violations(violations: &[hwc_physics::connectivity::ConnectivityViolation]) {
    println!("\n❌ CONNECTIVITY VIOLATIONS DETECTED:");
    for violation in violations {
        match violation {
            hwc_physics::connectivity::ConnectivityViolation::DisconnectedNet {
                net_name,
                pour_a,
                pour_b,
                reason,
                smart_hint,
            } => {
                println!(
                    "  • Net '{}': No physical path between '{}' and '{}'",
                    net_name, pour_a, pour_b
                );
                println!("    Reason: {}", reason);

                if let Some(hint) = smart_hint {
                    println!("\n    💡 SMART HINT: {}", hint);
                } else if reason.contains("Z-layer gap") || reason.contains("Z-height") {
                    println!("    💡 Suggestion: Add a via to bridge the Z-height gap");
                } else if reason.contains("gap") {
                    println!("    💡 Suggestion: Add trace or use route command to connect pours");
                } else {
                    println!("    💡 Suggestion: Check for insulating material between pours");
                }
            }
            hwc_physics::connectivity::ConnectivityViolation::MaterialInterpenetration {
                net_name,
                pour_a,
                pour_b,
                material_a,
                material_b,
                overlap_location,
            } => {
                println!("  • Net '{}': Material interpenetration detected", net_name);
                println!(
                    "    Pour '{}' (material: {}) overlaps with pour '{}' (material: {})",
                    pour_a, material_a, pour_b, material_b
                );
                println!("    Location: {}", overlap_location);
                println!("    💡 Suggestion: Adjust boundaries so pours touch at edges but do not overlap");
            }
        }
    }
}
