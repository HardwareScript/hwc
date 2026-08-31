//! Generates the reusable SPICE circuit body (DUT) shared by all variants.
//!
//! Architecture 2 (Metadata-Driven Model References):
//! - Profile `models {}` section → `.include` directives (foundry model files)
//! - Device `spice {}` section → typed instance cards (prefix, model_name, W, L)
//! - ZERO hardcoded foundry strings or SPICE subcircuit logic in this crate.

use compact_str::CompactString;
use super::types::{PhysicalNetlist, PhysicalNetlistGraph};
use hwc_compiler::SymbolTable;
use hwc_engine::HardwareSpace;
use rustc_hash::FxHashSet;

use super::extracted_devices::{emit_extracted_devices, emit_parasitics};

/// Format a Z coordinate with appropriate unit selection for maximum precision.
///
/// **Unit Selection Rules (v0.2.2 - External Audit Precision Fix):**
/// - Values >= 1000nm: Format as micrometers (µm) with up to 6 decimal places
/// - Values < 1000nm: Format as nanometers (nm) as integer
///
/// This avoids precision loss from rounding (e.g., 380nm → 0.38µm exact, not 0.0004mm).
fn format_z_coordinate(z_nm: i64) -> String {
    if z_nm.abs() >= 1000 {
        // Format as micrometers for readability when >= 1µm
        let um = z_nm as f64 / 1000.0;
        let formatted = format!("{:.6}", um);
        // Trim trailing zeros and decimal point if no fractional part
        let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
        format!("{}µm", trimmed)
    } else {
        // Keep as nanometers for sub-micron precision
        format!("{}nm", z_nm)
    }
}

/// Generate the circuit body (devices and nets) - reused by all SPICE variants
pub fn generate_circuit_body(
    space: &HardwareSpace,
    symbol_table: &SymbolTable,
    physical_netlist: Option<&PhysicalNetlist>,
    physical_graph: &PhysicalNetlistGraph,
    space_def: Option<&hwc_parser::SpaceDefinition>,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut netlist_str = String::new();

    // **Stage 1: PDK Model Includes**
    // Reads `models { "path/to/model.spice" }` from the profile and emits
    // `.include` directives. The SPICE simulator resolves the actual models.
    // ZERO foundry-specific strings live in Rust.
    emit_pdk_model_includes(&mut netlist_str, space_def, symbol_table);

    // Emit nets as comments for reference
    emit_net_comments(&mut netlist_str, space);

    // Emit components as SPICE devices
    // If a physical netlist is present, skip schematic-level components to avoid
    // conflicting with extracted M devices (prevents LTspice subcircuit conflicts)
    let is_physical_mode = physical_netlist.is_some();
    emit_components(&mut netlist_str, space, symbol_table, is_physical_mode)?;

    // GAP 7 Phase 4: EXTRACTED DEVICES (Intent-Based Atom Architecture)
    // Devices are extracted from explicit device: bindings during alignment validation
    // This section outputs devices using their SPICE metadata from device definitions
    if let Some(netlist) = physical_netlist {
        emit_extracted_devices(&mut netlist_str, netlist, symbol_table, physical_graph)?;

        // Emit parasitics integrated into the netlist
        emit_parasitics(&mut netlist_str, physical_graph);

        // Emit 0V net bridges connecting stimulus/space nets to physical trace endpoints
        emit_net_bridges(&mut netlist_str, space, physical_graph, space_def);
    } else {
        // No physical netlist available (Artist Mode or no device bindings)
        println!("   ├─ Device extraction skipped (requires module with explicit bindings)");
        println!("   ├─ Use 'device: DeviceName.terminal' to bind pours to devices");
        netlist_str.push_str("* ========================================\n");
        netlist_str.push_str("* DEVICE EXTRACTION REQUIRES EXPLICIT BINDINGS\n");
        netlist_str.push_str("* Use 'device: DeviceName.terminal' property\n");
        netlist_str.push_str("* ========================================\n\n");
    }

    Ok(netlist_str)
}

