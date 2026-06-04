//! Conversion functions from AST definitions to runtime structures
//!
//! This module implements Phase 6.4 and 6.5:
//! - Profile → ConstraintSet conversion
//! - Material → MaterialDatabase conversion

use crate::symbol_table::SymbolTable;
use compact_str::CompactString;
use hwc_materials::{
    material::{ConductorProperties, InsulatorProperties, SemiconductorProperties},
    ClearanceConstraints, ConstraintSet, LayerConstraints, MaterialDatabase, TraceConstraints,
    ViaConstraints,
};
use hwc_parser::{MaterialCategory, ProfileDefinition, Unit};

/// Convert ProfileDefinition from Symbol Table to ConstraintSet
///
/// This implements Phase 6.4: Profile to Constraints Conversion
/// Reference: ROUTING-AND-PHYSICS.md - Translation 1 & 2
pub fn profile_to_constraints(
    profile: &ProfileDefinition,
    _symbol_table: &SymbolTable,
) -> Result<ConstraintSet, ConversionError> {
    // Extract trace constraints
    let trace = if let Some(trace_def) = &profile.trace {
        TraceConstraints {
            min_width_nm: measurement_to_nm(&trace_def.min_width),
            max_width_nm: 0, // Not specified in v0.1.4 profile syntax
            min_spacing_nm: measurement_to_nm(&trace_def.min_spacing),
            default_width_nm: measurement_to_nm(&trace_def.min_width), // Use min as default
        }
    } else {
        return Err(ConversionError::MissingProfileConstraint(
            "trace (min_width, min_spacing)".into(),
        ));
    };

    // Extract via constraints
    let via = if let Some(via_def) = &profile.via {
        ViaConstraints {
            min_diameter_nm: measurement_to_nm(&via_def.min_diameter),
            max_diameter_nm: 0, // Not specified in v0.1.4 profile syntax
            min_annular_ring_nm: measurement_to_nm(&via_def.min_annular_ring),
            min_spacing_nm: via_def
                .min_spacing
                .as_ref()
                .map(measurement_to_nm)
                .unwrap_or_else(|| measurement_to_nm(&via_def.min_diameter) * 2),
            default_diameter_nm: via_def
                .default_diameter
                .as_ref()
                .map(measurement_to_nm)
                .unwrap_or_else(|| measurement_to_nm(&via_def.min_diameter)),
        }
    } else {
        return Err(ConversionError::MissingProfileConstraint(
            "via (min_diameter, min_annular_ring)".into(),
        ));
    };

    // Extract manufacturing constraints (copper thickness, IPC-2221 constants)
    let copper_thickness_nm = profile
        .manufacturing
        .as_ref()
        .and_then(|m| m.copper_thickness.as_ref())
        .map(measurement_to_nm)
        .ok_or_else(|| {
            ConversionError::MissingProfileConstraint("manufacturing.copper_thickness".into())
        })?;

    let _ipc2221_k_external = profile
        .manufacturing
        .as_ref()
        .and_then(|m| m.ipc2221_k_external)
        .unwrap_or(0.048); // IPC-2221 default for external layers

    let _ipc2221_k_internal = profile
        .manufacturing
        .as_ref()
        .and_then(|m| m.ipc2221_k_internal)
        .unwrap_or(0.024); // IPC-2221 default for internal layers

    // Extract voltage classification thresholds
    let _low_voltage_threshold_v = profile
        .clearance
        .as_ref()
        .and_then(|c| c.low_voltage_threshold.as_ref())
        .map(|m| measurement_to_nm(m) / 1_000_000) // Convert nm to V (assuming voltage stored as mV in nm)
        .unwrap_or(50); // Default 50V threshold

    let _medium_voltage_threshold_v = profile
        .clearance
        .as_ref()
        .and_then(|c| c.medium_voltage_threshold.as_ref())
        .map(|m| measurement_to_nm(m) / 1_000_000)
        .unwrap_or(150); // Default 150V threshold

    // Extract clearance constraints
    // Note: These are baseline values. Actual clearances are calculated at routing time
    // using Translation 1: clearance = (voltage / dielectric_strength) × safety_factor
    // Reference: ROUTING-AND-PHYSICS.md - Translation 1: Dielectric Breakdown to Clearance
    let clearance = if let Some(clearance_def) = &profile.clearance {
        ClearanceConstraints {
            // v0.1.7: Standard net-to-net spacing is now governed by trace.min_spacing.
            // These low/medium voltage fields are legacy and not used by the DRC engine
            // for standard spacing checks.
            low_voltage_nm: 0,
            medium_voltage_nm: 0,
            // High voltage (>150V): User-specified or calculated
            high_voltage_nm: clearance_def
                .high_voltage
                .as_ref()
                .map(measurement_to_nm)
                .ok_or_else(|| {
                    ConversionError::MissingProfileConstraint("clearance.high_voltage".into())
                })?,
            // Safety factor per IPC-2221 recommendations
            safety_factor: clearance_def.safety_factor.ok_or_else(|| {
                ConversionError::MissingProfileConstraint("clearance.safety_factor".into())
            })?,
        }
    } else {
        return Err(ConversionError::MissingProfileConstraint(
            "clearance (high_voltage, safety_factor)".into(),
        ));
    };

    // Extract layer constraints
    let layer = if let Some(layer_def) = &profile.layer {
        LayerConstraints {
            min_thickness_nm: layer_def
                .min_thickness
                .as_ref()
                .map(measurement_to_nm)
                .unwrap_or(copper_thickness_nm),
            max_thickness_nm: 0, // Not specified in v0.1.4 profile syntax
            allowed_conductors: vec!["copper".into()], // Default to copper
            allowed_dielectrics: vec!["fr4".into(), "air".into()], // Common PCB materials
        }
    } else {
        return Err(ConversionError::MissingProfileConstraint(
            "layer (min_thickness)".into(),
        ));
    };

    // Extract thermal constraints
    // Note: No hardcoded defaults - thermal constraints are optional
    // If not specified in profile, physics validation will skip thermal checks
    let thermal = profile
        .thermal
        .as_ref()
        .map(|thermal_def| hwc_materials::ThermalConstraints {
            ambient_temp_c: measurement_to_celsius(&thermal_def.ambient_temp),
            max_operating_temp_c: measurement_to_celsius(&thermal_def.max_operating_temp),
            max_temp_rise_c: measurement_to_celsius(&thermal_def.max_temp_rise),
            clustering_threshold_nm: thermal_def
                .clustering_threshold
                .as_ref()
                .map(measurement_to_nm),
        });

    // v0.1.7: Old impedance-only stackup has been removed (see Phase 2 "Rip Off the Band-Aid").
    // Proper physical stackup derivation from LayerStackup + Material DB
    // will be done in the StackupManager (Phase 3).
    //
    // For now we emit None so the rest of constraint generation continues.
    let stackup: Option<hwc_materials::StackupConstraints> = None;

    // v0.1.7: Multi-material bridges
    let bridges = profile
        .bridges
        .iter()
        .map(|b| hwc_materials::BridgeRule {
            from_material: b.from.clone(),
            to_material: b.to.clone(),
            interface_material: b.interface_material.clone(),
            fill_material: b.fill_material.clone().unwrap_or_else(|| b.interface_material.clone()),
        })
        .collect();

    Ok(ConstraintSet {
        name: profile.name.to_string().into(),
        description: profile.description.clone().unwrap_or_default(),
        trace,
        via,
        clearance,
        layer,
        thermal,
        stackup,
        bridges,
    })
}

