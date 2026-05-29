//! Alignment Layer - The Triple-Check Architecture
//!
//! This module implements Hardware Script's "Correct-by-Construction" validation system
//! that replaces traditional LVS (Layout vs. Schematic) verification.
//!
//! # Why Traditional LVS is Redundant in Hardware Script
//!
//! In legacy EDA tools (Cadence, Altium), LVS is a "post-mortem autopsy":
//! - You design a body (layout)
//! - You have a soul (schematic)
//! - Then you run LVS to see if the soul fits the body
//!
//! Hardware Script uses "Silent Atoms" and "Voxel Stamping" for **Correct-by-Construction**:
//! - The `implements` keyword enforces that layout follows the module
//! - The router only routes what the module says (mathematically impossible to create mismatches)
//! - Physical continuity is validated at the voxel level (not abstract netlists)
//!
//! # The Triple-Check Architecture
//!
//! Instead of graph isomorphism (CPU-intensive, redundant), we use three lightweight checks:
//!
//! ## Layer 1: Symbolic Alignment (Symbol Table)
//! - Check if every device name in the module exists in the space
//! - Verify device types match (NMOS vs PMOS vs Resistor)
//! - Fast fail on missing or mismatched devices
//! - **Performance**: O(devices) hash table lookup
//!
//! ## Layer 2: Physical Continuity (Island Builder - Sprint 2.3)
//! - Verify each net forms a single conductive island
//! - Catch disconnected copper blocks with same net label
//! - Detect short circuits (multiple nets on one island)
//! - Detect floating conductors (islands with no pins)
//! - **Performance**: O(voxels) flood-fill, runs once
//!
//! ## Layer 3: Device Extraction (Parameter Validation)
//! - Extract physical parameters from geometry (W/L, R, C)
//! - Compare extracted values to module specification
//! - Verify device signatures match (NMOS vs Resistor)
//! - Validate port mapping (physical entry points match logical pins)
//! - **Performance**: O(devices) parameter comparison
//!
//! # Example
//!
//! ```hardware
//! module Inverter:
//!     pin VIN: input
//!     pin VOUT: output
//!     pin VDD: power
//!     pin GND: ground
//!     
//!     add NMOS(W: 10um, L: 1um) named M1
//!     add PMOS(W: 20um, L: 1um) named M2
//!     
//!     route M1.gate to VIN
//!     route M1.source to GND
//!     route M1.drain to VOUT
//!     route M1.bulk to GND
//!     
//!     route M2.gate to VIN
//!     route M2.source to VDD
//!     route M2.drain to VOUT
//!     route M2.bulk to VDD
//!
//! space Inverter_Layout implements Inverter:
//!     # Physical layout must match module definition
//!     # Alignment Layer validates this automatically
//! ```
//!
//! The Alignment Layer will:
//! 1. **Layer 1**: Check M1 and M2 exist in layout with correct types ✓
//! 2. **Layer 2**: Verify VIN, VOUT, VDD, GND form single islands ✓
//! 3. **Layer 3**: Verify M1 has W=10um±1%, M2 has W=20um±1% ✓
//!
//! # Why This is Better Than Traditional LVS
//!
//! - **No Graph Isomorphism**: Router follows module, so topology is guaranteed correct
//! - **Physics-Aware**: Validates actual voxel connectivity, not abstract netlists
//! - **Faster**: O(devices + voxels) vs O(devices² × nets²) for graph matching
//! - **Better Errors**: Shows physical locations and gaps, not just "net mismatch"
//! - **Correct-by-Construction**: Catches errors during routing, not after

pub mod report;

pub use report::{AlignmentReport, AlignmentViolation};

// TODO: AlignmentValidator implementation
// The validator will integrate:
// - Layer 1: Symbolic checks (device names, types) - THIS FILE
// - Layer 2: Physical Continuity (Sprint 2.3 Island Builder) - ALREADY EXISTS
// - Layer 3: Device Extraction (Gap 2 Parameter Validation) - ALREADY EXISTS
//
// The Alignment Layer is a thin wrapper that runs these three checks in sequence
// and generates a unified AlignmentReport.