/// Emit bridging resistors connecting stimulus net names (e.g. In, Out, GND)
/// to their corresponding physical trace endpoint nodes when they differ and are NOT already connected via parasitics.
fn emit_net_bridges(
    netlist_str: &mut String,
    space: &HardwareSpace,
    physical_graph: &PhysicalNetlistGraph,
    space_def: Option<&hwc_parser::SpaceDefinition>,
) {
    let mut bridges_to_emit: Vec<(String, String)> = Vec::new();

    // Collect from net_entry_points in physical graph
    for (net_name, entry_node) in &physical_graph.net_entry_points {
        if net_name != entry_node && !bridges_to_emit.iter().any(|(n, _)| n == net_name) {
            bridges_to_emit.push((net_name.clone(), entry_node.clone()));
        }
    }

    // Also check space pours and nets to ensure all declared nets are connected
    for pour in &space.pours {
        if let Some(ref name) = pour.net {
            if !bridges_to_emit.iter().any(|(n, _)| n == name.as_str()) {
                if let Some(entry_node) = physical_graph.net_entry_points.get(name.as_str()) {
                    if name.as_str() != entry_node {
                        bridges_to_emit.push((name.to_string(), entry_node.clone()));
                    }
                }
            }
        }
    }

    if let Some(space_def) = space_def {
        for net_decl in &space_def.nets {
            let name = net_decl.name.to_string();
            if let Some(entry_node) = physical_graph.net_entry_points.get(&name) {
                if &name != entry_node && !bridges_to_emit.iter().any(|(n, _)| n == &name) {
                    bridges_to_emit.push((name, entry_node.clone()));
                }
            }
        }
    }

    // Filter out bridges where the net_name and entry_node are already connected through parasitic resistors
    bridges_to_emit.retain(|(net_name, entry_node)| {
        let directly_connected = physical_graph.parasitics.iter().any(|p| match p {
            crate::netlist::types::ParasiticElement::TraceResistor { node_a, node_b, .. } => {
                (node_a == net_name && node_b == entry_node) || (node_b == net_name && node_a == entry_node)
            }
            _ => false,
        });
        let net_in_parasitics = physical_graph.parasitics.iter().any(|p| match p {
            crate::netlist::types::ParasiticElement::TraceResistor { node_a, node_b, .. } => {
                node_a == net_name || node_b == net_name
            }
            _ => false,
        });
        !directly_connected && !net_in_parasitics
    });

    if bridges_to_emit.is_empty() {
        return;
    }

    netlist_str.push_str("* ========================================\n");
    netlist_str.push_str("* TOP-LEVEL NET BRIDGES (STIMULUS TO PHYSICAL TRACES)\n");
    netlist_str.push_str("* ========================================\n");
    for (net_name, entry_node) in bridges_to_emit {
        netlist_str.push_str(&format!(
            "Rbridge_{} {} {} 1.000000e-4\n",
            net_name, net_name, entry_node
        ));
    }
    netlist_str.push('\n');
}

