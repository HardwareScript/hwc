use crate::symbol_table::SymbolTable;
use hwc_materials::material::{ConductorProperties, InsulatorProperties, SemiconductorProperties};
use hwc_materials::MaterialDatabase;
use hwc_parser::MaterialCategory;

use super::error::ConversionError;
use super::unit_conversion::convert_to_base_unit;

/// Populate MaterialDatabase from Symbol Table
pub fn populate_material_database(
    symbol_table: &SymbolTable,
) -> Result<MaterialDatabase, ConversionError> {
    let mut database = MaterialDatabase::empty();

    for (name, material_def) in symbol_table.materials() {
        match material_def.category {
            MaterialCategory::Conductor => {
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
                        _ => {}
                    }
                }

                let resistivity_ohm_m =
                    resistivity_ohm_m.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "resistivity".to_string(),
                    })?;
                let thermal_conductivity_w_mk =
                    thermal_conductivity_w_mk.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "thermal_conductivity".to_string(),
                    })?;
                let density_kg_m3 =
                    density_kg_m3.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "density".to_string(),
                    })?;
                let max_current_density_a_mm2 =
                    max_current_density_a_mm2.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "max_current_density".to_string(),
                    })?;
                let melting_point_c =
                    melting_point_c.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "melting_point".to_string(),
                    })?;

                let color_hex = material_def
                    .properties
                    .iter()
                    .find(|prop| prop.key == "color")
                    .and_then(|prop| match &prop.value {
                        hwc_parser::PropertyValue::String(s) => Some(s.clone()),
                        _ => None,
                    })
                    .ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "color".to_string(),
                    })?;

                let conductor = ConductorProperties {
                    name: name.clone(),
                    symbol: material_def.symbol.clone().unwrap_or_default(),
                    description: material_def.description.clone().unwrap_or_default(),
                    process: match material_def.get_process() {
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
                    color_hex: color_hex.into(),
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
                let mut dielectric_strength_kv_mm = None;
                let mut relative_permittivity = None;
                let mut thermal_conductivity_w_mk = None;
                let mut density_kg_m3 = None;
                let mut thermal_conductivity_temp_coeff_per_c = None;
                let mut reference_temp_c = None;
                let mut glass_transition_temp_c = None;
                let mut max_operating_temp_c = None;

                for prop in &material_def.properties {
                    match prop.key.as_str() {
                        "dielectric_strength" => {
                            if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
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
                        _ => {}
                    }
                }

                let dielectric_strength_kv_mm =
                    dielectric_strength_kv_mm.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "dielectric_strength".to_string(),
                    })?;
                let relative_permittivity =
                    relative_permittivity.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "relative_permittivity".to_string(),
                    })?;
                let thermal_conductivity_w_mk =
                    thermal_conductivity_w_mk.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "thermal_conductivity".to_string(),
                    })?;
                let density_kg_m3 =
                    density_kg_m3.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "density".to_string(),
                    })?;

                let color_hex = material_def
                    .properties
                    .iter()
                    .find(|prop| prop.key == "color")
                    .and_then(|prop| match &prop.value {
                        hwc_parser::PropertyValue::String(s) => Some(s.clone()),
                        _ => None,
                    })
                    .ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "color".to_string(),
                    })?;

                let insulator = InsulatorProperties {
                    name: name.clone(),
                    symbol: material_def.symbol.clone().unwrap_or_default(),
                    description: material_def.description.clone().unwrap_or_default(),
                    process: match material_def.get_process() {
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
                    color_hex: color_hex.into(),
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
                let mut band_gap_ev: Option<f64> = None;
                let mut electron_mobility_cm2_vs: Option<f64> = None;
                let mut hole_mobility_cm2_vs: Option<f64> = None;
                let mut thermal_conductivity_w_mk: Option<f64> = None;
                let mut density_kg_m3: Option<f64> = None;
                let mut max_operating_temp_c: Option<f64> = None;

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
                        _ => {}
                    }
                }

                let band_gap_ev = band_gap_ev.ok_or_else(|| ConversionError::MissingProperty {
                    material: name.clone(),
                    property: "band_gap".to_string(),
                })?;
                let electron_mobility_cm2_vs =
                    electron_mobility_cm2_vs.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "electron_mobility".to_string(),
                    })?;
                let hole_mobility_cm2_vs =
                    hole_mobility_cm2_vs.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "hole_mobility".to_string(),
                    })?;
                let thermal_conductivity_w_mk =
                    thermal_conductivity_w_mk.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "thermal_conductivity".to_string(),
                    })?;
                let density_kg_m3 =
                    density_kg_m3.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "density".to_string(),
                    })?;

                let color_hex = material_def
                    .properties
                    .iter()
                    .find(|prop| prop.key == "color")
                    .and_then(|prop| match &prop.value {
                        hwc_parser::PropertyValue::String(s) => Some(s.clone()),
                        _ => None,
                    })
                    .ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "color".to_string(),
                    })?;

                let semiconductor = SemiconductorProperties {
                    name: name.clone(),
                    symbol: material_def.symbol.clone().unwrap_or_default(),
                    description: material_def.description.clone().unwrap_or_default(),
                    process: match material_def.get_process() {
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
                    color_hex: color_hex.into(),
                    band_gap_ev,
                    electron_mobility_cm2_vs,
                    hole_mobility_cm2_vs,
                    max_operating_temp_c,
                    doping_type,
                    bias_requirement,
                };

                database
                    .semiconductors
                    .insert(name.to_lowercase(), semiconductor);
            }

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
                        "melting_point" | "bonding_temp" | "curing_temp" | "reflow_temp" => {
                            if melting_point_c.is_none() {
                                if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
                                    melting_point_c = Some(convert_to_base_unit(m.value, &m.unit));
                                }
                            }
                        }
                        "temp_coefficient" | "resistivity_temp_coeff" => {
                            if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
                                resistivity_temp_coeff_per_c =
                                    Some(convert_to_base_unit(m.value, &m.unit));
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
                        _ => {}
                    }
                }

                let resistivity_ohm_m =
                    resistivity_ohm_m.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "resistivity".to_string(),
                    })?;
                let density_kg_m3 =
                    density_kg_m3.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "density".to_string(),
                    })?;
                let thermal_conductivity_w_mk =
                    thermal_conductivity_w_mk.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "thermal_conductivity".to_string(),
                    })?;
                let max_current_density_a_mm2 =
                    max_current_density_a_mm2.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "max_current_density".to_string(),
                    })?;
                let melting_point_c =
                    melting_point_c.ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "melting_point".to_string(),
                    })?;

                let color_hex = material_def
                    .properties
                    .iter()
                    .find(|prop| prop.key == "color")
                    .and_then(|prop| match &prop.value {
                        hwc_parser::PropertyValue::String(s) => Some(s.clone()),
                        _ => None,
                    })
                    .ok_or_else(|| ConversionError::MissingProperty {
                        material: name.clone(),
                        property: "color".to_string(),
                    })?;

                let conductor = ConductorProperties {
                    name: name.clone(),
                    symbol: material_def.symbol.clone().unwrap_or_default(),
                    description: material_def.description.clone().unwrap_or_default(),
                    process: match material_def.get_process() {
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
                    color_hex: color_hex.into(),
                    resistivity_ohm_m,
                    max_current_density_a_mm2,
                    resistivity_temp_coeff_per_c,
                    thermal_conductivity_temp_coeff_per_c,
                    reference_temp_c,
                    melting_point_c,
                    is_metal: !matches!(material_def.category, MaterialCategory::Adhesive),
                };

                database.conductors.insert(name.to_lowercase(), conductor);
            }
        }
    }

    Ok(database)
}
