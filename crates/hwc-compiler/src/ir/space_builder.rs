//! Hardware space creation from space definitions.

use super::conversions::measurement_to_nm;
use super::errors::IrError;
use crate::conversions::profile_to_constraints;
use hwc_engine::{Dimensions, GridCells, HardwareSpace, MaterialRegistry, AIR_MATERIAL_ID};
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

    let grid = space_def.grid.as_ref().ok_or(IrError::MissingGrid)?;

    // Convert dimensions to nanometers using the symbol table (supports custom units!)
    let dims = Dimensions {
        width_nm: measurement_to_nm(&dimensions.width, symbol_table),
        height_nm: measurement_to_nm(&dimensions.height, symbol_table),
        depth_nm: measurement_to_nm(&dimensions.depth, symbol_table),
    };

    // Create grid cells
    let grid_cells = GridCells::new(grid.x, grid.y, grid.z);

    // Create material registry
    let material_registry = MaterialRegistry::new();

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

    // Create hardware space (VoxelGrid and NetlistArena are created inside)
    let mut space = HardwareSpace::new(
        space_def.name.to_string().into(),
        dims,
        grid_cells,
        AIR_MATERIAL_ID, // Default substrate material, will be set if substrate specified
        material_registry,
        space_view,
    );

    // Load fabrication constraints from profile (v0.1.6: DRC Integration)
    if let Some(profile_name) = &space_def.profile {
        // Look up profile in symbol table
        let profile_def = symbol_table.get_profile(&profile_name.name).map_err(|e| {
            IrError::CompilationError(format!(
                "Profile '{}' not found: {:?}",
                profile_name.name, e
            ))
        })?;

        let constraints = profile_to_constraints(profile_def, symbol_table).map_err(|e| {
            IrError::CompilationError(format!(
                "Failed to convert profile '{}' to constraints: {:?}",
                profile_name.name, e
            ))
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
    }

    Ok(space)
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
    grid: 500 by 500 by 4
"#;

        let program = parse(source).expect("Failed to parse");
        let space_def = get_space(&program);
        let symbol_table = crate::SymbolTable::new();

        let space = create_hardware_space(space_def, &symbol_table).unwrap();
        assert_eq!(space.name, "Test");
        assert_eq!(space.dimensions.width_nm, 50_000_000);
        assert_eq!(space.grid.x_cols, 500);
        assert_eq!(space.voxel_size.x_nm, 100_000);
    }
}
