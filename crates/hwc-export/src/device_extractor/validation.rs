use compact_str::CompactString;
use hwc_engine::space::PourMetadata;
use rustc_hash::FxHashMap;

use super::error::DeviceExtractionError;
use super::DeviceExtractor;

impl<'a> DeviceExtractor<'a> {
    /// Validate bulk biasing based on material properties (GAP 5)
    pub(super) fn validate_bulk_biasing_from_material(
        &self,
        bulk_net: &str,
        device_type_name: &str,
        bulk_pour: Option<&PourMetadata>,
        transistor_name: &str,
    ) -> Result<(), DeviceExtractionError> {
        // Get the bulk material from the pour
        let bulk_material = match bulk_pour {
            Some(pour) => &pour.material_name,
            None => {
                // No bulk pour - skip validation (will be caught by missing terminal check)
                return Ok(());
            }
        };

        // Look up material in database (case-insensitive)
        let semiconductor = match self
            .material_database
            .get_semiconductor(&bulk_material.to_lowercase())
        {
            Ok(semi) => semi,
            Err(_) => {
                // Material not in database - skip validation
                println!(
                    "   ⚠️  Warning: Material '{}' not found in database, skipping bias validation",
                    bulk_material
                );
                return Ok(());
            }
        };

        // Get bias requirement from material properties
        let bias_req = match &semiconductor.bias_requirement {
            Some(req) => req,
            None => {
                // No bias requirement - material doesn't need biasing (e.g., intrinsic)
                return Ok(());
            }
        };

        // Get net classification from space
        let net_classification = self.space.get_net_classification(bulk_net);

        // Check if net is classified
        if matches!(
            net_classification,
            hwc_engine::space::NetClassification::Unclassified
        ) {
            return Err(DeviceExtractionError::BiasViolation {
                transistor: transistor_name.to_string().into(),
                device_type_name: device_type_name.to_string().into(),
                bulk_net: bulk_net.to_string().into(),
                expected_net: format!(
                    "{:?} classification required by material {} (net '{}' is unclassified - add net_classifications to space)",
                    bias_req, bulk_material, bulk_net
                ).into(),
            });
        }

        // Convert hwc_engine::NetClassification to hwc_materials::NetClassification
        let materials_net_class = match net_classification {
            hwc_engine::space::NetClassification::Power => hwc_materials::NetClassification::Power,
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

        // Validate using the material's bias requirement method (data-driven!)
        if let Err(reason) = bias_req.validate_net_classification(materials_net_class) {
            return Err(DeviceExtractionError::BiasViolation {
                transistor: transistor_name.to_string().into(),
                device_type_name: format!(
                    "{} ({} bulk: {})",
                    device_type_name,
                    semiconductor
                        .doping_type
                        .as_ref()
                        .map(|dt| format!("{:?}", dt))
                        .unwrap_or_else(|| "unknown".to_string()),
                    bulk_material
                )
                .into(),
                bulk_net: bulk_net.to_string().into(),
                expected_net: reason.into(),
            });
        }

        Ok(())
    }

    /// Validate device materials against device definition (GAP 7: Material Validation)
    pub(super) fn validate_device_materials(
        &self,
        device_name: &str,
        device_type: &str,
        terminal_pours: &FxHashMap<CompactString, PourMetadata>,
    ) -> Result<(), DeviceExtractionError> {
        // Try to get device definition from symbol table
        let device_def = match self.symbol_table.get_device(device_type) {
            Ok(def) => def,
            Err(_) => {
                // Device definition not found - this is OK, validation is optional
                return Ok(());
            }
        };

        // Convert device definition to contract for validation
        let contract = hwc_parser::DeviceContract::from_device_definition(device_def);

        // Validate each terminal's material using contract
        for (terminal_name, pour) in terminal_pours {
            if let Err(reason) =
                contract.validate_terminal_material(terminal_name, &pour.material_name)
            {
                return Err(DeviceExtractionError::InvalidGeometry {
                    device_name: device_name.to_string().into(),
                    device_type: device_type.to_string().into(),
                    reason: format!(
                        "❌ Physics Error: {} device contract violation\n\n  \
                        Device: {} ({})\n  \
                        Terminal: {}\n  \
                        Pour: {}\n  \
                        {}\n  \
                        Contract: @std/foundry/transistors.hw::{}",
                        device_type,
                        device_name,
                        device_type,
                        terminal_name,
                        pour.name,
                        reason,
                        device_type
                    )
                    .into(),
                });
            }
        }

        Ok(())
    }
}
