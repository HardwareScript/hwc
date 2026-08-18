//! Parameter Extraction Registry - Universal Strongly-Typed Extraction
//!
//! ZERO-MAGIC ARCHITECTURE:
//! - All parameter extraction is evaluated through strongly-typed geometric and physical models
//! - Eliminates ad-hoc string matching and axis-guessing heuristics
//! - Driven directly by the device definition's declarative `metrics:` contract

use compact_str::CompactString;
use hwc_engine::space::PourMetadata;
use hwc_engine::HardwareSpace;
use rustc_hash::FxHashMap;

use super::metrics::DeviceGeometryContext;

/// Universal parameter extraction dispatcher
///
/// Dispatches metric evaluation through the strongly-typed `DeviceGeometryContext`.
/// Returns strongly-typed PhysicalQuantity values with zero string matching.
pub fn extract_parameters_universal(
    device_type: &str,
    terminal_pours: &FxHashMap<CompactString, Vec<PourMetadata>>,
    space: &HardwareSpace,
    symbol_table: &hwc_compiler::SymbolTable,
) -> Result<FxHashMap<CompactString, hwc_engine::PhysicalQuantity>, String> {
    let device_def = symbol_table
        .get_device(device_type)
        .map_err(|e| format!("Device definition '{}' not found: {}", device_type, e))?;

    let spice_info = device_def
        .spice_info
        .as_ref()
        .ok_or_else(|| format!("Device '{}' is missing required 'spice:' block", device_type))?;

    let context = DeviceGeometryContext::new(device_type, terminal_pours, Some(space))?;
    let mut results = FxHashMap::default();

    // Strongly-typed topological extraction driven strictly by device metrics: contract
    if let Some(ref metrics) = device_def.metrics {
        let all_evaluated = context.evaluate_all_metrics(metrics)?;
        for param_name in &spice_info.parameters {
            let quantity = all_evaluated.get(param_name).copied().ok_or_else(|| {
                format!(
                    "Device '{}' requires SPICE parameter '{}' but no corresponding metric is declared in 'metrics:' block",
                    device_type, param_name
                )
            })?;
            results.insert(param_name.clone(), quantity);
        }
    } else if !spice_info.parameters.is_empty() {
        return Err(format!(
            "Device '{}' requests parameters {:?} but has no 'metrics:' block defined in its contract",
            device_type, spice_info.parameters
        ));
    }

    Ok(results)
}