/// Populate MaterialDatabase from Symbol Table
pub fn populate_material_database(
    symbol_table: &SymbolTable,
) -> Result<MaterialDatabase, ConversionError> {
    let mut database = MaterialDatabase::empty();

    // Iterate through all materials in symbol table
    for (name, material_def) in symbol_table.materials() {
        match material_def.category {
            MaterialCategory::Conductor => {
                // Extract conductor properties from material definition
                // NOTE: No hardcoded defaults - all values must come from material definition
                // Standard library (hwc/stdlib/materials.hw) provides default materials
                // Users can override by defining materials with the same name
                let mut resistivity_ohm_m = None;
                let mut thermal_conductivity_w_mk = None;
                let mut density_kg_m3 = None;
                let mut max_current_density_a_mm2 = None;
                let mut melting_point_c = None;
                let mut resistivity_temp_coeff_per_c = None;
                let mut thermal_conductivity_temp_coeff_per_c = None;
                let mut reference_temp_c = None;

                // Parse properties
                for prop in &material_def.properties {
                    match prop.key.as_str() {
                        "resistivity" => {
                            if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
                                resistivity_ohm_m = Some(convert_to_base_unit(m.value, &m.unit));
                            }
                        }
                        "thermal_conductivity" => {
                            if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
                                thermal_conductivity_w_mk =
                                    Some(convert_to_base_unit(m.value, &m.unit));
                            }
                        }
                        "density" => {
                            if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
                                density_kg_m3 = Some(convert_to_base_unit(m.value, &m.unit));
                            }
                        }
                        "max_current_density" => {
                            if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
                                max_current_density_a_mm2 =
                                    Some(convert_to_base_unit(m.value, &m.unit));
                            }
                        }
                        "melting_point" => {
                            if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
                                melting_point_c = Some(convert_to_base_unit(m.value, &m.unit));
                            }
                        }
                        "resistivity_temp_coeff" => {
                            if let hwc_parser::PropertyValue::Number(n) = prop.value {
                                resistivity_temp_coeff_per_c = Some(n);
                            }
                        }
                        "thermal_conductivity_temp_coeff" => {
                            if let hwc_parser::PropertyValue::Number(n) = prop.value {
                                thermal_conductivity_temp_coeff_per_c = Some(n);
                            }
                        }
                        "reference_temp" => {
                            if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
                                reference_temp_c = Some(convert_to_base_unit(m.value, &m.unit));
                            }
                        }
                        _ => {} // Ignore unknown properties
                    }
                }

                // Validate required properties
                let resistivity_ohm_m =
                    resistivity_ohm_m.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "resistivity".into(),
                    })?;
                let thermal_conductivity_w_mk =
                    thermal_conductivity_w_mk.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "thermal_conductivity".into(),
                    })?;
                let density_kg_m3 =
                    density_kg_m3.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "density".into(),
                    })?;
                let max_current_density_a_mm2 =
                    max_current_density_a_mm2.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "max_current_density".into(),
                    })?;
                let melting_point_c =
                    melting_point_c.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "melting_point".into(),
                    })?;

                let conductor = ConductorProperties {
                    name: name.clone(),
                    symbol: material_def.symbol.clone().unwrap_or_default(),
                    description: material_def.description.clone().unwrap_or_default(),
                    process: match material_def.process {
                        hwc_parser::ManufacturingProcess::DrilledPlated => {
                            hwc_materials::ManufacturingProcess::DrilledPlated
                        }
                        hwc_parser::ManufacturingProcess::Deposited => {
                            hwc_materials::ManufacturingProcess::Deposited
                        }
                        hwc_parser::ManufacturingProcess::Etched => {
                            hwc_materials::ManufacturingProcess::Etched
                        }
                    },
                    density_kg_m3,
                    thermal_conductivity_w_mk,
                    color_hex: "#B87333".into(), // Default copper color
                    resistivity_ohm_m,
                    max_current_density_a_mm2,
                    resistivity_temp_coeff_per_c,
                    thermal_conductivity_temp_coeff_per_c,
                    reference_temp_c,
                    melting_point_c,
                    is_metal: true,
                };

                database.conductors.insert(name.to_lowercase(), conductor);
            }
            MaterialCategory::Insulator => {
                // Extract insulator properties from material definition
                // NOTE: No hardcoded defaults - all values must come from material definition
                // Standard library (hwc/stdlib/materials.hw) provides default materials
                // Users can override by defining materials with the same name
                let mut dielectric_strength_kv_mm = None;
                let mut relative_permittivity = None;
                let mut thermal_conductivity_w_mk = None;
                let mut density_kg_m3 = None;
                let mut thermal_conductivity_temp_coeff_per_c = None;
                let mut reference_temp_c = None;
                let mut glass_transition_temp_c = None;
                let mut max_operating_temp_c = None;

                // Parse properties
                for prop in &material_def.properties {
                    match prop.key.as_str() {
                        "dielectric_strength" => {
                            if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
                                // Dielectric strength is stored in kV/mm in the material definition
                                // No conversion needed - use value directly
                                dielectric_strength_kv_mm = Some(m.value);
                            }
                        }
                        "dielectric_constant" | "relative_permittivity" => {
                            if let hwc_parser::PropertyValue::Number(n) = prop.value {
                                relative_permittivity = Some(n);
                            }
                        }
                        "thermal_conductivity" => {
                            if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
                                thermal_conductivity_w_mk =
                                    Some(convert_to_base_unit(m.value, &m.unit));
                            }
                        }
                        "density" => {
                            if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
                                density_kg_m3 = Some(convert_to_base_unit(m.value, &m.unit));
                            }
                        }
                        "thermal_conductivity_temp_coeff" => {
                            if let hwc_parser::PropertyValue::Number(n) = prop.value {
                                thermal_conductivity_temp_coeff_per_c = Some(n);
                            }
                        }
                        "reference_temp" => {
                            if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
                                reference_temp_c = Some(convert_to_base_unit(m.value, &m.unit));
                            }
                        }
                        "glass_transition_temp" => {
                            if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
                                glass_transition_temp_c =
                                    Some(convert_to_base_unit(m.value, &m.unit));
                            }
                        }
                        "max_operating_temp" => {
                            if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
                                max_operating_temp_c = Some(convert_to_base_unit(m.value, &m.unit));
                            }
                        }
                        _ => {} // Ignore unknown properties
                    }
                }

                // Validate required properties
                let dielectric_strength_kv_mm =
                    dielectric_strength_kv_mm.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "dielectric_strength".into(),
                    })?;
                let relative_permittivity =
                    relative_permittivity.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "dielectric_constant or relative_permittivity".into(),
                    })?;
                let thermal_conductivity_w_mk =
                    thermal_conductivity_w_mk.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "thermal_conductivity".into(),
                    })?;
                let density_kg_m3 =
                    density_kg_m3.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "density".into(),
                    })?;

                let insulator = InsulatorProperties {
                    name: name.clone(),
                    symbol: material_def.symbol.clone().unwrap_or_default(),
                    description: material_def.description.clone().unwrap_or_default(),
                    process: match material_def.process {
                        hwc_parser::ManufacturingProcess::DrilledPlated => {
                            hwc_materials::ManufacturingProcess::DrilledPlated
                        }
                        hwc_parser::ManufacturingProcess::Deposited => {
                            hwc_materials::ManufacturingProcess::Deposited
                        }
                        hwc_parser::ManufacturingProcess::Etched => {
                            hwc_materials::ManufacturingProcess::Etched
                        }
                    },
                    density_kg_m3,
                    thermal_conductivity_w_mk,
                    color_hex: "#4CAF50".into(), // Default green for insulators
                    relative_permittivity,
                    dielectric_strength_kv_mm,
                    thermal_conductivity_temp_coeff_per_c,
                    reference_temp_c,
                    glass_transition_temp_c,
                    max_operating_temp_c,
                };

                database.insulators.insert(name.to_lowercase(), insulator);
            }
            MaterialCategory::Semiconductor => {
                // Extract semiconductor properties
                let mut band_gap_ev: Option<f64> = None;
                let mut electron_mobility_cm2_vs: Option<f64> = None;
                let mut hole_mobility_cm2_vs: Option<f64> = None;
                let mut thermal_conductivity_w_mk: Option<f64> = None;
                let mut density_kg_m3: Option<f64> = None;
                let mut max_operating_temp_c: Option<f64> = None;

                // NEW v0.1.6: Doping and biasing properties for physics validation
                let mut doping_type: Option<hwc_materials::DopingType> = None;
                let mut bias_requirement: Option<hwc_materials::BiasRequirement> = None;

                for prop in &material_def.properties {
                    match prop.key.as_str() {
                        "band_gap" | "bandgap" => {
                            if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
                                band_gap_ev = Some(convert_to_base_unit(m.value, &m.unit));
                            }
                        }
                        "electron_mobility" => {
                            if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
                                electron_mobility_cm2_vs =
                                    Some(convert_to_base_unit(m.value, &m.unit));
                            }
                        }
                        "hole_mobility" => {
                            if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
                                hole_mobility_cm2_vs = Some(convert_to_base_unit(m.value, &m.unit));
                            }
                        }
                        "thermal_conductivity" => {
                            if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
                                thermal_conductivity_w_mk =
                                    Some(convert_to_base_unit(m.value, &m.unit));
                            }
                        }
                        "density" => {
                            if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
                                density_kg_m3 = Some(convert_to_base_unit(m.value, &m.unit));
                            }
                        }
                        "max_operating_temp" => {
                            if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
                                max_operating_temp_c = Some(convert_to_base_unit(m.value, &m.unit));
                            }
                        }
                        // NEW v0.1.6: Parse doping_type property
                        "doping_type" => {
                            if let hwc_parser::PropertyValue::String(s) = &prop.value {
                                doping_type = match s.to_lowercase().as_str() {
                                    "p-type" | "p_type" | "ptype" => {
                                        Some(hwc_materials::DopingType::PType)
                                    }
                                    "n-type" | "n_type" | "ntype" => {
                                        Some(hwc_materials::DopingType::NType)
                                    }
                                    "intrinsic" | "undoped" => {
                                        Some(hwc_materials::DopingType::Intrinsic)
                                    }
                                    _ => None,
                                };
                            }
                        }
                        // NEW v0.1.6: Parse bias_requirement property
                        "bias_requirement" => {
                            if let hwc_parser::PropertyValue::String(s) = &prop.value {
                                bias_requirement = match s.to_lowercase().as_str() {
                                    "lowest_potential" | "ground" | "gnd" => {
                                        Some(hwc_materials::BiasRequirement::LowestPotential)
                                    }
                                    "highest_potential" | "power" | "vdd" => {
                                        Some(hwc_materials::BiasRequirement::HighestPotential)
                                    }
                                    "none" | "no_requirement" => {
                                        Some(hwc_materials::BiasRequirement::None)
                                    }
                                    _ => None,
                                };
                            }
                        }
                        _ => {} // Ignore unknown properties
                    }
                }

                // Validate required properties
                let band_gap_ev = band_gap_ev.ok_or_else(|| ConversionError::MissingProperty {
                    material: name.clone(),
                    property: "band_gap".into(),
                })?;
                let electron_mobility_cm2_vs =
                    electron_mobility_cm2_vs.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "electron_mobility".into(),
                    })?;
                let hole_mobility_cm2_vs =
                    hole_mobility_cm2_vs.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "hole_mobility".into(),
                    })?;
                let thermal_conductivity_w_mk =
                    thermal_conductivity_w_mk.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "thermal_conductivity".into(),
                    })?;
                let density_kg_m3 =
                    density_kg_m3.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "density".into(),
                    })?;

                let semiconductor = SemiconductorProperties {
                    name: name.clone(),
                    symbol: material_def.symbol.clone().unwrap_or_default(),
                    description: material_def.description.clone().unwrap_or_default(),
                    process: match material_def.process {
                        hwc_parser::ManufacturingProcess::DrilledPlated => {
                            hwc_materials::ManufacturingProcess::DrilledPlated
                        }
                        hwc_parser::ManufacturingProcess::Deposited => {
                            hwc_materials::ManufacturingProcess::Deposited
                        }
                        hwc_parser::ManufacturingProcess::Etched => {
                            hwc_materials::ManufacturingProcess::Etched
                        }
                    },
                    density_kg_m3,
                    thermal_conductivity_w_mk,
                    color_hex: material_def
                        .properties
                        .iter()
                        .find(|prop| prop.key == "color")
                        .and_then(|prop| match &prop.value {
                            hwc_parser::PropertyValue::String(s) => Some(s.clone()),
                            _ => None,
                        })
                        .unwrap_or_else(|| "#708090".into())
                        .into(), // Default slate gray
                    band_gap_ev,
                    electron_mobility_cm2_vs,
                    hole_mobility_cm2_vs,
                    max_operating_temp_c,
                    // NEW v0.1.6: Doping and biasing properties
                    doping_type,
                    bias_requirement,
                };

                database
                    .semiconductors
                    .insert(name.to_lowercase(), semiconductor);
            }

            // Bridge categories (Phase 1 - BRIDGE-IMPLEMENTATION.md)
            // All bridge materials are conductive and stored in the conductors map.
            // They use relaxed validation: only resistivity and density are required.
            // Missing properties get sensible defaults since bridge materials may use
            // different property names (e.g., curing_temp instead of melting_point).
            MaterialCategory::OhmicContact
            | MaterialCategory::DieInterconnect
            | MaterialCategory::PcbSolder
            | MaterialCategory::BarrierLayer
            | MaterialCategory::Adhesive => {
                let mut resistivity_ohm_m = None;
                let mut thermal_conductivity_w_mk = None;
                let mut density_kg_m3 = None;
                let mut max_current_density_a_mm2 = None;
                let mut melting_point_c = None;
                let mut resistivity_temp_coeff_per_c = None;
                let mut thermal_conductivity_temp_coeff_per_c = None;
                let mut reference_temp_c = None;

                for prop in &material_def.properties {
                    match prop.key.as_str() {
                        "resistivity" => {
                            if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
                                resistivity_ohm_m = Some(convert_to_base_unit(m.value, &m.unit));
                            }
                        }
                        "thermal_conductivity" => {
                            if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
                                thermal_conductivity_w_mk =
                                    Some(convert_to_base_unit(m.value, &m.unit));
                            }
                        }
                        "density" => {
                            if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
                                density_kg_m3 = Some(convert_to_base_unit(m.value, &m.unit));
                            }
                        }
                        "max_current_density" => {
                            if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
                                max_current_density_a_mm2 =
                                    Some(convert_to_base_unit(m.value, &m.unit));
                            }
                        }
                        // Accept both melting_point and bonding/curing temps as the thermal limit
                        "melting_point" | "bonding_temp" | "curing_temp" | "reflow_temp" => {
                            if melting_point_c.is_none() {
                                if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
                                    melting_point_c = Some(convert_to_base_unit(m.value, &m.unit));
                                }
                            }
                        }
                        "temp_coefficient" | "resistivity_temp_coeff" => {
                            if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
                                resistivity_temp_coeff_per_c = Some(convert_to_base_unit(m.value, &m.unit));
                            }
                        }
                        "thermal_conductivity_temp_coeff" => {
                            if let hwc_parser::PropertyValue::Number(n) = prop.value {
                                thermal_conductivity_temp_coeff_per_c = Some(n);
                            }
                        }
                        "reference_temp" => {
                            if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
                                reference_temp_c = Some(convert_to_base_unit(m.value, &m.unit));
                            }
                        }
                        _ => {} // Bridge materials have many specialized properties we don't need here
                    }
                }

                // Bridge materials: only resistivity and density are strictly required
                let resistivity_ohm_m =
                    resistivity_ohm_m.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "resistivity".into(),
                    })?;
                let density_kg_m3 =
                    density_kg_m3.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "density".into(),
                    })?;

                // Extract color from properties if available
                let color_hex = material_def
                    .properties
                    .iter()
                    .find(|prop| prop.key == "color")
                    .and_then(|prop| match &prop.value {
                        hwc_parser::PropertyValue::String(s) => Some(s.clone()),
                        _ => None,
                    })
                    .unwrap_or_else(|| "#808080".into()); // Default gray for bridge materials

                let conductor = ConductorProperties {
                    name: name.clone(),
                    symbol: material_def.symbol.clone().unwrap_or_default(),
                    description: material_def.description.clone().unwrap_or_default(),
                    process: match material_def.process {
                        hwc_parser::ManufacturingProcess::DrilledPlated => {
                            hwc_materials::ManufacturingProcess::DrilledPlated
                        }
                        hwc_parser::ManufacturingProcess::Deposited => {
                            hwc_materials::ManufacturingProcess::Deposited
                        }
                        hwc_parser::ManufacturingProcess::Etched => {
                            hwc_materials::ManufacturingProcess::Etched
                        }
                    },
                    density_kg_m3,
                    thermal_conductivity_w_mk: thermal_conductivity_w_mk.unwrap_or(20.0), // Conservative default
                    color_hex: color_hex.into(),
                    resistivity_ohm_m,
                    max_current_density_a_mm2: max_current_density_a_mm2.unwrap_or(1e6), // Conservative default
                    resistivity_temp_coeff_per_c,
                    thermal_conductivity_temp_coeff_per_c,
                    reference_temp_c,
                    melting_point_c: melting_point_c.unwrap_or(1000.0), // Conservative default
                    is_metal: !matches!(material_def.category, MaterialCategory::Adhesive),
                };

                database.conductors.insert(name.to_lowercase(), conductor);
            }
        }
    }

    Ok(database)
}

