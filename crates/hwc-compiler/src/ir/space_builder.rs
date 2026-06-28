//! Hardware space creation from space definitions.

use super::conversions::measurement_to_nm;
use super::errors::IrError;
use crate::conversions::profile_to_constraints;
use hwc_engine::{Dimensions, HardwareSpace, MaterialRegistry, AIR_MATERIAL_ID};
use hwc_parser::SpaceDefinition;

/// Create a hardware space from space definition.
pub fn create_hardware_space(
    space_def: &SpaceDefinition,
    symbol_table: &crate::SymbolTable,
) -> Result<HardwareSpace, IrError> {
    let dimensions = space_def
        .dimensions
        .as_ref()
        .ok_or(IrError::MissingDimensions)?;

    // Convert dimensions to nanometers using the symbol table (supports custom units!)
    let dims = Dimensions {
        width_nm: measurement_to_nm(&dimensions.width, symbol_table)
            .map_err(|e| IrError::InvalidExpression(e))?,
        height_nm: measurement_to_nm(&dimensions.height, symbol_table)
            .map_err(|e| IrError::InvalidExpression(e))?,
        depth_nm: measurement_to_nm(&dimensions.depth, symbol_table)
            .map_err(|e| IrError::InvalidExpression(e))?,
    };

    // Determine resolution for coordinate snapping
    let resolution_nm = if let Some(res_measurement) = &space_def.resolution {
        let res = measurement_to_nm(res_measurement, symbol_table)
            .map_err(|e| IrError::InvalidExpression(e))?;
        if res <= 0 {
            return Err(IrError::InvalidResolution { value: res });
        }
        res
    } else {
        // Default resolution if not specified (e.g., 1nm for ASIC, 10um for PCB)
        1
    };

    // Create material registry
    let mut material_registry = MaterialRegistry::new();

    // Populate registry from symbol table material definitions.
    // This ensures every declared/imported material has correct conductivity
    // BEFORE any pours or contacts call get_or_register().
    for (name, mat_def) in symbol_table.materials() {
        let conductivity = match mat_def.category {
            hwc_parser::MaterialCategory::Conductor
            | hwc_parser::MaterialCategory::OhmicContact
            | hwc_parser::MaterialCategory::DieInterconnect
            | hwc_parser::MaterialCategory::PcbSolder
            | hwc_parser::MaterialCategory::BarrierLayer
            | hwc_parser::MaterialCategory::Adhesive => {
                hwc_engine::MaterialConductivity::Conductor
            }
            hwc_parser::MaterialCategory::Semiconductor => {
                hwc_engine::MaterialConductivity::Semiconductor
            }
            hwc_parser::MaterialCategory::Insulator => {
                hwc_engine::MaterialConductivity::Insulator
            }
        };
        let process = match mat_def.process {
            hwc_parser::ManufacturingProcess::DrilledPlated => {
                hwc_engine::ManufacturingProcess::DrilledPlated
            }
            hwc_parser::ManufacturingProcess::Deposited => {
                hwc_engine::ManufacturingProcess::Deposited
            }
            hwc_parser::ManufacturingProcess::Etched => {
                hwc_engine::ManufacturingProcess::Etched
            }
        };
        material_registry.register_with_properties(&name, conductivity, process);

        // Extract and store physical properties for thermal/electrical calculations
        let mut resistivity_ohm_m: Option<f64> = None;
        let mut thermal_conductivity_w_mk: Option<f64> = None;
        for prop in &mat_def.properties {
            match prop.key.as_str() {
                "resistivity" => {
                    if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
                        // Value is already in base units (e.g. 2.82e-8ohm_m -> 2.82e-8)
                        resistivity_ohm_m = Some(m.value);
                    }
                }
                "thermal_conductivity" => {
                    if let hwc_parser::PropertyValue::Measurement(m) = &prop.value {
                        thermal_conductivity_w_mk = Some(m.value);
                    } else if let hwc_parser::PropertyValue::Number(v) = prop.value {
                        thermal_conductivity_w_mk = Some(v);
                    }
                }
                _ => {}
            }
        }
        if let (Some(rho), Some(k)) = (resistivity_ohm_m, thermal_conductivity_w_mk) {
            if let Some(id) = material_registry.get_id(&name) {
                material_registry.set_physical_props(id, rho, k);
            }
        }
    }

    // Determine space view orientation (v0.1.6)
    let space_view = if let Some(render) = &space_def.render {
        if let Some(view) = &render.view {
            match view.as_str() {
                "vertical" | "vertical_standing" => hwc_engine::SpaceView::Vertical,
                _ => hwc_engine::SpaceView::Horizontal,
            }
        } else {
            hwc_engine::SpaceView::Horizontal
        }
    } else {
        hwc_engine::SpaceView::Horizontal
    };

    // Create hardware space
    let mut space = HardwareSpace::new(
        space_def.name.to_string().into(),
        dims,
        AIR_MATERIAL_ID, // Default substrate material, will be set if substrate specified
        material_registry,
        space_view,
        resolution_nm,
    );

    // Load fabrication constraints from profile (v0.1.6: DRC Integration)
    if let Some(profile_name) = &space_def.profile {
        // Look up profile in symbol table
        let profile_def = symbol_table.get_profile(&profile_name.name).map_err(|_e| {
            IrError::ProfileNotFound { name: profile_name.name.clone().into() }
        })?;

        let constraints = profile_to_constraints(profile_def, symbol_table).map_err(|_e| {
            IrError::ProfileNotFound { name: profile_name.name.clone().into() }
        })?;

        space.fabrication_constraints = Some(constraints);
    }

    // Process net classifications (v0.1.6)
    for net_decl in &space_def.nets {
        let classification = match net_decl.classification {
            hwc_parser::NetClassification::Power => hwc_engine::space::NetClassification::Power,
            hwc_parser::NetClassification::Ground => hwc_engine::space::NetClassification::Ground,
            hwc_parser::NetClassification::Signal => hwc_engine::space::NetClassification::Signal,
            hwc_parser::NetClassification::HighVoltage => {
                hwc_engine::space::NetClassification::HighVoltage
            }
            hwc_parser::NetClassification::Unclassified => {
                hwc_engine::space::NetClassification::Unclassified
            }
        };
        space.set_net_classification(net_decl.name.clone(), classification);

        // v0.1.7: Set net frequency on the netlist (for SI-aware routing)
        if let Some(freq_hz) = net_decl.frequency_hz {
            if let Some(net_id) = space.netlist.get_net_by_name(&net_decl.name) {
                space.netlist.set_net_frequency(net_id, freq_hz);
            }
        }
    }

    Ok(space)
}

