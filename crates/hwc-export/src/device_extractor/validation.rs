use compact_str::CompactString;
use hwc_engine::space::PourMetadata;
use rustc_hash::FxHashMap;

use super::error::DeviceExtractionError;
use super::DeviceExtractor;

impl<'a> DeviceExtractor<'a> {
    /// Validate bulk biasing based on material properties.
    ///
    /// Uses the unified material registry for conductivity checks.
    /// Full bias validation requires material properties from the symbol table;
    /// gracefully skips when properties are unavailable.
    pub(super) fn validate_bulk_biasing_from_material(
        &self,
        bulk_net: &str,
        _device_type_name: &str,
        _bulk_pour: Option<&PourMetadata>,
        _transistor_name: &str,
    ) -> Result<(), DeviceExtractionError> {
        let net_classification = self.space.get_net_classification(bulk_net);

        if matches!(
            net_classification,
            hwc_engine::space::NetClassification::Unclassified
        ) {
            return Err(DeviceExtractionError::BiasViolation {
                transistor: _transistor_name.to_string().into(),
                device_type_name: _device_type_name.to_string().into(),
                bulk_net: bulk_net.to_string().into(),
                expected_net: format!(
                    "Net '{}' is unclassified — add net_classifications to space",
                    bulk_net
                )
                .into(),
            });
        }

        // Full bias validation requires material properties (bias_requirement, doping_type)
        // from the symbol table. If available, validate against net classification.
        if let Some(pour) = _bulk_pour {
            if let Ok(mat_def) = self.symbol_table.get_material(&pour.material_name) {
                // Look up bias_requirement from material properties
                if let Some(bias_req_str) = mat_def
                    .properties
                    .iter()
                    .find(|p| p.key == "bias_requirement")
                    .and_then(|p| match &p.value {
                        hwc_parser::PropertyValue::String(s) => Some(s.as_str()),
                        _ => None,
                    })
                {
                    let bias_req = match bias_req_str.to_lowercase().as_str() {
                        "lowest_potential" | "ground" | "gnd" => {
                            hwc_materials::BiasRequirement::LowestPotential
                        }
                        "highest_potential" | "power" | "vdd" => {
                            hwc_materials::BiasRequirement::HighestPotential
                        }
                        _ => hwc_materials::BiasRequirement::None,
                    };

                    let materials_net_class = match net_classification {
                        hwc_engine::space::NetClassification::Power => {
                            hwc_materials::NetClassification::Power
                        }
                        hwc_engine::space::NetClassification::Ground => {
                            hwc_materials::NetClassification::Ground
                        }
                        hwc_engine::space::NetClassification::Signal => {
                            hwc_materials::NetClassification::Signal
                        }
                        hwc_engine::space::NetClassification::HighVoltage => {
                            hwc_materials::NetClassification::HighVoltage
                        }
                        hwc_engine::space::NetClassification::Unclassified => {
                            hwc_materials::NetClassification::Unclassified
                        }
                    };

                    if let Err(reason) = bias_req.validate_net_classification(materials_net_class) {
                        return Err(DeviceExtractionError::BiasViolation {
                            transistor: _transistor_name.to_string().into(),
                            device_type_name: format!(
                                "{} (bulk: {})",
                                _device_type_name, pour.material_name
                            )
                            .into(),
                            bulk_net: bulk_net.to_string().into(),
                            expected_net: reason.into(),
                        });
                    }
                }
            }
        }

        Ok(())
    }

    /// Validate device materials against device definition (Contract-Driven Validation)
    ///
    /// **Zero Hardcoded Assumptions**: This function does NOT assume what materials
    /// devices should use. It reads the device contract from the user's PDK file and
    /// validates that the physical layout matches the contract.
    ///
    /// The device contract (defined by the user) specifies allowed materials for each
    /// terminal. This validator ensures physical geometry respects those constraints.
    pub(super) fn validate_device_materials(
        &self,
        device_name: &str,
        device_type: &str,
        terminal_pours: &FxHashMap<CompactString, Vec<PourMetadata>>,
    ) -> Result<(), DeviceExtractionError> {
        // Try to get device definition from symbol table
        let device_def = match self.symbol_table.get_device(device_type) {
            Ok(def) => def,
            Err(_) => {
                // Device definition not found in symbol table
                // This is OK - validation is only possible when the contract is available
                println!(
                    "      ├─ Note: Device contract for '{}' not found in symbol table. \
                     Material validation skipped.",
                    device_type
                );
                return Ok(());
            }
        };

        // Convert device definition to contract for validation
        let contract = hwc_parser::DeviceContract::from_device_definition(device_def);

        // Validate EVERY pour bound to each terminal using the contract.
        // A terminal may legitimately have several pours (e.g. resistive channel +
        // contact pads); each one must independently satisfy the contract.
        let mut violations = Vec::new();
        for (terminal_name, pours) in terminal_pours {
            for pour in pours {
                if let Err(reason) =
                    contract.validate_terminal_material(terminal_name, &pour.material_name)
                {
                    violations.push(format!(
                        "  • Terminal '{}' (pour '{}'): {}",
                        terminal_name, pour.name, reason
                    ));
                }
            }
        }

        // If violations exist, construct a detailed error
        if !violations.is_empty() {
            return Err(DeviceExtractionError::InvalidGeometry {
                device_name: device_name.to_string().into(),
                device_type: device_type.to_string().into(),
                reason: format!(
                    "Device contract violation: Physical layout materials don't match contract.\n\
                     \n\
                     Device: {} (type: {})\n\
                     Contract: {} (defined in your PDK)\n\
                     \n\
                     Violations:\n\
                     {}\n\
                     \n\
                     How to fix:\n\
                     1. Check the device contract in your PDK file: 'device {}'\n\
                     2. Verify the 'materials:' block specifies allowed materials for each terminal\n\
                     3. Update your layout to use contract-compliant materials, OR\n\
                     4. Update the device contract if your physical design is correct\n\
                     \n\
                     The compiler enforces YOUR contract - it doesn't assume what materials should be.",
                    device_name,
                    device_type,
                    device_type,
                    violations.join("\n"),
                    device_type
                )
                .into(),
            });
        }

        println!(
            "      ├─ Material validation: All terminals match contract ✓"
        );
        Ok(())
    }
}
