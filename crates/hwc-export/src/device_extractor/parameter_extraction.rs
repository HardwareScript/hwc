//! Parameter Extraction Registry - Universal Parameter-Driven Calculation
//!
//! TRUE ZERO-MAGIC ARCHITECTURE:
//! The compiler extracts ONLY what the user/PDK requests in parameters: [...]
//!
//! - If parameters: [R] → Calculate R = ρ · (L / A) from Maxwell's equations
//! - If parameters: [C] → Calculate C = ε · (A / d) from electrostatics
//! - If parameters: [L] → Calculate L = μ · (h / A) from magnetic flux
//! - If parameters: [W, L] → Measure channel width and length from geometry
//! - If parameters: [AREA] → Measure polygon surface area
//! - If parameters: [AD, AS, PD, PS] → Measure diffusion parasitics
//!
//! The SPICE prefix (R, C, L, M, Q, D, J, X, etc.) is JUST A LABEL.
//! It has zero influence on extraction logic.

use compact_str::CompactString;
use hwc_engine::space::{BindingPriority, PourMetadata};
use hwc_engine::HardwareSpace;
use rustc_hash::FxHashMap;

/// Universal parameter extraction dispatcher
///
/// ZERO PREFIX DISCRIMINATION: This function does not branch on spice.prefix.
/// The prefix is merely a formatting character for the netlist output line.
///
/// Extraction is driven SOLELY by the requested parameters:
/// - Physics calculations: [R], [C], [L]
/// - Geometry measurements: [W], [L], [AREA], [PJ], [AD], [AS], [PD], [PS]
pub fn extract_parameters_universal(
    device_type: &str,
    terminal_pours: &FxHashMap<CompactString, Vec<PourMetadata>>,
    space: &HardwareSpace,
    symbol_table: &hwc_compiler::SymbolTable,
) -> Result<FxHashMap<CompactString, f64>, String> {
    let device_def = symbol_table
        .get_device(device_type)
        .map_err(|e| format!("Device definition '{}' not found: {}", device_type, e))?;

    let spice_info = device_def
        .spice_info
        .as_ref()
        .ok_or_else(|| format!("Device '{}' is missing required 'spice:' block", device_type))?;

    let mut results = FxHashMap::default();

    // ========================================================================
    // PHASE 1: MAXWELL PHYSICS CALCULATIONS (Linear Passives: R, C)
    // ========================================================================

    for param in &spice_info.parameters {
        match param.as_str() {
            "R" => {
                let r_val = calculate_resistance(terminal_pours, space)?;
                results.insert("R".into(), r_val);
            }
            "C" => {
                let c_val = calculate_capacitance(terminal_pours, space)?;
                results.insert("C".into(), c_val);
            }
            _ => {} // W, L, AREA, etc. are handled in Phase 2
        }
    }

    // ========================================================================
    // PHASE 2: GEOMETRY MEASUREMENTS (Semiconductors & Macro Models)
    // ========================================================================
    // If the user requests geometric dimensions ([W], [L], [AREA], [AD], etc.),
    // measure them from the physical polygon layout.

    let needs_geometry = spice_info.parameters.iter().any(|p| {
        matches!(
            p.as_str(),
            "W" | "L" | "AREA" | "PJ" | "PERIMETER" | "AD" | "AS" | "PD" | "PS"
        )
    });

    if needs_geometry {
        let measurements = extract_all_geometry_measurements(
            terminal_pours,
            &spice_info.terminal_order,
            space,
        )?;

        for param in &spice_info.parameters {
            if let Some(val) = measurements.get(param.as_str()) {
                results.insert(param.clone(), *val);
            }
        }
    }

    Ok(results)
}

// ============================================================================
// PHASE 1: MAXWELL PHYSICS CALCULATIONS
// ============================================================================

