//! Parasitic extraction: build the physical netlist graph from routed traces.
//!
//! Extracts trace resistance (R) and ground capacitance (C) from analytic routes,
//! and via/contact resistance from contact metadata.

use rustc_hash::FxHashMap;

use super::types::{ParasiticElement, PhysicalNetlistGraph};
use hwc_compiler::alignment::PhysicalNetlist;
use hwc_compiler::SymbolTable;
use hwc_engine::HardwareSpace;

/// Extract trace parasitics (R, C, L) from routed segments and build physical netlist graph
pub fn build_physical_netlist_graph(
    space: &HardwareSpace,
    _symbol_table: &SymbolTable,
    physical_netlist: Option<&PhysicalNetlist>,
    substrate_net: &str,
) -> Result<PhysicalNetlistGraph, Box<dyn std::error::Error>> {
    let mut graph = PhysicalNetlistGraph::new();

    eprintln!(
        "[NETLIST PARASITIC DEBUG] analytic_routes.len() = {}",
        space.analytic_routes.len()
    );

    if space.analytic_routes.is_empty() {
        eprintln!("[NETLIST PARASITIC DEBUG] No analytic routes - skipping parasitic extraction");
        // No routing - use logical net names directly
        if let Some(netlist) = physical_netlist {
            for device in &netlist.devices {
                for (terminal, net_name) in &device.terminals {
                    let key = (device.name.to_string(), terminal.to_string());
                    graph.device_nodes.insert(key, net_name.clone());
                }
            }
        }
        return Ok(graph);
    }

    eprintln!(
        "[NETLIST PARASITIC DEBUG] Found {} analytic routes - proceeding with parasitic extraction",
        space.analytic_routes.len()
    );
    
    eprintln!(
        "[NETLIST PARASITIC DEBUG] Found {} contacts for via resistance extraction",
        space.contacts.len()
    );

    // Physical constants
    const EPS_0: f64 = 8.854187817e-12; // F/m

    // Build net-to-segments mapping
    let mut net_segments: FxHashMap<String, Vec<(usize, usize)>> = FxHashMap::default();
    for (trace_idx, trace) in space.analytic_routes.iter().enumerate() {
        for seg_idx in 0..trace.segments.len() {
            net_segments
                .entry(trace.net_name.to_string())
                .or_default()
                .push((trace_idx, seg_idx));
        }
    }

    eprintln!(
        "[NETLIST PARASITIC DEBUG] Built net_segments map with {} nets",
        net_segments.len()
    );

    // For each net with routing, create physical node chain
    for (net_name, segments) in &net_segments {
        eprintln!(
            "[NETLIST PARASITIC DEBUG] Processing net '{}' with {} segments",
            net_name,
            segments.len()
        );

        if segments.is_empty() {
            graph
                .net_entry_points
                .insert(net_name.clone(), net_name.clone());
            continue;
        }

        // Entry point is the first segment start node
        let entry_node = format!("n{}_entry", net_name);
        graph
            .net_entry_points
            .insert(net_name.clone(), entry_node.clone());

        let mut prev_node = entry_node;

        // Extract parasitics for each segment
        for (seg_num, &(trace_idx, seg_idx)) in segments.iter().enumerate() {
            let trace = &space.analytic_routes[trace_idx];
            let segment = &trace.segments[seg_idx];

            // **Stage 1 Node Topology:**
            // Entry point → Trace R → Logical net name
            // The final node for this net's parasitic chain is the logical net name itself
            let node_end = net_name.clone();

            // Extract resistance
            if let Some(material_props) = space.material_registry.get_physical_props(trace.material)
            {
                eprintln!(
                    "[NETLIST PARASITIC DEBUG] Net '{}' seg {}: Found material properties",
                    net_name, seg_num
                );

                if let Some(resistivity) = material_props.get("resistivity") {
                    eprintln!(
                        "[NETLIST PARASITIC DEBUG] Net '{}' seg {}: resistivity = {}",
                        net_name, seg_num, resistivity
                    );

                    let thickness_nm = trace.cross_section.thickness_nm;
                    let width_nm = trace.cross_section.width_nm;

                    let dx = (segment.end.x - segment.start.x) as f64;
                    let dy = (segment.end.y - segment.start.y) as f64;
                    let dz = (segment.end.z - segment.start.z) as f64;
                    let length_m = ((dx * dx + dy * dy + dz * dz).sqrt()) * 1e-9;

                    let thickness_m = thickness_nm as f64 * 1e-9;
                    let width_m = width_nm as f64 * 1e-9;
                    let cross_section_m2 = width_m * thickness_m;

                    if cross_section_m2 <= 0.0 {
                        return Err(format!("Invalid cross-section for net '{}'", net_name).into());
                    }

                    let resistance_ohm = resistivity * (length_m / cross_section_m2);

                    eprintln!(
                        "[NETLIST PARASITIC DEBUG] Net '{}' seg {}: R = {}Ω (length={}m, area={}m²)",
                        net_name, seg_num, resistance_ohm, length_m, cross_section_m2
                    );

                    if resistance_ohm > 0.001 {
                        eprintln!(
                            "[NETLIST PARASITIC DEBUG] Net '{}' seg {}: Adding trace resistor",
                            net_name, seg_num
                        );
                        graph.parasitics.push(ParasiticElement::TraceResistor {
                            name: format!("Rtr_{}_{}", net_name, seg_num),
                            node_a: prev_node.clone(),
                            node_b: node_end.clone(),
                            value_ohms: resistance_ohm,
                        });
                    } else {
                        eprintln!(
                            "[NETLIST PARASITIC DEBUG] Net '{}' seg {}: Resistance too small, skipping",
                            net_name, seg_num
                        );
                    }

                    // Extract ground capacitance for horizontal traces
                    if dz.abs() < 1.0 {
                        // Find the dielectric layer beneath this trace to get accurate substrate height and permittivity
                        let trace_z = segment.start.z as f64; // Use starting Z coordinate

                        // Require stackup layers to be defined
                        if space.stackup_layers.is_empty() {
                            return Err(format!(
                                "SPICE parasitic extraction requires stackup definition for net '{}'",
                                net_name
                            )
                            .into());
                        }

                        // Search stackup for dielectric layer below trace
                        // Strategy: Find the closest insulator layer with Z_top <= trace_z
                        let mut found_dielectric: Option<(f64, f64)> = None;
                        let mut min_distance = f64::MAX;

                        for layer in &space.stackup_layers {
                            let _layer_z_bottom = layer.z_bottom as f64;
                            let layer_z_top = layer.z_top as f64;

                            // Only consider dielectric layers that are completely below the trace
                            if layer_z_top < trace_z {
                                let mat_id = space.material_registry.get_id(&layer.material_name).ok_or_else(|| {
                                    format!(
                                        "Material '{}' used in stackup layer '{}' not found in registry",
                                        layer.material_name, layer.name
                                    )
                                })?;

                                if space.material_registry.is_insulator(mat_id) {
                                    let distance = trace_z - layer_z_top;
                                    if distance < min_distance {
                                        // Get thickness and permittivity from this layer
                                        let thickness = (layer.z_top - layer.z_bottom) as f64;

                                        let material_props = space
                                            .material_registry
                                            .get_physical_props(mat_id)
                                            .ok_or_else(|| {
                                                format!(
                                                    "Dielectric material '{}' has no physical properties defined",
                                                    layer.material_name
                                                )
                                            })?;

                                        let permittivity = material_props
                                            .get("relative_permittivity")
                                            .ok_or_else(|| {
                                                format!(
                                                    "Dielectric material '{}' missing 'relative_permittivity' property.\n\
                                                     Add to material definition:\n\
                                                     properties:\n    relative_permittivity: 3.9",
                                                    layer.material_name
                                                )
                                            })?;

                                        found_dielectric = Some((thickness, permittivity));
                                        min_distance = distance;
                                    }
                                }
                            }
                        }

                        // Only extract ground capacitance if there's a dielectric layer below
                        // (bottom-layer traces have no ground plane below them)
                        if let Some((substrate_height_nm, epsilon_r)) = found_dielectric {
                            let substrate_height_m = substrate_height_nm * 1e-9;
                            let area_m2 = width_m * length_m;
                            let capacitance_f = EPS_0 * epsilon_r * (area_m2 / substrate_height_m);

                            if capacitance_f > 1e-17 {
                                // Phase 1b Item 3: Dynamic Substrate/Bulk Net Mapping
                                // Use the substrate_net declared in the profile
                                // NO FALLBACK - profile MUST declare substrate_net
                                graph.parasitics.push(ParasiticElement::GroundCapacitance {
                                    name: format!("Cgnd_{}_{}", net_name, seg_num),
                                    node: node_end.clone(),
                                    ref_node: substrate_net.to_string(),
                                    value_farads: capacitance_f,
                                });
                            }
                        }
                        // If no dielectric below, silently skip ground capacitance extraction
                        // This is correct behavior for bottom-layer routing
                    }
                }
            }

            prev_node = node_end;
        }

        // Map all devices on this net to the LOGICAL NET NAME
        // Devices connect to the logical net (e.g., "In", "Out"),
        // NOT to intermediate parasitic nodes (e.g., "nIn_dev").
        // This ensures clean SPICE output: RR1 In Out 1400.00
        if let Some(netlist) = physical_netlist {
            eprintln!(
                "[NETLIST PARASITIC DEBUG] Net '{}': Mapping device terminals to logical net",
                net_name
            );
            for device in &netlist.devices {
                eprintln!(
                    "[NETLIST PARASITIC DEBUG]   Device '{}': terminals = {:?}",
                    device.name, device.terminals
                );
                for (terminal, terminal_net) in &device.terminals {
                    eprintln!(
                        "[NETLIST PARASITIC DEBUG]     Checking terminal '{}' -> '{}' (looking for net '{}')",
                        terminal, terminal_net, net_name
                    );
                    if terminal_net == net_name {
                        let key = (device.name.to_string(), terminal.to_string());
                        eprintln!(
                            "[NETLIST PARASITIC DEBUG]     ✓ MATCH! Adding device_nodes[{:?}] = '{}'",
                            key, net_name
                        );
                        // Use the logical net name, not the intermediate physical node
                        graph.device_nodes.insert(key, net_name.clone());
                    }
                }
            }
        }
    }

    // ========================================
    // Extract Via/Contact Resistance
    // ========================================
    // SkyWater 130nm typical values:
    // - licon (diffusion/poly to LI): ~10-30Ω per contact
    // - mcon (LI to Metal1): ~5-10Ω per contact
    // - Via1 (Metal1 to Metal2): ~4-6Ω per via
    //
    // For arrays of N vias in parallel: R_total = R_single / N
    eprintln!(
        "[NETLIST PARASITIC DEBUG] Extracting via/contact resistance from {} contacts",
        space.contacts.len()
    );

    // Group contacts by (net, from_layer, to_layer) to find via stacks
    // This groups all vias connecting the same two layers on the same net
    let mut via_stacks: FxHashMap<(String, String, String), Vec<&hwc_engine::space::ContactMetadata>> = FxHashMap::default();
    
    for contact in &space.contacts {
        if let Some(net_name) = &contact.net {
            if let (Some(from_layer), Some(to_layer)) = (&contact.from_layer, &contact.to_layer) {
                let key = (net_name.to_string(), from_layer.to_string(), to_layer.to_string());
                via_stacks.entry(key).or_default().push(contact);
            }
        }
    }

    eprintln!(
        "[NETLIST PARASITIC DEBUG] Found {} via stacks after grouping by (net, from_layer, to_layer)",
        via_stacks.len()
    );

    // Extract resistance for each via stack
    for ((net_name, from_layer, to_layer), contacts_in_stack) in via_stacks {
        let num_vias = contacts_in_stack.len();
        
        eprintln!(
            "[NETLIST PARASITIC DEBUG] Processing via stack on net '{}' ({} -> {}) with {} parallel vias",
            net_name, from_layer, to_layer, num_vias
        );

        // Get via material and lookup contact resistance
        if let Some(first_contact) = contacts_in_stack.first() {
            // Get material ID from material name
            if let Some(material_id) = space.material_registry.get_id(&first_contact.material_name) {
                if let Some(material_props) = space.material_registry.get_physical_props(material_id) {
                    if let Some(contact_resistance_ohm_cm2) = material_props.get("contact_resistance") {
                        // Calculate via area from drill diameter
                        if let Some(drill_diameter_nm) = first_contact.drill_diameter_nm {
                            let drill_radius_cm = (drill_diameter_nm as f64 * 1e-9 * 100.0) / 2.0; // nm to cm
                            let via_area_cm2 = std::f64::consts::PI * drill_radius_cm * drill_radius_cm;
                            
                            // Single via resistance: R = ρ_c / A (contact_resistance is ρ_c, specific contact resistivity)
                            let single_via_resistance = contact_resistance_ohm_cm2 / via_area_cm2;
                            
                            // Parallel via array: R_total = R_single / N
                            let total_via_resistance = single_via_resistance / (num_vias as f64);
                            
                            eprintln!(
                                "[NETLIST PARASITIC DEBUG] Via resistance calculation: material={}, diameter={}nm, area={:.3e}cm², ρ_c={:.3e}Ω·cm², R_single={:.3}Ω, R_parallel={:.3}Ω ({} vias)",
                                first_contact.material_name,
                                drill_diameter_nm,
                                via_area_cm2,
                                contact_resistance_ohm_cm2,
                                single_via_resistance,
                                total_via_resistance,
                                num_vias
                            );

                            // Only extract if resistance is significant (> 0.1Ω)
                            if total_via_resistance > 0.1 {
                                // Create unique via resistance name based on net and layer transition
                                let via_name = format!("via_{}_{}_{}", net_name, from_layer, to_layer);
                                
                                eprintln!(
                                    "[NETLIST PARASITIC DEBUG] Adding via resistor: {} = {:.3}Ω",
                                    via_name, total_via_resistance
                                );

                                // Via resistances are inserted in series with the net
                                // The resistor connects the logical net to itself (effectively in series with traces)
                                // SPICE topology: Entry → Trace_R → Net → Via_R → Net_post_via
                                graph.parasitics.push(ParasiticElement::TraceResistor {
                                    name: via_name.clone(),
                                    node_a: net_name.clone(),
                                    node_b: format!("{}_post_{}", net_name, via_name),
                                    value_ohms: total_via_resistance,
                                });
                            } else {
                                eprintln!(
                                    "[NETLIST PARASITIC DEBUG] Via resistance {:.3}Ω is negligible, skipping",
                                    total_via_resistance
                                );
                            }
                        } else {
                            eprintln!(
                                "[NETLIST PARASITIC DEBUG] Warning: Contact '{}' missing drill_diameter_nm, skipping via resistance",
                                first_contact.name
                            );
                        }
                    } else {
                        eprintln!(
                            "[NETLIST PARASITIC DEBUG] Material '{}' has no 'contact_resistance' property, skipping via resistance for net '{}'",
                            first_contact.material_name, net_name
                        );
                    }
                } else {
                    eprintln!(
                        "[NETLIST PARASITIC DEBUG] Material '{}' has no physical properties, skipping via resistance",
                        first_contact.material_name
                    );
                }
            } else {
                eprintln!(
                    "[NETLIST PARASITIC DEBUG] Material '{}' not found in registry",
                    first_contact.material_name
                );
            }
        }
    }

    Ok(graph)
}
