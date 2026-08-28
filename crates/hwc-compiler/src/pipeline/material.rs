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
            // Extract numeric value from the property expression
            if let Some(value) = extract_numeric_value(prop_value) {
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