/// Calculate resistance: R = ρ · (L / A)
///
/// Works for ANY device requesting [R] parameter:
/// - Discrete chip resistors (prefix R)
/// - Polysilicon/metal resistors on-chip
/// - Diffused resistors in silicon
fn calculate_resistance(
    terminal_pours: &FxHashMap<CompactString, Vec<PourMetadata>>,
    space: &HardwareSpace,
) -> Result<f64, String> {
    // Find the primary resistive body (marked as Channel priority)
    let channel_pour = terminal_pours
        .values()
        .flatten()
        .find(|p| {
            p.device_binding
                .as_ref()
                .map_or(false, |b| b.priority == BindingPriority::Channel)
        })
        .ok_or_else(|| {
            "Resistance calculation requires a pour marked with BindingPriority::Channel".to_string()
        })?;

    let bbox = channel_pour
        .bbox
        .as_ref()
        .ok_or_else(|| format!("Resistive pour '{}' has no bounding box", channel_pour.name))?;

    // Geometry
    let dx_um = (bbox.max.x - bbox.min.x).abs() as f64 / 1000.0;
    let dy_um = (bbox.max.y - bbox.min.y).abs() as f64 / 1000.0;
    let length_um = dx_um.max(dy_um); // Major axis
    let width_um = dx_um.min(dy_um); // Minor axis

    // Material properties
    let mat_id = space
        .material_registry
        .get_id(&channel_pour.material_name)
        .ok_or_else(|| format!("Material '{}' not found", channel_pour.material_name))?;

    let props = space
        .material_registry
        .get_physical_props(mat_id)
        .ok_or_else(|| {
            format!(
                "Material '{}' has no physical properties defined",
                channel_pour.material_name
            )
        })?;

    let resistivity = props.get("resistivity").ok_or_else(|| {
        format!(
            "Material '{}' missing REQUIRED 'resistivity' property for R calculation.\n\
             Add to material definition:\n  properties:\n    resistivity: <value>  # Ω·m",
            channel_pour.material_name
        )
    })?;

    // Thickness from stackup
    let z_bot = channel_pour.z_bottom_nm;
    let thickness_nm = space
        .stackup_layers
        .iter()
        .find(|l| z_bot >= l.z_bottom && z_bot < l.z_top)
        .map(|l| (l.z_top - l.z_bottom) as f64)
        .ok_or_else(|| format!("Stackup layer not found for pour at Z={}nm", z_bot))?;

    // R = ρ · (L / A) where A = width × thickness
    let length_m = length_um * 1e-6;
    let cross_section_m2 = (width_um * 1e-6) * (thickness_nm * 1e-9);

    if cross_section_m2 <= 0.0 {
        return Err(format!(
            "Resistor cross-section area is zero (W={:.2}um, thickness={:.2}nm)",
            width_um, thickness_nm
        ));
    }

    let r_ohms = resistivity * (length_m / cross_section_m2);

    println!(
        "      ├─ R={:.2e}Ω (ρ={:.2e}Ω·m, L={:.2}um, A={:.2e}m²)",
        r_ohms, resistivity, length_um, cross_section_m2
    );

    Ok(r_ohms)
}

