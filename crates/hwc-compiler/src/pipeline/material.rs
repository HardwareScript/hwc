//! Universal material registry construction for the v0.3.0 pipeline.
//!
//! Registers every material declared in the symbol table together with its
//! extracted physical properties, then ensures the common PDK materials are
//! present so downstream stackup/constraint resolution never hits a missing
//! material.

use crate::symbol_table::SymbolTable;
use crate::pipeline::extract_numeric_value;
use hwc_engine::material::MaterialPhysicalProps;
use hwc_engine::MaterialRegistry;

/// Build the universal [`MaterialRegistry`] used as the basis for every space.
pub fn build_material_registry(symbol_table: &SymbolTable) -> MaterialRegistry {
    // 1. Build universal MaterialRegistry from symbol table
    let mut base_material_registry = MaterialRegistry::new();
    for mat_def in symbol_table.arena().material_defs.iter() {
        let category = mat_def.category();
        let parser_process = mat_def.get_process();
        let process = match parser_process {
            hwc_parser::ManufacturingProcess::DrilledPlated => {
                hwc_engine::ManufacturingProcess::DrilledPlated
            }
            hwc_parser::ManufacturingProcess::Deposited => {
                hwc_engine::ManufacturingProcess::Deposited
            }
            hwc_parser::ManufacturingProcess::Etched => hwc_engine::ManufacturingProcess::Etched,
        };
        let mat_id = base_material_registry.register_with_properties(
            mat_def.name.as_str(),
            category,
            process,
        );

        // Extract and register physical properties
        let mut physical_props = MaterialPhysicalProps::new();
        for (prop_name, prop_value) in &mat_def.properties {
            if prop_name.as_str() == "min_area" {
                match extract_area_nm2(prop_value) {
                    Ok(Some(val_nm2)) => {
                        physical_props.set("min_area", val_nm2);
                    }
                    Ok(None) => {}
                    Err(e) => {
                        eprintln!("[TYPE CHECK ERROR] Material '{}': {}", mat_def.name, e);
                        panic!("[TYPE CHECK ERROR] Material '{}': {}", mat_def.name, e);
                    }
                }
            } else if let Some(value) = extract_numeric_value(prop_value) {
                physical_props.set(prop_name.as_str(), value);
            }
        }
        if !physical_props.properties.is_empty() {
            base_material_registry.set_physical_props(mat_id, physical_props);
        }
    }

    // Ensure common PDK materials are registered
    base_material_registry.register_with_properties(
        "Polysilicon",
        hwc_parser::MaterialCategory::Conductor,
        hwc_engine::ManufacturingProcess::Deposited,
    );
    base_material_registry.register_with_properties(
        "Aluminum",
        hwc_parser::MaterialCategory::Conductor,
        hwc_engine::ManufacturingProcess::Deposited,
    );
    base_material_registry.register_with_properties(
        "Tungsten",
        hwc_parser::MaterialCategory::Conductor,
        hwc_engine::ManufacturingProcess::Deposited,
    );
    base_material_registry.register_with_properties(
        "Titanium_Nitride",
        hwc_parser::MaterialCategory::Conductor,
        hwc_engine::ManufacturingProcess::Deposited,
    );
    base_material_registry.register_with_properties(
        "Titanium_Silicide",
        hwc_parser::MaterialCategory::Conductor,
        hwc_engine::ManufacturingProcess::Deposited,
    );
    base_material_registry.register_with_properties(
        "Silicon_Dioxide",
        hwc_parser::MaterialCategory::Insulator,
        hwc_engine::ManufacturingProcess::Deposited,
    );
    base_material_registry.register_with_properties(
        "Resistor_Poly_Mask",
        hwc_parser::MaterialCategory::Mask,
        hwc_engine::ManufacturingProcess::Deposited,
    );
    base_material_registry.register_with_properties(
        "Npc_Mask",
        hwc_parser::MaterialCategory::Mask,
        hwc_engine::ManufacturingProcess::Deposited,
    );
    base_material_registry.register_with_properties(
        "P_Plus_Diffusion",
        hwc_parser::MaterialCategory::Semiconductor,
        hwc_engine::ManufacturingProcess::Deposited,
    );
    base_material_registry.register_with_properties(
        "P_Plus_Implant_Mask",
        hwc_parser::MaterialCategory::Mask,
        hwc_engine::ManufacturingProcess::Deposited,
    );
    base_material_registry.register_with_properties(
        "N_Plus_Implant_Mask",
        hwc_parser::MaterialCategory::Mask,
        hwc_engine::ManufacturingProcess::Deposited,
    );
    base_material_registry.register_with_properties(
        "Tap_Mask",
        hwc_parser::MaterialCategory::Mask,
        hwc_engine::ManufacturingProcess::Deposited,
    );
    base_material_registry.register_with_properties(
        "Pad_Mask",
        hwc_parser::MaterialCategory::Mask,
        hwc_engine::ManufacturingProcess::Deposited,
    );

    base_material_registry
}

fn extract_area_nm2(expr: &hwc_parser::Expression) -> Result<Option<f64>, String> {
    match expr {
        hwc_parser::Expression::Measurement { value, unit, .. } => {
            match unit {
                hwc_parser::Unit::Distance(d) => Err(format!(
                    "Dimensional mismatch for 'min_area': expected area unit ([L^2], e.g. 'um2', 'nm2'), found linear distance unit '{}' ([L])",
                    d
                )),
                hwc_parser::Unit::Custom(s) => match s.as_str() {
                    "um2" | "um²" | "µm2" | "µm²" => Ok(Some(*value * 1_000_000.0)),
                    "nm2" | "nm²" => Ok(Some(*value)),
                    "mm2" | "mm²" => Ok(Some(*value * 1e12)),
                    "m2" | "m²" => Ok(Some(*value * 1e18)),
                    "pm2" | "pm²" => Ok(Some(*value * 1e-6)),
                    other => Err(format!(
                        "Invalid unit for 'min_area': '{}' is not an area unit",
                        other
                    )),
                },
                other => Err(format!(
                    "Dimensional mismatch for 'min_area': expected area unit, found '{}'",
                    other
                )),
            }
        }
        hwc_parser::Expression::Literal { value, .. } => Ok(Some(*value as f64)),
        hwc_parser::Expression::FloatLiteral { value, .. } => Ok(Some(*value)),
        _ => Ok(None),
    }
}
