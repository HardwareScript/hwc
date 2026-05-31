use crate::commands::build_cmd::BuildConfig;
use hwc_engine::HardwareSpace;
use hwc_physics::error_mapping::PhysicsError;
use miette::Result;
use std::io::Write;
use std::time::Instant;

/// Validation result (Task 5.1: Phantom Buffer)
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub passed: bool,
    pub violation_count: usize,
    pub violations: Vec<PhysicsError>,
}

impl ValidationResult {
    pub fn success() -> Self {
        Self {
            passed: true,
            violation_count: 0,
            violations: Vec::new(),
        }
    }

    pub fn failed(violations: Vec<PhysicsError>) -> Self {
        Self {
            passed: false,
            violation_count: violations.len(),
            violations,
        }
    }
}

/// Run all validation checks (DRC and connectivity)
/// Returns ValidationResult instead of Result to support Commit Gate
pub fn run_validation_checks(
    space: &HardwareSpace,
    config: &BuildConfig,
    is_artist_mode: bool,
    start_time: Instant,
) -> Result<ValidationResult> {
    let mut all_violations = Vec::new();

    // Run DRC if enabled (requires Professional Mode)
    if !config.skip_drc && !is_artist_mode {
        if let Err(e) = run_drc_check(space, config, start_time) {
            // DRC errors are already formatted, wrap them as PhysicsError
            all_violations.push(PhysicsError::new(
                "DRC",
                format!("DRC: {}", e).into(),
            ));
        }
    }

    // Run connectivity check if enabled (requires Professional Mode)
    if !config.skip_connectivity_check && !is_artist_mode {
        match run_connectivity_check(space, config, start_time) {
            Ok(violations) => {
                all_violations.extend(violations);
            }
            Err(e) => {
                all_violations.push(PhysicsError::new(
                    "CONNECTIVITY",
                    format!("Connectivity: {}", e).into(),
                ));
            }
        }
    }

    if all_violations.is_empty() {
        Ok(ValidationResult::success())
    } else {
        Ok(ValidationResult::failed(all_violations))
    }
}

