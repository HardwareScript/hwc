//! `hwc eval` Command
//!
//! Quick interactive expression evaluator (like `node -e` or `python -c`)
//! or comptime function evaluator.
//!
//! Example:
//!   `hwc eval "4.0um / 1.41um * 350.0"` -> `992.9078 Ohm`
//!   `hwc eval "1 + 1"` -> `2`
//!   `hwc eval file.hw --fn test_math`

use compact_str::CompactString;
use miette::Result;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub fn execute(target: String, target_fn: Option<CompactString>, verbose: bool) -> Result<()> {
    let start_time = Instant::now();

    let target_path = Path::new(&target);
    // If target is an existing file or ends in .hw, execute as a file/script
    if target_path.exists() || target.ends_with(".hw") {
        return super::run_cmd::execute(PathBuf::from(target), target_fn, verbose);
    }

    // Otherwise, treat as an inline expression string
    let unit_registry = hwc_stdlib::load_stdlib_registry()
        .unwrap_or_else(|_| hwc_types::UnitRegistry::new(vec![]));

    let result = hwc_compiler::eval_expression_str(&target, Some(&unit_registry))
        .map_err(|e| miette::miette!("Expression evaluation failed: {}", e))?;

    println!("{}", result);

    if verbose {
        let elapsed_ms = start_time.elapsed().as_secs_f64() * 1000.0;
        eprintln!("✅ Evaluation completed in {:.2}ms", elapsed_ms);
    }

    Ok(())
}