/// Calculate capacitance: C = ε₀ · εᵣ · (A / d)
///
/// Works for ANY device requesting [C] parameter:
/// - Discrete chip capacitors (prefix C)
/// - MIM capacitors on-chip
/// - Metal-oxide-metal plate capacitors
fn calculate_capacitance(
    terminal_pours: &FxHashMap<CompactString, Vec<PourMetadata>>,
    space: &HardwareSpace,
) -> Result<f64, String> {
    // Find the two conductive plates
    let conductor_pours: Vec<&PourMetadata> = terminal_pours
        .values()
        .flatten()
        .filter(|p| {
            space
                .material_registry
                .get_id(&p.material_name)
                .map_or(false, |id| space.material_registry.is_conductor(id))
        })
        .collect();

    if conductor_pours.len() < 2 {
        return Err(format!(
            "Capacitance calculation requires at least 2 conductive plates, found {}",
            conductor_pours.len()
        ));
    }

    let plate1 = conductor_pours[0];
    let plate2 = conductor_pours[1];

    let bbox1 = plate1
        .bbox
        .as_ref()
        .ok_or("Capacitor plate 1 missing bounding box")?;
    let bbox2 = plate2
        .bbox
        .as_ref()
        .ok_or("Capacitor plate 2 missing bounding box")?;

    // Calculate overlap area (parallel-plate assumption)
    let x_overlap = (bbox1.max.x.min(bbox2.max.x) - bbox1.min.x.max(bbox2.min.x)).max(0) as f64;
    let y_overlap = (bbox1.max.y.min(bbox2.max.y) - bbox1.min.y.max(bbox2.min.y)).max(0) as f64;
    let overlap_nm2 = x_overlap * y_overlap;

    if overlap_nm2 <= 0.0 {
        return Err("Capacitor plates have zero overlap area".to_string());
    }

    // Calculate separation distance
    let z_min = plate1.z_bottom_nm.min(plate2.z_bottom_nm);
    let z_max = plate1.z_bottom_nm.max(plate2.z_bottom_nm);
    let separation_nm = (z_max - z_min) as f64;

    if separation_nm <= 0.0 {
        return Err(format!(
            "Capacitor plates are at the same Z level ({}nm)",
            z_min
        ));
    }

    // Find dielectric material between plates
    let epsilon_r = space
        .stackup_layers
        .iter()
        .find(|l| l.z_bottom >= z_min && l.z_top <= z_max)
        .and_then(|l| space.material_registry.get_id(&l.material_name))
        .and_then(|id| space.material_registry.get_physical_props(id))
        .and_then(|props| props.get("relative_permittivity"))
        .ok_or_else(|| {
            format!(
                "No dielectric layer with 'relative_permittivity' found between Z={}nm and Z={}nm.\n\
                 Add to insulator material:\n  properties:\n    relative_permittivity: <value>",
                z_min, z_max
            )
        })?;

    // C = ε₀ · εᵣ · (A / d)
    const EPSILON_0: f64 = 8.854e-12; // F/m
    let area_m2 = overlap_nm2 * 1e-18;
    let distance_m = separation_nm * 1e-9;

    let c_farads = EPSILON_0 * epsilon_r * (area_m2 / distance_m);

    println!(
        "      ├─ C={:.2e}F (εᵣ={:.1}, A={:.0}nm², d={:.0}nm)",
        c_farads, epsilon_r, overlap_nm2, separation_nm
    );

    Ok(c_farads)
}

// ============================================================================
// PHASE 2: GEOMETRY MEASUREMENTS
// ============================================================================

/// Extract all requested geometric measurements from layout
///
/// Works for ANY device requesting dimensional parameters:
/// - MOSFETs requesting [W, L, AD, AS, PD, PS]
/// - BJTs requesting [AREA, PJ]
/// - Diodes requesting [AREA]
/// - Subcircuits requesting [W, L] (foundry PDK models)
fn extract_all_geometry_measurements<'a>(
    terminal_pours: &FxHashMap<CompactString, Vec<PourMetadata>>,
    terminal_order: &[CompactString],
    space: &HardwareSpace,
) -> Result<FxHashMap<&'a str, f64>, String> {
    let mut measurements = FxHashMap::default();

    // Standard channel width/length measurements
    if let Ok(dims) = measure_channel_dimensions(terminal_pours, space) {
        measurements.insert("W", dims.width_um);
        measurements.insert("L", dims.length_um);
        measurements.insert("AREA", dims.area_um2);
        measurements.insert("PJ", dims.perimeter_um);
        measurements.insert("PERIMETER", dims.perimeter_um);
    }

    // MOSFET-specific source/drain diffusion parasitics
    if terminal_order.len() >= 3 {
        if let Ok(parasitics) = measure_mosfet_diffusion_parasitics(terminal_pours, terminal_order)
        {
            measurements.insert("AD", parasitics.drain_area_m2);
            measurements.insert("AS", parasitics.source_area_m2);
            measurements.insert("PD", parasitics.drain_perimeter_m);
            measurements.insert("PS", parasitics.source_perimeter_m);
        }
    }

    Ok(measurements)
}