/// Convert measurement to nanometers (for distances)
fn measurement_to_nm(measurement: &hwc_parser::Measurement) -> i64 {
    let value_nm = match measurement.unit {
        Unit::Millimeter => measurement.value * 1_000_000.0,
        Unit::Centimeter => measurement.value * 10_000_000.0,
        Unit::Micrometer => measurement.value * 1_000.0,
        _ => measurement.value, // Fallback for non-distance units
    };
    value_nm as i64
}

/// Convert measurement to Celsius (for temperatures)
fn measurement_to_celsius(measurement: &hwc_parser::Measurement) -> f64 {
    match measurement.unit {
        Unit::Celsius => measurement.value,
        _ => measurement.value, // Fallback - assume Celsius
    }
}

/// Convert measurement to base SI unit (for material properties)
/// Note: Material property units (kg/m³, W/mK, Ω·m, A/mm²) are already in their
/// base SI form as defined in the standard library, so no conversion is needed.
/// The parser stores them as Custom(String) and we use them directly.
fn convert_to_base_unit(value: f64, _unit: &Unit) -> f64 {
    // Material properties in standard-materials.hw are already in base SI units:
    // - density: kg/m³ (base SI)
    // - resistivity: Ω·m (base SI)
    // - thermal_conductivity: W/mK or W/(m·K) (base SI)
    // - max_current_density: A/mm² (base SI)
    // - dielectric_strength: V/m (base SI)
    // - temperature: C (Celsius, as specified)
    //
    // No conversion needed - values are used as-is
    value
}