/// Run Design Rule Check (DRC)
fn run_drc_check(space: &HardwareSpace, config: &BuildConfig, start_time: Instant) -> Result<()> {
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

    let _drc_start = start_time.elapsed();
    // eprintln!($3"[DEBUG] Starting DRC voxel iteration at {:.2}ms",
    //     drc_start.as_secs_f64() * 1000.0
    // );

    // Collect voxels from main grid
    for (x, y, z, _material, net_id) in space.voxel_grid.iter_occupied() {
        let pos = space.voxel_to_position(x, y, z);
        current_net_voxels.entry(net_id).or_default().push(pos);
    }

    // PRIMITIVES OVER PIXELS: Use substrate layer bounding boxes directly (analytic geometry)
    // Instead of sampling 72 million voxels, we use the sparse substrate layer metadata
    // This is the same "Primitives Over Pixels" architecture used for routing (Sprint 3.11)
    let substrate_layers = space.voxel_grid.get_substrate_layers();
    // eprintln!($3"[DEBUG] Using analytic geometry for {} substrate layers (Primitives Over Pixels)",
    //  substrate_layers.len()
    // );

    // Track geometry types for each net (for proper thermal analysis)
    use hwc_engine::design_rule_check::GeometryType;
    let mut net_geometry_types: FxHashMap<hwc_engine::netlist::NetHandle, GeometryType> =
        FxHashMap::default();

    for layer in substrate_layers.iter() {
        // eprintln!($3"[DEBUG DRC] Processing layer {}/{} (analytic)", layer_idx + 1, substrate_layers.len());

        // Get net handle from net ID
        let net_handle = hwc_engine::netlist::NetHandle::new(layer.net);

        // Determine geometry type from substrate layer properties
        let z_span = layer.bbox.max.z - layer.bbox.min.z;
        let xy_area = (layer.bbox.max.x - layer.bbox.min.x) * (layer.bbox.max.y - layer.bbox.min.y);

        // Heuristic for geometry type classification (physical Z only):
        // - Contact: tall Z-span (>2 voxel slabs) AND small XY footprint (via-like)
        // - Pour: thin Z-span or large XY area (pad/plane-like)
        let geometry_type = if z_span > 2 * space.voxel_size.z_nm && xy_area < 10_000_000_000 {
            // Vertical structure with small footprint = Contact/Via
            GeometryType::Contact
        } else {
            // Horizontal structure or large area = Pour/Pad
            GeometryType::Pour
        };

        // eprintln!($3"[DEBUG DRC] Layer {} geometry type: {:?}", layer_idx + 1, geometry_type);

        // Store geometry type for this net
        net_geometry_types.insert(net_handle, geometry_type);

        // ANALYTIC APPROACH: Store just the bounding box center point as a representative
        // DRC checks will use bounding box geometry directly (no voxel sampling)
        let center = hwc_engine::Point3D::new(
            (layer.bbox.min.x + layer.bbox.max.x) / 2,
            (layer.bbox.min.y + layer.bbox.max.y) / 2,
            (layer.bbox.min.z + layer.bbox.max.z) / 2,
        );
        current_net_voxels
            .entry(net_handle)
            .or_default()
            .push(center);

        // eprintln!($3"[DEBUG DRC] Layer {} registered as analytic primitive (center: {:.3}mm, {:.3}mm, {:.3}mm)",
        // layer_idx + 1,
        // center.x as f64 / 1_000_000.0,
        // center.y as f64 / 1_000_000.0,
        //     center.z as f64 / 1_000_000.0
        //  );
    }

    // Task 4.2: Add via geometry from ContactMetadata for DRC validation
    // PRIMITIVES OVER PIXELS: Use contact bounding boxes directly (analytic geometry)
    // eprintln!($3"[DEBUG DRC] Adding {} contact/via geometries to DRC (analytic)", space.contacts.len());
    for contact in space.contacts.iter() {
        // eprintln!($3"[DEBUG DRC] Processing contact {}/{}: '{}'",
        // contact_idx + 1, space.contacts.len(), contact.name);

        // Get net handle from contact's net name
        if let Some(ref net_name) = contact.net {
            // Find the net in the netlist
            if let Some(net_data) = space.netlist.get_net_by_name(net_name.as_str()) {
                let net_handle = hwc_engine::netlist::NetHandle::new(net_data.raw());

                // ANALYTIC APPROACH: Store just the bounding box center point as a representative
                // Via checks will use ContactMetadata bounding boxes directly (no voxel sampling)
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

                    // eprintln!($3"[DEBUG DRC] Added via '{}' as analytic primitive (center: {:.3}mm, {:.3}mm, {:.3}mm)",
                    // contact.name,
                    // center.x as f64 / 1_000_000.0,
                    // center.y as f64 / 1_000_000.0,
                    //        center.z as f64 / 1_000_000.0
                    //   );
                } else {
                    // eprintln!($3"[DEBUG DRC] Warning: Contact '{}' has no bounding box", contact.name);
                }
            } else {
                // eprintln!($3"[DEBUG DRC] Warning: Contact '{}' references unknown net '{}'",
                // contact.name, net_name);
            }
        } else {
            // eprintln!($3"[DEBUG DRC] Warning: Contact '{}' has no net assignment", contact.name);
        }
    }

    let _drc_end = start_time.elapsed();
    // eprintln!($3"[DEBUG] DRC voxel iteration complete at {:.2}ms (took {:.2}ms)",
    // drc_end.as_secs_f64() * 1000.0,
    //   (drc_end - drc_start).as_secs_f64() * 1000.0
    // );

    // eprintln!($3"[DEBUG DRC] Converting {} nets to NetVoxels format", current_net_voxels.len());

    // Convert to NetVoxels
    let nets: Vec<NetVoxels> = current_net_voxels
        .into_iter()
        .map(|(net_id, voxels)| {
            // eprintln!($3"[DEBUG DRC] Converting net {} with {} voxels", net_id.raw(), voxels.len());

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

            // eprintln!($3"[DEBUG DRC] Net '{}' geometry type: {:?}", net_name, geometry_type);

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

    // eprintln!($3"[DEBUG DRC] NetVoxels conversion complete");

    if config.verbose {
        println!(
            "   Found {} nets with {} total voxels",
            nets.len(),
            nets.iter().map(|n| n.voxels.len()).sum::<usize>()
        );
    }

    // eprintln!($3"[DEBUG DRC] Creating constraint rulebook");

    // Create constraint rulebook
    let mut constraint_rulebook = ConstraintRulebook::new(space.voxel_size.x_nm);

    // eprintln!($3"[DEBUG DRC] Loading fabrication constraints from profile");

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
            high_voltage_clearance_nm: Some(constraints.clearance.high_voltage_nm),
            safety_factor: constraints.clearance.safety_factor,
            stackup,
        };

        constraint_rulebook.set_fabrication_constraints(fab_constraints);

        // Load thermal constraints from profile (v0.1.6: Thermal Integration)
        if let Some(ref thermal) = constraints.thermal {
            constraint_rulebook.max_temp_rise_c = Some(thermal.max_temp_rise_c);
            // Note: default_current_ma is not in profile yet, keep the default (20mA)

            if config.verbose {
                println!("     Thermal: max temp rise {}°C", thermal.max_temp_rise_c);
            }
        } else {
            // No thermal constraints in profile - use safe defaults
            // These match IPC-2221 recommendations for typical PCB operation
            constraint_rulebook.default_current_ma = Some(20); // 20mA for signal traces
            constraint_rulebook.max_temp_rise_c = Some(10.0); // 10°C rise (conservative)

            if config.verbose {
                println!("     Thermal: using defaults (20mA, 10°C rise)");
            }
        }

        if config.verbose {
            println!(
                "   Loaded fabrication constraints from profile '{}'",
                constraints.name
            );
            println!(
                "     Min via diameter: {}µm",
                constraints.via.min_diameter_nm / 1000
            );
            println!(
                "     Min annular ring: {}µm",
                constraints.via.min_annular_ring_nm / 1000
            );
            if let Some(ref s) = constraints.stackup {
                println!(
                    "     Stackup: {} (εr={:.2}), dielectric height: {}µm",
                    s.dielectric_material,
                    s.relative_permittivity,
                    s.dielectric_height_nm / 1000
                );
            }
        }
    }

    // eprintln!($3"[DEBUG DRC] Constraint rulebook configured");
    // eprintln!($3"[DEBUG DRC] Running DRC checks on {} nets", nets.len());

    // Run DRC
    let drc_checker = DesignRuleChecker::default();

    // eprintln!($3"[DEBUG DRC] DRC checker created, calling check()...");
    let mut drc_report = drc_checker.check(&nets, &constraint_rulebook, space.voxel_size.x_nm);

    // eprintln!($3"[DEBUG DRC] DRC check complete");

    // Task 4.2: Run analytic via checks (Primitives Over Pixels)
    // eprintln!($3"[DEBUG DRC] Running analytic via diameter check...");
    let via_diameter_report = hwc_engine::design_rule_check::validate_via_diameters_analytic(
        &space.contacts,
        &constraint_rulebook,
    );
    // eprintln!($3"[DEBUG DRC] Via diameter check complete: {} violations", via_diameter_report.violations.len());

    // Merge via diameter violations into main report
    for violation in via_diameter_report.violations {
        drc_report.add_violation(violation);
    }

    // eprintln!($3"[DEBUG DRC] Running analytic via enclosure check...");
    let substrate_layers = space.voxel_grid.get_substrate_layers();
    let via_enclosure_report = hwc_engine::design_rule_check::validate_via_enclosure_analytic(
        &space.contacts,
        substrate_layers,
        &constraint_rulebook,
        &space.netlist,
        &space.material_registry,
    );
    // eprintln!($3"[DEBUG DRC] Via enclosure check complete: {} violations", via_enclosure_report.violations.len());

    // Merge via enclosure violations into main report
    for violation in via_enclosure_report.violations {
        drc_report.add_violation(violation);
    }

    // v0.1.7: Run God-Tier Physics Validator (Bit-Parallel)
    // This handles substrate shorts and Keep-Out Zone (KOZ) violations
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

    // eprintln!($3"[DEBUG DRC] All DRC checks complete (including analytic via checks)");

    if !drc_report.is_valid() {
        println!("\n❌ DRC VIOLATIONS DETECTED:");
        
        // Group violations by type for cleaner output
        use rustc_hash::FxHashMap;
        let mut grouped: FxHashMap<String, Vec<String>> = FxHashMap::default();
        
        for violation in &drc_report.violations {
            let violation_str = violation.to_string();
            // Extract violation type (everything before " at ")
            let violation_type = if let Some(pos) = violation_str.find(" at ") {
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

/// Run connectivity check (Layer 2 and Layer 3)
fn run_connectivity_check(
    space: &HardwareSpace,
    config: &BuildConfig,
    start_time: Instant,
) -> Result<Vec<PhysicsError>> {
    // println!($3"[DEBUG] Starting connectivity pre-checks at {:.2}ms",
    //  start_time.elapsed().as_secs_f64() * 1000.0
    //);

    // Pre-check: Error P43 - Unassigned Conductor Detection
    check_unassigned_conductors(space)?;

    println!("🔌 Running Connectivity Check (Electrical Borrow Checker)...");
    // println!($3"[DEBUG] About to start connectivity check at {:.2}ms",
    //     start_time.elapsed().as_secs_f64() * 1000.0
    // );
    std::io::stdout().flush().unwrap();
    let connectivity_start = Instant::now();

    // Convert metadata to physics format
    let (physics_pours, physics_contacts, physics_substrate_layers) =
        convert_metadata_to_physics(space);

    if config.verbose {
        // println!($3"[DEBUG] Substrate layers for connectivity check: {}",
        //      physics_substrate_layers.len()
        //   );
        for _layer in physics_substrate_layers.iter() {
            // println!($3"[DEBUG]   Layer {}: net_id={}, net_name={:?}, bbox=({},{},{}) to ({},{},{}) [nm]",
            // idx,
            // layer.net,
            // layer.net_name,
            // layer.bbox.min_x,
            // layer.bbox.min_y,
            // layer.bbox.min_z,
            // layer.bbox.max_x,
            // layer.bbox.max_y,
            //       layer.bbox.max_z
            //  );
        }
    }

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
fn check_unassigned_conductors(space: &HardwareSpace) -> Result<()> {
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

/// Convert engine metadata to physics format
fn convert_metadata_to_physics(
    space: &HardwareSpace,
) -> (
    Vec<hwc_physics::connectivity::PourMetadata>,
    Vec<hwc_physics::connectivity::ContactMetadata>,
    Vec<hwc_physics::connectivity::SubstrateLayerMetadata>,
) {
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
            bbox: contact
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

    let physics_substrate_layers: Vec<hwc_physics::connectivity::SubstrateLayerMetadata> = space
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
                hwc_engine::voxel_grid::SubstrateLayerShape::Cylinder { diameter, .. } => {
                    hwc_physics::connectivity::SubstrateLayerShapeMetadata::Cylinder { diameter }
                }
                hwc_engine::voxel_grid::SubstrateLayerShape::Tube {
                    outer_diameter,
                    inner_diameter,
                    ..
                } => hwc_physics::connectivity::SubstrateLayerShapeMetadata::Tube {
                    outer_diameter,
                    inner_diameter,
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
                        hwc_engine::voxel_grid::SubstrateLayerShape::Cylinder { diameter, .. } => {
                            hwc_physics::connectivity::SubstrateLayerShapeMetadata::Cylinder {
                                diameter,
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

    (physics_pours, physics_contacts, physics_substrate_layers)
}

/// Print connectivity violations
fn print_connectivity_violations(violations: &[hwc_physics::connectivity::ConnectivityViolation]) {
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

/// Run Layer 3: Physical Continuity Validation
fn run_physical_continuity_check(
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
    // If module has `pins: []`, skip P43 (this is a test/utility space without real components)
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
            errors.push(hwc_physics::error_mapping::physical_continuity_to_error(violation));
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
fn extract_pin_positions(
    space: &HardwareSpace,
    config: &BuildConfig,
) -> Vec<hwc_physics::PinPosition> {
    let mut pin_positions = Vec::new();

    // v0.1.6: Get component pins directly from voxel grid
    let component_pins = space.voxel_grid.get_component_pins();

    if config.verbose {
        println!(
            "  Extracting {} component pins for P43 validation",
            component_pins.len()
        );
    }

    // Convert ComponentPin to PinPosition format
    // We use a simple hash to generate unique IDs from component and pin names
    for (idx, pin) in component_pins.iter().enumerate() {
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

        if config.verbose && idx < 10 {
            println!(
                "    Pin {}: {}.{} at ({:.3}mm, {:.3}mm, {:.3}mm)",
                idx,
                pin.component_name,
                pin.pin_name,
                pin.x_nm as f64 / 1_000_000.0,
                pin.y_nm as f64 / 1_000_000.0,
                pin.z_nm as f64 / 1_000_000.0
            );
        }
    }

    if config.verbose && component_pins.len() > 10 {
        println!("    ... and {} more pins", component_pins.len() - 10);
    }

    if config.verbose {
        println!(
            "  Extracted {} real component pin positions for P43 detection",
            pin_positions.len()
        );
    }

    pin_positions
}

/// Print physical continuity violations
fn print_continuity_violations(violations: &[hwc_physics::PhysicalContinuityViolation]) {
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