/// Emit `.include` directives from the `models {}` section of the space's profile.
///
/// Architecture 2 (Metadata-Driven Model References): the profile owns the list
/// of foundry model files; the Rust compiler merely formats `.include` lines.
/// ZERO foundry-specific strings or SPICE subcircuit logic lives in this crate.
///
/// Example profile syntax:
/// ```text
/// models {
///     "sky130_fd_pr/models/sky130_fd_pr__res_high_po.model.spice"
///     "sky130_fd_pr/models/sky130_fd_pr__nfet_01v8.model.spice"
/// }
/// ```
fn emit_pdk_model_includes(
    netlist_str: &mut String,
    space_def: Option<&hwc_parser::SpaceDefinition>,
    symbol_table: &SymbolTable,
) {
    // Resolve the profile attached to this space
    let profile = space_def
        .and_then(|sd| sd.profile.as_ref())
        .and_then(|pname| symbol_table.get_profile(pname.as_str()).ok());

    let Some(profile) = profile else {
        return; // No profile → no model includes (valid for PCB / discrete designs)
    };

    // Find the `models { ... }` section in the profile
    let Some(models_sec) = profile.sections.iter().find(|s| s.section_type == "models") else {
        return; // Profile has no models section → skip (not all profiles need PDK models)
    };

    // Collect model paths: each field value that is a StringLiteral is a model path.
    // The field *name* is the positional index generated by the parser for list items.
    let model_paths: Vec<&str> = models_sec
        .fields
        .iter()
        .filter_map(|(_, expr)| {
            if let hwc_parser::ast::Expression::StringLiteral { value, .. } = expr {
                Some(value.as_str())
            } else {
                None
            }
        })
        .collect();

    if model_paths.is_empty() {
        return;
    }

    netlist_str.push_str("* ========================================\n");
    netlist_str.push_str("* FOUNDRY PDK MODEL INCLUDES\n");
    netlist_str.push_str("* ========================================\n");
    for path in model_paths {
        netlist_str.push_str(&format!(".include \"{}\"\n", path));
    }
    netlist_str.push('\n');
}

/// Emit nets as reference comments (pours + netlist arena connections).
fn emit_net_comments(netlist_str: &mut String, space: &HardwareSpace) {
    // Collect nets from pours
    let mut pour_nets: FxHashSet<CompactString> = FxHashSet::default();
    for pour in &space.pours {
        if let Some(ref net_name) = pour.net {
            pour_nets.insert(net_name.clone());
        }
    }

    let net_count = space.netlist.num_nets();
    let total_nets = net_count + pour_nets.len();

    if total_nets > 0 {
        netlist_str.push_str("* ========================================\n");
        netlist_str.push_str("* NETS\n");
        netlist_str.push_str("* ========================================\n");

        // Emit nets from pours (silicon regions, metal layers)
        // Group merged regions together for parasitic extraction
        let mut merged_regions: rustc_hash::FxHashMap<
            CompactString,
            Vec<&hwc_engine::space::PourMetadata>,
        > = rustc_hash::FxHashMap::default();
        let mut standalone_pours = Vec::new();

        for pour in &space.pours {
            if let Some(ref merged_id) = pour.merged_region_id {
                merged_regions
                    .entry(merged_id.clone())
                    .or_default()
                    .push(pour);
            } else {
                standalone_pours.push(pour);
            }
        }

        // Emit merged regions (treat as single electrical node)
        for (merged_id, pours) in &merged_regions {
            if let Some(first_pour) = pours.first() {
                if let Some(ref net_name) = first_pour.net {
                    // Calculate total area for merged region
                    let total_area: i64 = pours.iter().map(|p| p.area_nm2).sum();

                    netlist_str.push_str(&format!(
                        "* Net: {} (merged region: {}, {} instances, total area: {} nm², material: {}, z: {})\n",
                        net_name,
                        merged_id,
                        pours.len(),
                        total_area,
                        first_pour.material_name,
                        format_z_coordinate(first_pour.z_bottom_nm)
                    ));
                    netlist_str
                        .push_str("*   Parasitic extraction: Treat as single electrical node\n");
                }
            }
        }

        // Emit standalone pours
        for pour in standalone_pours {
            if let Some(ref net_name) = pour.net {
                netlist_str.push_str(&format!(
                    "* Net: {} (pour: {}, material: {}, z: {})\n",
                    net_name,
                    pour.name,
                    pour.material_name,
                    format_z_coordinate(pour.z_bottom_nm)
                ));
            }
        }

        // Emit nets from netlist arena (discrete component connections)
        for net_id in space.netlist.all_net_ids() {
            if let Some(net) = space.netlist.get_net(net_id) {
                let material_name = space
                    .material_registry
                    .get_name(net.material)
                    .unwrap_or("Unknown");
                netlist_str.push_str(&format!(
                    "* Net: {} (width={}nm, material={})\n",
                    net.name, net.width_nm, material_name
                ));

                // List connected pins
                if !net.pins.is_empty() {
                    let mut valid_pins = Vec::new();
                    for pin_id in &net.pins {
                        if let Some(pin) = space.netlist.get_pin(*pin_id) {
                            if let Some(comp) = space.netlist.get_component(pin.parent_component) {
                                if !comp.name.is_empty()
                                    && !pin.name.is_empty()
                                    && !pin.name.starts_with("__virtual_")
                                    && pin.name != "anchor"
                                {
                                    valid_pins.push(format!("{}.{}", comp.name, pin.name));
                                }
                            }
                        }
                    }
                    if !valid_pins.is_empty() {
                        netlist_str.push_str("*   Connected pins:\n");
                        for p in valid_pins {
                            netlist_str.push_str(&format!("*     - {}\n", p));
                        }
                    }
                }
            }
        }
        netlist_str.push('\n');
    }
}