/// Calculate minimum clearance based on voltage and dielectric strength
///
/// Implements Translation 1 from ROUTING-AND-PHYSICS.md:
/// clearance = (voltage / dielectric_strength) × safety_factor
///
/// # Arguments
/// * `voltage_diff_mv` - Voltage difference in millivolts
/// * `dielectric_strength_kv_mm` - Dielectric strength in kV/mm
/// * `safety_factor` - Safety multiplier (typically 2.0 per IPC-2221)
///
/// # Returns
/// Minimum clearance in nanometers
///
/// # Example
/// 120V across FR4 (20 kV/mm) with 2× safety factor:
/// - Calculation: 120V / 20,000V/mm = 0.006mm = 6µm
/// - With 2× safety: 12µm = 12,000nm
pub fn calculate_clearance_nm(
    voltage_diff_mv: i64,
    dielectric_strength_kv_mm: f64,
    safety_factor: f64,
) -> i64 {
    let voltage_v = voltage_diff_mv as f64 / 1000.0;

    // Convert dielectric strength from kV/mm to V/mm
    let dielectric_v_mm = dielectric_strength_kv_mm * 1000.0;

    // Calculate minimum clearance in mm
    let min_clearance_mm = voltage_v / dielectric_v_mm;

    // Convert to nm and apply safety factor
    let min_clearance_nm = min_clearance_mm * 1_000_000.0;

    (min_clearance_nm * safety_factor) as i64
}

