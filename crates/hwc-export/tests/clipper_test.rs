#[cfg(test)]
mod tests {
    use clipper2_rust::core::FillRule;
    use clipper2_rust::{Path64, Point64};

    #[test]
    fn test_coreldraw_boolean_handshake() {
        // 1. Define Shape A: A 10mm x 10mm square (coordinates in nanometers)
        // Millimeters to nanometers: 10mm = 10,000,000 nm
        let square = vec![
            Point64::new(-5_000_000, -5_000_000),
            Point64::new(5_000_000, -5_000_000),
            Point64::new(5_000_000, 5_000_000),
            Point64::new(-5_000_000, 5_000_000),
        ];

        // 2. Define Shape B: A 5mm radius circle, offset to the right at X = 5mm
        let cx = 5_000_000;
        let cy = 0;
        let radius = 5_000_000;
        let segments = 32;

        let mut circle = Path64::new();
        for i in 0..segments {
            let angle = (i as f64 / segments as f64) * 2.0 * std::f64::consts::PI;
            let x = cx + (radius as f64 * angle.cos()) as i64;
            let y = cy + (radius as f64 * angle.sin()) as i64;
            circle.push(Point64::new(x, y));
        }

        let subjects = vec![square];
        let clips = vec![circle];

        // --- OPERATION 1: UNION (Weld) ---
        // v0.1.8: Use NonZero to ensure overlapping shapes merge into a solid mass
        let weld_result = clipper2_rust::union_64(&subjects, &clips, FillRule::NonZero);
        assert!(!weld_result.is_empty());
        assert_eq!(weld_result.len(), 1);
        println!(
            "Weld Success: Unified shape has {} vertices",
            weld_result[0].len()
        );

        // --- OPERATION 2: DIFFERENCE (Trim) ---
        // Difference still works correctly with NonZero subjects/clips
        let trim_result = clipper2_rust::difference_64(&subjects, &clips, FillRule::NonZero);
        assert!(!trim_result.is_empty());

        // The result should be a single chopped polygon
        assert_eq!(trim_result.len(), 1);
        println!(
            "Trim Success: Chopped shape has {} vertices",
            trim_result[0].len()
        );

        // Verify the bite mark: The point (5mm, 0) is inside the circle,
        // so it must have been removed from the square.
        for point in &trim_result[0] {
            // Ensure no vertex is sitting in the center of the deleted circle
            assert!(!(point.x == 5_000_000 && point.y == 0));
        }
    }

    #[test]
    fn test_print_substrate_layers() {
        use hwc_compiler::SymbolTable;
        let source = std::fs::read_to_string("tests/tutorial_examples/artist_two_pins.hw")
            .or_else(|_| std::fs::read_to_string("../tests/tutorial_examples/artist_two_pins.hw"))
            .or_else(|_| {
                std::fs::read_to_string("../../tests/tutorial_examples/artist_two_pins.hw")
            })
            .unwrap();
        let collector =
            hwc_diagnostics::DiagnosticCollector::new_with_file(&source, "artist_two_pins.hw", 20);
        let lexer = hwc_parser::Lexer::new(&source);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = hwc_parser::Parser::new(tokens);
        let program = parser.parse(&collector);

        let mut symbol_table = SymbolTable::new();
        // Load prelude
        let prelude = hwc_compiler::Prelude::load().unwrap();
        for unit in &prelude.units {
            symbol_table.register_prelude_unit(unit.clone());
        }
        for (name, value) in &prelude.constants {
            symbol_table.register_prelude_constant(name.clone(), *value);
        }

        let input_path = std::path::PathBuf::from("../tests/tutorial_examples/artist_two_pins.hw");
        let mut resolver = hwc_compiler::ModuleResolver::new().unwrap();
        for import in &program.imports {
            resolver
                .resolve_import(import, &input_path, &mut symbol_table)
                .unwrap();
        }
        for definition in &program.definitions {
            match definition {
                hwc_parser::Definition::Unit(unit) => {
                    symbol_table.register_unit(&collector, unit.clone());
                }
                hwc_parser::Definition::Material(mat) => {
                    symbol_table.register_material(&collector, mat.clone());
                }
                hwc_parser::Definition::Profile(profile) => {
                    symbol_table.register_profile(&collector, *profile.clone());
                }
                hwc_parser::Definition::Component(component) => {
                    symbol_table.register_component(&collector, component.clone());
                }
                hwc_parser::Definition::Module(module) => {
                    symbol_table.register_module(&collector, module.clone());
                }
                hwc_parser::Definition::Mechanical(mechanical) => {
                    symbol_table.register_mechanical(&collector, mechanical.clone());
                }
                hwc_parser::Definition::Interface(interface) => {
                    symbol_table.register_interface(&collector, interface.clone());
                }
                hwc_parser::Definition::Test(test) => {
                    symbol_table.register_test(&collector, test.clone());
                }
                _ => {}
            }
        }

        let mut space =
            hwc_compiler::program_to_space(&program, &symbol_table, &collector).unwrap();


        println!("SUBSTRATE LAYERS:");
        for (i, layer) in space.entity_graph.get_substrate_layers().iter().enumerate() {
            println!(
                "  Layer {}: material={}, net={}, type={:?}, shape={:?}, bbox=min:({:?}) max:({:?})",
                i,
                layer.material,
                layer.net,
                layer.layer_type,
                layer.shape,
                layer.bbox.min,
                layer.bbox.max
            );
        }
    }
}