/// Validate ASIC-specific constraints (No Implicit Defaults rule).
///
/// When technology is "ASIC", the compiler must NEVER silently fall back to
/// PCB-scale defaults. Every route width, material property, and physical
/// constraint must be explicitly declared. If missing, the build halts
/// with a clear error instead of generating a physically incorrect layout.
pub fn validate_asic_constraints(
    space_def: &SpaceDefinition,
    symbol_table: &crate::SymbolTable,
) -> Result<(), IrError> {
    // Check if this is an ASIC build
    let profile = space_def
        .profile
        .as_ref()
        .and_then(|p| symbol_table.get_profile(p.as_str()).ok());

    let is_asic = profile.as_ref().is_some_and(|p| p.is_asic())
        || space_def.profile.as_ref().map_or(false, |p| {
            p.name.to_lowercase().contains("asic")
        });

    if !is_asic {
        return Ok(()); // PCB builds allow implicit defaults
    }

    // Rule 1: Profile MUST declare trace constraints with min_width
    let has_trace_constraints = profile
        .as_ref()
        .and_then(|p| p.trace.as_ref())
        .is_some();

    if !has_trace_constraints {
        return Err(IrError::MissingAsicConstraint {
            message: "ASIC profile missing required 'trace:' constraints".into(),
            hint: "Add a 'trace:' block to your profile with explicit min_width and min_spacing.\n\nExample:\n  trace:\n    min_width: 180nm\n    min_spacing: 200nm".into(),
        });
    }

    // Rule 2: Every route must have an explicit width
    for statement in &space_def.statements {
        if let hwc_parser::SpaceTopLevelStatement::Route(route) = statement {
            if route.width.is_none() {
                let net_hint = format!(
                    "Route {:?} -> {:?} has no explicit width. Add 'width: <value>' to the route definition.",
                    route.from, route.to
                );
                return Err(IrError::MissingAsicConstraint {
                    message: format!(
                        "ASIC route {:?} -> {:?} lacks an explicit width constraint.",
                        route.from, route.to
                    ),
                    hint: net_hint,
                });
            }
        }
    }

    // Rule 3: Every material used in pours must have physical properties declared
    let declared_materials: Vec<String> = symbol_table
        .materials()
        .iter()
        .map(|(name, _)| name.to_string())
        .collect();

    for statement in &space_def.statements {
        if let hwc_parser::SpaceTopLevelStatement::Pour(pour) = statement {
            if !declared_materials.iter().any(|m| m == &pour.material) {
                return Err(IrError::MissingAsicConstraint {
                    message: format!(
                        "ASIC pour '{}' references undeclared material '{}'.",
                        pour.name, pour.material
                    ),
                    hint: format!(
                        "Material '{}' must be declared with full physical properties (resistivity, thermal_conductivity, density) before use in ASIC designs.\n\nExample:\n  material {}:\n      category: conductor\n      properties:\n          resistivity: 1.68e-8ohm_m\n          thermal_conductivity: 401.0W_mK\n          density: 8960.0kg_m3",
                        pour.material, pour.material
                    ),
                });
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hwc_diagnostics::DiagnosticCollector;
    use hwc_parser::{Lexer, Parser};

    fn parse(source: &str) -> Result<hwc_parser::Program, String> {
        let collector = DiagnosticCollector::new(source, 20);
        let lexer = Lexer::new(source);
        let tokens = lexer
            .tokenize()
            .map_err(|e| format!("Lex error: {:?}", e))?;
        let mut parser = Parser::new(tokens);
        let program = parser.parse(&collector);

        if collector.has_errors() {
            return Err("Parse errors occurred".into());
        }

        Ok(program)
    }

    fn get_space(program: &hwc_parser::Program) -> &hwc_parser::SpaceDefinition {
        program
            .definitions
            .iter()
            .find_map(|def| {
                if let hwc_parser::Definition::Space(space) = def {
                    Some(space)
                } else {
                    None
                }
            })
            .expect("No space definition found in program")
    }

    #[test]
    fn test_create_hardware_space() {
        let source = r#"space Test:
    dimensions: 50mm by 50mm by 4mm
    resolution: 100um
"#;

        let program = parse(source).expect("Failed to parse");
        let space_def = get_space(&program);
        let symbol_table = crate::SymbolTable::new();

        let space = create_hardware_space(space_def, &symbol_table).unwrap();
        assert_eq!(space.name, "Test");
        assert_eq!(space.dimensions.width_nm, 50_000_000);
        // Resolution is 100um (100_000 nm)
        assert_eq!(space.resolution_nm, 100_000);
    }
}
