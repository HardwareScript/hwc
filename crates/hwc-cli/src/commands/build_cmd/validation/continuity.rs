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
        println!("\n🔍 Running PIVB Connectivity Validation...");
    }

    let mut conductive_material_ids = rustc_hash::FxHashSet::default();
    for (id, _name) in space.material_registry.all_materials() {
        if space.material_registry.is_conductor(id) || space.material_registry.is_semiconductor(id)
        {
            conductive_material_ids.insert(id);
        }
    }

    // Convert physics_route_segments to SubstrateLayerMetadata for PIVB Pass 1
    let mut all_substrate_layers = physics_substrate_layers.to_vec();
    for route in physics_route_segments {
        all_substrate_layers.push(hwc_physics::connectivity::SubstrateLayerMetadata {
            material: route.material,
            net: route.net,
            net_name: route.net_name.clone(),
            bbox: route.bbox,
            layer_type: hwc_physics::connectivity::SubstrateLayerType::Pour, // Routes are planar pours
        });
    }

    // Prepare ContactPlacement for PIVB Pass 2
    let mut contact_placements = Vec::new();

    // v0.1.8: Include vertical routes from all_substrate_layers (converted from analytic_routes)
    for layer in &all_substrate_layers {
        if layer.layer_type == hwc_physics::connectivity::SubstrateLayerType::Contact {
            contact_placements.push(hwc_physics::ContactPlacement {
                name: "via_resolved".into(),
                x: layer.bbox.center().x,
                y: layer.bbox.center().y,
                z_min: layer.bbox.min.z,
                z_max: layer.bbox.max.z,
                net_name: layer.net_name.as_ref().map(|n| n.clone().into()),
                material: layer.material,
                bbox: Some(layer.bbox),
            });
        }
    }

    for contact in &space.contacts {
        let material_id = space.material_registry.get_id(&contact.material_name);
        if let Some(id) = material_id {
            if conductive_material_ids.contains(&id) {
                if let Some(bbox) = &contact.bbox {
                    contact_placements.push(hwc_physics::ContactPlacement {
                        name: contact.name.clone(),
                        x: bbox.center().x,
                        y: bbox.center().y,
                        z_min: bbox.min.z,
                        z_max: bbox.max.z,
                        net_name: contact.net.clone(),
                        material: id,
                        bbox: Some(*bbox),
                    });
                }
            }
        }
    }

    let solver = hwc_physics::PivbSolver::new(
        &all_substrate_layers,
        &contact_placements,
        &conductive_material_ids,
    );

    let results = solver.validate();

    let mut errors = Vec::new();
    let mut failure_count = 0;

    for result in &results {
        match result {
            hwc_physics::ConnectivityResult::Fail(report) => {
                failure_count += 1;
                if config.verbose {
                    println!(
                        "  ❌ Net '{}' is fragmented into {} components",
                        report.net_name, report.component_count
                    );
                }
                errors.push(hwc_physics::error_mapping::pivb_to_error(report));
            }
            hwc_physics::ConnectivityResult::Pass { .. } => {}
        }
    }

    if config.verbose && failure_count == 0 {
        println!("✅ PIVB connectivity check passed - all nets are physically continuous");
    }

    println!(
        "[{:>8.2}ms] PIVB connectivity check completed in {:.2}ms",
        start_time.elapsed().as_secs_f64() * 1000.0,
        continuity_start.elapsed().as_secs_f64() * 1000.0
    );

    Ok(errors)
}