/// Calculate trace width required for given current using IPC-2221 formula
///
/// Implements Translation 2 from ROUTING-AND-PHYSICS.md:
/// I = k × ΔT^0.44 × A^0.725
/// Solving for A: A = (I / (k × ΔT^0.44))^(1/0.725)
/// Then: width = A / thickness
///
/// CRITICAL: IPC-2221 uses mils² for area, not mm²
///
/// # Arguments
/// * `current_ma` - Current in milliamperes
/// * `temp_rise_c` - Allowed temperature rise in Celsius
/// * `is_external` - True for external layers, false for internal
/// * `thickness_nm` - Copper thickness in nanometers (typically 35,000 for 1oz)
///
/// # Returns
/// Minimum trace width in nanometers
///
/// # Example
/// 1A on external layer, 10°C rise, 1oz copper (35µm = 1.378 mils):
/// - Result: ~1.5mm for 1A
///
/// # References
/// - IPC-2221A Section 6.2
/// - Valid for: 0-35A, 10-100°C rise, 0.5-3oz copper
pub fn calculate_trace_width_nm(
    current_ma: i64,
    temp_rise_c: i64,
    is_external: bool,
    thickness_nm: i64,
) -> i64 {
    // IPC-2221 constants (defaults - should come from profile)
    let k = if is_external { 0.048 } else { 0.024 };

    let current_a = current_ma as f64 / 1000.0;
    let temp_rise = temp_rise_c as f64;

    // Calculate required cross-sectional area using IPC-2221 formula
    // I = k × ΔT^0.44 × A^0.725
    // Solving for A: A = (I / (k × ΔT^0.44))^(1/0.725)
    let area_mils2 = (current_a / (k * temp_rise.powf(0.44))).powf(1.0 / 0.725);

    // Convert thickness from nm to mils (1 mil = 25,400 nm)
    let thickness_mils = thickness_nm as f64 / 25_400.0;

    // Calculate width in mils (area = width × thickness)
    let width_mils = area_mils2 / thickness_mils;

    // Convert width from mils to nm (1 mil = 25,400 nm)
    (width_mils * 25_400.0) as i64
}