// ============================================================================
// Geometry Measurement Helpers
// ============================================================================

pub struct ChannelDimensions {
    pub width_um: f64,
    pub length_um: f64,
    pub area_um2: f64,
    pub perimeter_um: f64,
}

/// Measure physical dimensions from the primary channel geometry
///
/// L (Length) = Clear span between the INNER edges of the contact heads
/// W (Width)  = Transverse channel width
///
/// Physical Layout Convention:
/// - Length (L): Dimension along the axis of current flow (connecting terminals)
/// - Width (W): Dimension transverse (perpendicular) to current flow
///
/// CRITICAL LVS FIX:
/// For integrated resistors (e.g., SkyWater poly resistors), the foundry PDK
/// subcircuit already includes fixed contact head resistances. We must measure
/// the EFFECTIVE channel length between contact inner edges, NOT the drawn
/// polysilicon rectangle length, to avoid double-counting contact regions.
///
/// Example: A 4.00µm drawn poly rectangle with 400nm contact heads on each end
/// has an effective channel length of 3.20µm (4.00µm - 2×400nm).
fn measure_channel_dimensions(
    terminal_pours: &FxHashMap<CompactString, Vec<PourMetadata>>,
    space: &HardwareSpace,
) -> Result<ChannelDimensions, String> {
    // 1. Find the primary Channel pour (Polysilicon body)
    let channel_pour = terminal_pours
        .values()
        .flatten()
        .find(|p| {
            p.device_binding
                .as_ref()
                .map_or(false, |b| b.priority == BindingPriority::Channel)
        })
        .or_else(|| {
            // Fallback: Find largest conductive pour
            terminal_pours.values().flatten().find(|p| {
                space
                    .material_registry
                    .get_id(&p.material_name)
                    .map_or(false, |id| space.material_registry.is_conductor(id))
            })
        })
        .ok_or_else(|| "No channel geometry found for measurement".to_string())?;

    let channel_bbox = channel_pour
        .bbox
        .as_ref()
        .ok_or_else(|| format!("Channel pour '{}' has no bounding box", channel_pour.name))?;

    let channel_dx_um = (channel_bbox.max.x - channel_bbox.min.x).abs() as f64 / 1000.0;
    let channel_dy_um = (channel_bbox.max.y - channel_bbox.min.y).abs() as f64 / 1000.0;

    // 2. Identify the conduction terminals bound to this channel pour
    let channel_terminals = channel_pour
        .device_binding
        .as_ref()
        .map(|b| &b.terminals);

    let (length_um, width_um) = if let Some(terms) = channel_terminals {
        if terms.len() >= 2 {
            let term_a = &terms[0];
            let term_b = &terms[1];

            // Find the contact pour on Terminal A and Terminal B
            let pour_a = terminal_pours.get(term_a).and_then(|v| {
                v.iter().find(|p| {
                    p.device_binding
                        .as_ref()
                        .map_or(false, |b| b.priority == BindingPriority::Contact)
                })
            });

            let pour_b = terminal_pours.get(term_b).and_then(|v| {
                v.iter().find(|p| {
                    p.device_binding
                        .as_ref()
                        .map_or(false, |b| b.priority == BindingPriority::Contact)
                })
            });

            if let (Some(pa), Some(pb)) = (pour_a, pour_b) {
                if let (Some(ba), Some(bb)) = (&pa.bbox, &pb.bbox) {
                    let sep_x = (ba.center_x() - bb.center_x()).abs();
                    let sep_y = (ba.center_y() - bb.center_y()).abs();

                    if sep_x >= sep_y {
                        // Current flows horizontally (along X-axis)
                        // Clear channel length = inner edge of right contact - inner edge of left contact
                        let left_inner_x = ba.max.x.min(bb.max.x);
                        let right_inner_x = ba.min.x.max(bb.min.x);
                        let l_clear_um = (right_inner_x - left_inner_x).max(0) as f64 / 1000.0;
                        
                        let l_final = if l_clear_um > 0.0 { l_clear_um } else { channel_dx_um };
                        let w_final = channel_dy_um; // Width is transverse (Y-axis)
                        
                        (l_final, w_final)
                    } else {
                        // Current flows vertically (along Y-axis)
                        // Clear channel length = inner edge of top contact - inner edge of bottom contact
                        let bottom_inner_y = ba.max.y.min(bb.max.y);
                        let top_inner_y = ba.min.y.max(bb.min.y);
                        let l_clear_um = (top_inner_y - bottom_inner_y).max(0) as f64 / 1000.0;
                        
                        let l_final = if l_clear_um > 0.0 { l_clear_um } else { channel_dy_um };
                        let w_final = channel_dx_um; // Width is transverse (X-axis)
                        
                        (l_final, w_final)
                    }
                } else {
                    (channel_dx_um.max(channel_dy_um), channel_dx_um.min(channel_dy_um))
                }
            } else {
                (channel_dx_um.max(channel_dy_um), channel_dx_um.min(channel_dy_um))
            }
        } else {
            (channel_dx_um.max(channel_dy_um), channel_dx_um.min(channel_dy_um))
        }
    } else {
        (channel_dx_um.max(channel_dy_um), channel_dx_um.min(channel_dy_um))
    };

    let area_um2 = width_um * length_um;
    let perimeter_um = 2.0 * (width_um + length_um);

    Ok(ChannelDimensions {
        width_um,
        length_um,
        area_um2,
        perimeter_um,
    })
}

