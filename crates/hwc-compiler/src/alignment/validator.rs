//! Alignment Validator - Progressive Trigger Integration
//!
//! This module implements the "Progressive Trigger" that enables Artist Mode
//! and Professional Mode based on the presence of the `implements` keyword.

use crate::alignment::{AlignmentError, GraphMatcher, LogicalSynthesizer, PhysicalNetlist};
use crate::SymbolTable;
use compact_str::CompactString;
use hwc_parser::SpaceDefinition;

/// Result of alignment validation
#[derive(Debug)]
pub enum AlignmentResult {
    /// Artist Mode: No `implements` clause, validation skipped
    Skipped { reason: CompactString },
    /// Professional Mode: Validation passed
    Passed {
        physical_device_count: usize,
        logical_device_count: usize,
    },
    /// Professional Mode: Validation failed
    Failed { error: AlignmentError },
}

impl AlignmentResult {
    pub fn is_success(&self) -> bool {
        matches!(
            self,
            AlignmentResult::Skipped { .. } | AlignmentResult::Passed { .. }
        )
    }

    pub fn is_failure(&self) -> bool {
        matches!(self, AlignmentResult::Failed { .. })
    }
}

/// Alignment Validator - Progressive Trigger
///
/// This struct orchestrates the alignment validation process:
/// 1. Checks if `implements` clause is present
/// 2. If absent: Artist Mode (skip validation)
/// 3. If present: Professional Mode (enforce validation)
pub struct AlignmentValidator;

impl AlignmentValidator {
    /// Perform alignment validation based on the `implements` clause
    ///
    /// # Arguments
    /// * `space_def` - The parsed space definition (contains `implements_module`)
    /// * `physical_netlist` - The physical netlist extracted from geometry
    /// * `symbol_table` - The symbol table (contains module definitions)
    /// * `space` - The hardware space (for spatial error reporting)
    /// * `tolerance` - Parameter tolerance (default: 0.01 = 1%)
    ///
    /// # Returns
    /// * `Ok(AlignmentResult)` - Validation result (skipped, passed, or failed)
    /// * `Err(AlignmentError)` - Critical error during validation process
    pub fn validate(
        space_def: &SpaceDefinition,
        physical_netlist: &PhysicalNetlist,
        symbol_table: &SymbolTable,
        space: &hwc_engine::HardwareSpace,
        tolerance: Option<f64>,
    ) -> Result<AlignmentResult, AlignmentError> {
        // Check if `implements` clause is present
        match &space_def.implements_module {
            None => {
                // Artist Mode: No `implements` clause
                Ok(AlignmentResult::Skipped {
                    reason: "No 'implements' clause - Artist Mode enabled".into(),
                })
            }
            Some(module_name) => {
                // Professional Mode: `implements` clause present
                Self::validate_professional_mode(
                    module_name,
                    physical_netlist,
                    symbol_table,
                    space,
                    tolerance,
                )
            }
        }
    }

    /// Professional Mode: Enforce alignment validation
    fn validate_professional_mode(
        module_name: &str,
        physical_netlist: &PhysicalNetlist,
        symbol_table: &SymbolTable,
        space: &hwc_engine::HardwareSpace,
        tolerance: Option<f64>,
    ) -> Result<AlignmentResult, AlignmentError> {
        // Step 1: Look up the module definition
        let module_def =
            symbol_table
                .get_module(module_name)
                .map_err(|_| AlignmentError::ModuleNotFound {
                    module_name: module_name.into(),
                })?;

        // Step 2: Synthesize logical netlist from module definition
        let mut logical_synthesizer = LogicalSynthesizer::new();
        let logical_netlist = logical_synthesizer.synthesize(module_def)?;

        // Step 3: Get device type registry from logical synthesizer
        let device_registry = logical_synthesizer.device_registry();

        // Step 4: Perform graph isomorphism comparison
        let tolerance_value = tolerance.unwrap_or(0.01); // Default: 1% tolerance
        let matcher = GraphMatcher::new(&logical_netlist, physical_netlist, device_registry, space)
            .with_tolerance(tolerance_value);
        matcher.verify_isomorphism()?;

        // Step 5: Validation passed!
        Ok(AlignmentResult::Passed {
            physical_device_count: physical_netlist.devices.len(),
            logical_device_count: logical_netlist.devices.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_artist_mode_skips_validation() {
        // This test verifies that Artist Mode correctly skips validation
        // when no `implements` clause is present

        // TODO: Implement test once we have test fixtures
        // For now, this is a placeholder to document expected behavior
    }

    #[test]
    fn test_professional_mode_validates() {
        // This test verifies that Professional Mode enforces validation
        // when `implements` clause is present

        // TODO: Implement test once we have test fixtures
        // For now, this is a placeholder to document expected behavior
    }
}
