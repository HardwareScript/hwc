use compact_str::CompactString;
use hwc_engine::space::PourMetadata;
use rustc_hash::FxHashMap;

use super::error::DeviceExtractionError;
use super::DeviceExtractor;

impl<'a> DeviceExtractor<'a> {
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