/// Calculate minimum trace width using IPC-2221 formula with custom k-value
///
/// # Arguments
/// * `current_ma` - Current in milliamps
/// * `temp_rise_c` - Temperature rise in °C
/// * `k_value` - IPC-2221 constant (from profile manufacturing constraints)
/// * `thickness_nm` - Copper thickness in nanometers
pub fn calculate_trace_width_nm_with_k(
    current_ma: i64,
    temp_rise_c: i64,
    k_value: f64,
    thickness_nm: i64,
) -> i64 {
    let current_a = current_ma as f64 / 1000.0;
    let temp_rise = temp_rise_c as f64;

    let area_mils2 = (current_a / (k_value * temp_rise.powf(0.44))).powf(1.0 / 0.725);
    let thickness_mils = thickness_nm as f64 / 25_400.0;
    let width_mils = area_mils2 / thickness_mils;

    (width_mils * 25_400.0) as i64
}

/// Calculate crosstalk penalty for parallel trace routing
///
/// Implements Translation 3 from ROUTING-AND-PHYSICS.md:
/// Adds exponential cost penalty when traces run parallel for too long
///
/// # Arguments
/// * `parallel_length_nm` - Length of parallel routing in nanometers
/// * `max_parallel_nm` - Maximum allowed parallel length
///
/// # Returns
/// Cost penalty (0 if within limits, exponential if exceeded)
///
/// # Example
/// Traces parallel for 5mm, max allowed is 3mm:
/// - Result: High penalty to discourage this routing
pub fn calculate_crosstalk_penalty(parallel_length_nm: i64, max_parallel_nm: i64) -> i64 {
    if parallel_length_nm <= max_parallel_nm {
        return 0;
    }

    let excess = parallel_length_nm - max_parallel_nm;
    let ratio = (excess * 1000) / max_parallel_nm;

    // Exponential penalty: base + linear + quadratic
    1000 + ratio + (ratio * ratio) / 2000
}

#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
    #[error("Invalid measurement unit for conversion: {0}")]
    InvalidUnit(String),

    #[error("Missing required property '{property}' in material '{material}'")]
    MissingProperty {
        material: CompactString,
        property: String,
    },

    #[error("Missing profile constraint: {0}")]
    MissingProfileConstraint(String),
}