/// Emit schematic-level components from space.netlist as SPICE subcircuits
fn emit_components(
    netlist_str: &mut String,
    space: &HardwareSpace,
    _symbol_table: &SymbolTable,
    is_physical_mode: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if is_physical_mode {
        return Ok(());
    }

    if !space.device_instances.is_empty() {
        netlist_str.push_str("* ========================================\n");
        netlist_str.push_str("* DEVICE INSTANCES\n");
        netlist_str.push_str("* ========================================\n");
        for dev in &space.device_instances {
            netlist_str.push('X');
            netlist_str.push_str(&dev.name);
            for term in &dev.terminals {
                let net = dev.terminal_nets.get(term).map(|s| s.as_str()).unwrap_or("0");
                netlist_str.push(' ');
                netlist_str.push_str(net);
            }
            netlist_str.push(' ');
            netlist_str.push_str(&dev.device_type);
            for (k, v) in &dev.parameters {
                netlist_str.push_str(&format!(" {}={}", k, v));
            }
            netlist_str.push('\n');
        }
    }

    let component_count = space.netlist.component_count();
    if component_count == 0 {
        return Ok(());
    }

    netlist_str.push_str("* ========================================\n");
    netlist_str.push_str("* COMPONENTS (Schematic-Level Subcircuits)\n");
    netlist_str.push_str("* ========================================\n");

    for i in 0..component_count {
        let comp_id = hwc_engine::netlist::ComponentId::new(i as u32);
        if let Some(component) = space.netlist.get_component(comp_id) {
            let pins = space.netlist.get_component_pins(comp_id);

            let mut pin_net_map = rustc_hash::FxHashMap::default();
            for pin_id in &pins {
                if let Some(pin) = space.netlist.get_pin(*pin_id) {
                    let entity_graph_net = space
                        .entity_graph
                        .get_component_pins()
                        .iter()
                        .find(|vp| vp.component_name == component.name && vp.pin_name == pin.name)
                        .and_then(|vp| vp.net.clone());

                    if let Some(net_name) = entity_graph_net {
                        pin_net_map.insert(pin.name.to_string(), net_name.to_string());
                    } else if let Some(net_id) = pin.connected_net {
                        if let Some(net) = space.netlist.get_net(net_id) {
                            pin_net_map.insert(pin.name.to_string(), net.name.to_string());
                        }
                    }
                }
            }

            netlist_str.push('X');
            netlist_str.push_str(&component.name);
            for pin_id in &pins {
                if let Some(pin) = space.netlist.get_pin(*pin_id) {
                    let net = pin_net_map.get(pin.name.as_str()).map(|s| s.as_str()).unwrap_or("0");
                    netlist_str.push(' ');
                    netlist_str.push_str(net);
                }
            }
            netlist_str.push(' ');
            netlist_str.push_str(&component.component_type);
            netlist_str.push('\n');
        }
    }
    netlist_str.push('\n');
    Ok(())
}
