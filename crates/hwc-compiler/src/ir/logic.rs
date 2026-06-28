//! Logic synthesis integration - NATIVE direct mutation of HardwareSpace.
//!
//! The logic synthesizer directly mutates HardwareSpace with NO intermediate
//! representations. This is the true native architecture.

use super::errors::IrError;
use crate::logic_synthesizer::LogicSynthesizer;
use crate::SymbolTable;
use hwc_diagnostics::DiagnosticCollector;
use hwc_engine::HardwareSpace;
use hwc_parser::logic::LogicBlock;

/// Synthesize a logic block directly into the hardware space.
///
/// NATIVE ARCHITECTURE: The synthesizer directly mutates the space.
/// No intermediate representations, no conversion layers, no bridges.
///
/// The synthesizer takes `&mut HardwareSpace` and places components/nets
/// directly into it. This function just calls the synthesizer and reports
/// warnings - there is NO conversion or transformation happening here.
pub fn synthesize_and_place_logic(
    space: &mut HardwareSpace,
    logic_block: &LogicBlock,
    module_pins: &[(String, Option<usize>)],
    symbol_table: &SymbolTable,
    collector: &DiagnosticCollector,
) -> Result<(), IrError> {
    // Create native logic synthesizer that directly mutates the space
    let mut synthesizer = LogicSynthesizer::new(space, symbol_table);

    // Synthesize directly into the space
    // The synthesizer mutates space in-place, returns only warnings
    let warnings = synthesizer
        .synthesize_logic_block(collector, logic_block, module_pins)
        .map_err(|e| IrError::LogicSynthesisFailed { message: e.to_string() })?;

    // Report warnings to user
    for warning in warnings {
        eprintln!("⚠️  Logic synthesis warning: {}", warning);
    }

    // Check for synthesis errors in collector
    if collector.has_errors() {
        return Err(IrError::LogicSynthesisFailed { message: "Logic synthesis failed".into() });
    }

    // Done - space has been mutated directly, nothing to return
    Ok(())
}