/*
/// Alignment Validator - Integrates the Triple-Check Architecture
pub struct AlignmentValidator {
    /// Physical netlist extracted from layout
    physical_netlist: PhysicalNetlist,

    /// Logical netlist synthesized from module
    logical_graph: LogicalGraph,
}

impl AlignmentValidator {
    /// Create a new Alignment Validator
    ///
    /// # Arguments
    /// * `physical_netlist` - Extracted from voxel grid via DeviceExtractor
    /// * `module` - Module definition containing logical schematic
    /// * `symbol_table` - Symbol table for unit conversion (optional)
    pub fn new(
        physical_netlist: PhysicalNetlist,
        module: &ModuleDefinition,
        symbol_table: Option<&crate::SymbolTable>,
    ) -> Self {
        let logical_graph = LogicalGraph::from_module(module, symbol_table);

        Self {
            physical_netlist,
            logical_graph,
        }
    }

    /// Run Alignment Layer validation
    ///
    /// Returns an Alignment Report with pass/fail status and detailed violations
    pub fn validate(&self) -> AlignmentReport {
        let mut violations = Vec::new();

        // Layer 1: Symbolic Alignment
        self.check_device_count(&mut violations);
        self.check_device_types(&mut violations);

        // Layer 2: Physical Continuity
        // (Already run in build pipeline - Sprint 2.3)
        // If we reach here, Physical Continuity passed

        // Layer 3: Device Extraction
        self.check_device_parameters(&mut violations);

        AlignmentReport {
            passed: violations.is_empty(),
            violations,
            physical_device_count: self.physical_netlist.devices.len(),
            logical_device_count: self.logical_graph.devices.len(),
            physical_net_count: self.physical_netlist.nets.len(),
            logical_net_count: self.logical_graph.nets.len(),
        }
    }

    /// Layer 1: Check device count matches
    fn check_device_count(&self, violations: &mut Vec<AlignmentViolation>) {
        let physical_device_count = self.physical_netlist.devices.len();
        let logical_device_count = self.logical_graph.devices.len();

        if physical_device_count != logical_device_count {
            violations.push(AlignmentViolation::DeviceCountMismatch {
                physical_count: physical_device_count,
                logical_count: logical_device_count,
            });
        }
    }

    /// Layer 1: Check device types match
    fn check_device_types(&self, violations: &mut Vec<AlignmentViolation>) {
        // Build device type maps
        let mut physical_types: FxHashMap<CompactString, CompactString> = FxHashMap::default();
        for device in &self.physical_netlist.devices {
            let device_type = self
                .physical_netlist
                .device_registry
                .get_name(device.device_type_id)
                .unwrap_or("UNKNOWN");
            physical_types.insert(device.name.clone(), device_type.into());
        }

        let mut logical_types: FxHashMap<CompactString, CompactString> = FxHashMap::default();
        for (device_name, device_type) in &self.logical_graph.devices {
            logical_types.insert(device_name.clone(), device_type.clone());
        }

        // Check each logical device has matching physical device
        for (device_name, logical_type) in &logical_types {
            match physical_types.get(device_name) {
                Some(physical_type) if physical_type == logical_type => {
                    // Match! Continue
                }
                Some(physical_type) => {
                    violations.push(AlignmentViolation::DeviceTypeMismatch {
                        device_name: device_name.clone(),
                        physical_type: physical_type.clone(),
                        logical_type: logical_type.clone(),
                    });
                }
                None => {
                    violations.push(AlignmentViolation::MissingPhysicalDevice {
                        device_name: device_name.clone(),
                        device_type: logical_type.clone(),
                    });
                }
            }
        }

        // Check for extra physical devices not in schematic
        for (device_name, physical_type) in &physical_types {
            if !logical_types.contains_key(device_name) {
                violations.push(AlignmentViolation::ExtraPhysicalDevice {
                    device_name: device_name.clone(),
                    device_type: physical_type.clone(),
                });
            }
        }
    }

    /// Layer 3: Check device parameters match within tolerance
    ///
    /// Tolerance is specified per-device-type in device contracts (.hw files).
    /// Default tolerance: 1% if not specified in contract.
    ///
    /// # Example
    /// ```
    /// Physical: W=10.0um, L=1.0um
    /// Logical:  W=10.1um, L=1.0um
    /// Tolerance (from device contract): W: 1%, L: 1%
    /// Result: PASS (0.1um difference is within 1% of 10um)
    /// ```
    fn check_device_parameters(&self, violations: &mut Vec<AlignmentViolation>) {
        const DEFAULT_TOLERANCE: f64 = 0.01; // 1% default if not in contract

        for device_name in self.logical_graph.devices.keys() {
            // Find corresponding physical device
            let physical_device = match self
                .physical_netlist
                .devices
                .iter()
                .find(|d| d.name == device_name)
            {
                Some(d) => d,
                None => continue, // Already reported as missing device
            };

            // Get logical parameters (if specified)
            let logical_params = match self.logical_graph.parameters.get(device_name) {
                Some(params) => params,
                None => continue, // No parameters specified in schematic - skip validation
            };

            // Check each parameter
            for (param_name, &logical_value) in logical_params {
                let physical_value = match physical_device.parameters.get(param_name) {
                    Some(&v) => v,
                    None => {
                        violations.push(AlignmentViolation::MissingParameter {
                            device_name: device_name.clone(),
                            parameter: param_name.clone(),
                        });
                        continue;
                    }
                };

                // Get tolerance from device contract (future: parse from .hw file)
                // For now, use sensible defaults based on parameter type
                let tolerance = match param_name.as_str() {
                    "W" | "L" => 0.01,      // 1% for critical dimensions
                    "AS" | "AD" => 0.05,    // 5% for areas (extraction is less precise)
                    "PS" | "PD" => 0.05,    // 5% for perimeters (extraction is less precise)
                    _ => DEFAULT_TOLERANCE, // 1% for other parameters
                };

                // Calculate relative error
                let relative_error = if logical_value != 0.0 {
                    ((physical_value - logical_value) / logical_value).abs()
                } else {
                    // If logical value is 0, check absolute difference
                    physical_value.abs()
                };

                // Check if within tolerance
                if relative_error > tolerance {
                    violations.push(AlignmentViolation::ParameterMismatch {
                        device_name: device_name.clone(),
                        parameter: param_name.clone(),
                        physical_value,
                        logical_value,
                        tolerance,
                        relative_error,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alignment_validator_creation() {
        // This is a placeholder test - real tests will be in integration tests
        // with actual hardware designs
    }
}
*/
