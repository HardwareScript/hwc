use hwc_compiler::{program_to_space, SymbolTable};
use hwc_engine::design_rule_check::{DesignRuleChecker, NetVoxels};
use hwc_parser::{Lexer, Parser};
use miette::Result;
use std::path::PathBuf;

/// Execute design rule check on a hardware design.
///
/// This command runs only the DRC validation phase without rebuilding
/// the entire design. Useful for quick validation during iteration.
///
/// # Arguments
/// * `input` - Path to .hw source file
/// * `build_dir` - Path to build directory (currently unused, for future use)
///
/// # Returns
/// Ok if DRC passes, error with detailed violations if DRC fails
pub fn execute(input: PathBuf, _build_dir: PathBuf) -> Result<()> {
    println!("🔍 DESIGN RULE CHECK");
    println!("==================================================\n");

    // Read source
    let source = std::fs::read_to_string(&input)
        .map_err(|e| miette::miette!("Failed to read file: {}", e))?;

    // Lex
    let lexer = Lexer::new(&source);
    let tokens = lexer
        .tokenize()
        .map_err(|e| miette::miette!("Lexer error: {}", e))?;

    // Parse with diagnostic collector
    let collector =
        hwc_compiler::DiagnosticCollector::new_with_file(&source, &input.to_string_lossy(), 20);
    let mut parser = Parser::new(tokens);
    let ast = parser.parse(&collector);

    if collector.has_errors() {
        eprintln!("❌ Syntax errors:");
        collector.print_all();
        eprintln!("\n{}", collector.summary());
        return Err(miette::miette!("Syntax errors found"));
    }

    // Transform AST to hardware space (includes routing)
    let symbol_table = SymbolTable::new();
    let space = program_to_space(&ast, &symbol_table, &collector)
        .map_err(|e| miette::miette!("Failed to create hardware space: {}", e))?;

    // Collect nets from voxel grid
    println!("📊 Analyzing design...");
    let nets = collect_nets_from_space(&space);

    if nets.is_empty() {
        println!("⚠️  No nets found in design");
        println!("\n✅ DRC COMPLETE (no nets to check)");
        return Ok(());
    }

    println!("   Found {} net(s)", nets.len());

    // Run DRC validation
    println!("\n🔬 Running design rule checks...");

    let drc_checker = DesignRuleChecker::default();
    let constraint_rulebook =
        hwc_engine::constraint_manager::ConstraintRulebook::new(space.voxel_size.x_nm);

    let report = drc_checker.check(&nets, &constraint_rulebook, space.voxel_size.x_nm);

    // Display results
    println!("\n📋 DRC RESULTS");
    println!("==================================================");

    if report.is_valid() {
        println!("✅ All checks passed!");
        println!("\nChecks performed:");
        println!("  ✓ Clearance violations (P16)");
        println!("  ✓ Trace width violations (P21)");
        println!("  ✓ Thermal violations (P22)");
        println!("\n✅ DRC COMPLETE - Design is valid");
        Ok(())
    } else {
        println!("❌ {} violation(s) found\n", report.violations.len());

        for (i, violation) in report.violations.iter().enumerate() {
            println!("{}. {}", i + 1, violation);
        }

        Err(miette::miette!(
            "DRC failed with {} violation(s)",
            report.violations.len()
        ))
    }
}

/// Collect nets from hardware space voxel grid
fn collect_nets_from_space(space: &hwc_engine::HardwareSpace) -> Vec<NetVoxels> {
    use rustc_hash::FxHashMap;

    let mut net_map: FxHashMap<u32, Vec<hwc_engine::Point3D>> = FxHashMap::default();

    // Scan voxel grid for occupied voxels
    let (x_size, y_size, z_size) = space.voxel_grid.size();

    for x in 0..x_size {
        for y in 0..y_size {
            for z in 0..z_size {
                let net_id = space.voxel_grid.get_net(x, y, z);

                // Skip empty voxels (net_id == 0)
                if net_id > 0 {
                    let point = space.voxel_to_position(x, y, z);
                    net_map.entry(net_id).or_default().push(point);
                }
            }
        }
    }

    // Convert to NetVoxels
    net_map
        .into_iter()
        .map(|(net_id, voxels)| {
            let net_name = space
                .netlist
                .get_net(hwc_engine::netlist::NetId::new(net_id))
                .map(|n| n.name.clone())
                .unwrap_or_else(|| format!("Net_{}", net_id).into());

            let classification = space
                .net_classifications
                .get(&net_name)
                .copied()
                .unwrap_or(hwc_engine::space::NetClassification::Unclassified);

            NetVoxels {
                net_name,
                voxels,
                geometry_type: hwc_engine::design_rule_check::GeometryType::Trace, // Standalone DRC assumes traces
                classification,
            }
        })
        .collect()
}