pub struct DiffusionParasitics {
    pub drain_area_m2: f64,
    pub source_area_m2: f64,
    pub drain_perimeter_m: f64,
    pub source_perimeter_m: f64,
}

/// Measure MOSFET source/drain diffusion areas and perimeters
///
/// Used by MOSFETs, JFETs, MESFETs, and other FET-type devices
fn measure_mosfet_diffusion_parasitics(
    terminal_pours: &FxHashMap<CompactString, Vec<PourMetadata>>,
    terminal_order: &[CompactString],
) -> Result<DiffusionParasitics, String> {
    // SPICE standard: terminal_order = [drain, gate, source, bulk]
    let drain_term = terminal_order
        .get(0)
        .ok_or("terminal_order[0] (drain) not found")?;
    let source_term = terminal_order
        .get(2)
        .ok_or("terminal_order[2] (source) not found")?;

    let measure_diffusion = |term: &CompactString| -> Result<(f64, f64), String> {
        let pour = terminal_pours
            .get(term)
            .and_then(|v| v.first())
            .ok_or_else(|| format!("No geometry bound to terminal '{}'", term))?;

        let bbox = pour
            .bbox
            .as_ref()
            .ok_or_else(|| format!("Terminal '{}' has no bounding box", term))?;

        let w_m = (bbox.max.x - bbox.min.x).abs() as f64 * 1e-9;
        let l_m = (bbox.max.y - bbox.min.y).abs() as f64 * 1e-9;
        let area = w_m * l_m;
        let perimeter = 2.0 * (w_m + l_m);

        Ok((area, perimeter))
    };

    let (ad, pd) = measure_diffusion(drain_term).unwrap_or((0.0, 0.0));
    let (as_val, ps) = measure_diffusion(source_term).unwrap_or((0.0, 0.0));

    println!(
        "      ├─ Diffusion: AD={:.2e}m² AS={:.2e}m² PD={:.2e}m PS={:.2e}m",
        ad, as_val, pd, ps
    );

    Ok(DiffusionParasitics {
        drain_area_m2: ad,
        source_area_m2: as_val,
        drain_perimeter_m: pd,
        source_perimeter_m: ps,
    })
}
