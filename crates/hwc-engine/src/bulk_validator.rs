//! Bulk Connection Validator (Task 4.3)
//!
//! Physics-driven validation of bulk terminal connections using material database.
//!
//! # Architecture
//!
//! This validator uses the SAME physics-driven approach as device extraction:
//! 1. Get bulk material from device terminal pours
//! 2. Look up material in MaterialDatabase
//! 3. Read BiasRequirement from material properties
//! 4. Get net classification from HardwareSpace
//! 5. Validate: does net classification satisfy bias requirement?
//!
//! This scales infinitely - works for ANY semiconductor material!
//!
//! # Why This Matters
//!
//! Proper bulk biasing prevents:
//! - Latch-up in CMOS circuits
//! - Forward-biased PN junctions
//! - Substrate noise injection
//! - Unpredictable device behavior

use crate::space::{HardwareSpace, NetClassification};
use compact_str::CompactString;
use rustc_hash::FxHashMap;
use std::fmt;

/// Type alias for device list to reduce complexity
type DeviceList = [(
    CompactString,
    CompactString,
    FxHashMap<CompactString, String>,
    FxHashMap<CompactString, String>,
)];

/// Bulk connection validation errors
#[derive(Debug, Clone)]
pub enum BulkValidationError {
    /// Missing bulk terminal connection
    MissingBulkConnection {
        device_name: CompactString,
        device_type: CompactString,
        bulk_material: CompactString,
        bias_requirement: CompactString,
    },
    /// Invalid bulk biasing (wrong power rail)
    InvalidBulkBiasing {
        device_name: CompactString,
        device_type: CompactString,
        bulk_net: CompactString,
        bulk_material: CompactString,
        reason: CompactString,
    },
    /// Bulk net not classified
    UnclassifiedBulkNet {
        device_name: CompactString,
        device_type: CompactString,
        bulk_net: CompactString,
        bulk_material: CompactString,
        bias_requirement: CompactString,
    },
}

impl fmt::Display for BulkValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingBulkConnection {
                device_name,
                device_type,
                bulk_material,
                bias_requirement,
            } => {
                write!(
                    f,
                    "❌ Missing bulk connection for {} '{}'\n   \
                     Bulk material: {}\n   \
                     Required: {}",
                    device_type, device_name, bulk_material, bias_requirement
                )
            }
            Self::InvalidBulkBiasing {
                device_name,
                device_type,
                bulk_net,
                bulk_material,
                reason,
            } => {
                write!(
                    f,
                    "❌ Invalid bulk biasing for {} '{}'\n   \
                     Bulk net: '{}'\n   \
                     Bulk material: {}\n   \
                     {}",
                    device_type, device_name, bulk_net, bulk_material, reason
                )
            }
            Self::UnclassifiedBulkNet {
                device_name,
                device_type,
                bulk_net,
                bulk_material,
                bias_requirement,
            } => {
                write!(
                    f,
                    "❌ Unclassified bulk net for {} '{}'\n   \
                     Bulk net: '{}' (unclassified)\n   \
                     Bulk material: {}\n   \
                     Required: {}\n   \
                     Fix: Add 'net_classifications' to space definition",
                    device_type, device_name, bulk_net, bulk_material, bias_requirement
                )
            }
        }
    }
}

impl std::error::Error for BulkValidationError {}

/// Physics validator for bulk connections
pub struct BulkValidator {
    /// Material database for physics-driven validation
    material_database: hwc_materials::MaterialDatabase,
}

impl BulkValidator {
    /// Create a new bulk validator with material database
    ///
    /// # Arguments
    /// * `material_database` - Material database containing semiconductor properties
    ///
    /// # Architecture
    /// Uses the SAME material database as device extraction - no hardcoding!
    pub fn new(material_database: hwc_materials::MaterialDatabase) -> Self {
        Self { material_database }
    }

    /// Validate bulk connections for all devices
    ///
    /// # Arguments
    /// * `devices` - List of devices with their terminals and terminal pours
    /// * `space` - Hardware space for net classification lookup
    ///
    /// # Returns
    /// * `Ok(())` if all bulk connections are valid
    /// * `Err(Vec<BulkValidationError>)` if any violations are found
    ///
    /// # Architecture
    /// Physics-driven validation using material database:
    /// 1. For each device with a bulk terminal
    /// 2. Get bulk material from terminal_pours metadata
    /// 3. Look up material in database → get BiasRequirement
    /// 4. Get net classification from space
    /// 5. Validate: BiasRequirement.validate_net_classification()
    pub fn validate_bulk_connections(
        &self,
        devices: &DeviceList,
        space: &HardwareSpace,
    ) -> Result<(), Vec<BulkValidationError>> {
        let mut errors = Vec::new();

        for (device_name, device_type, terminals, terminal_pours) in devices {
            // Check if device has a bulk terminal
            if let Some(bulk_net) = terminals.get("bulk") {
                // Validate bulk connection using physics
                if let Err(error) = self.validate_device_bulk_physics(
                    device_name,
                    device_type,
                    bulk_net,
                    terminal_pours,
                    space,
                ) {
                    errors.push(error);
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Validate bulk connection using physics from material database
    ///
    /// This is the SAME algorithm as DeviceExtractor::validate_bulk_biasing_from_material
    /// but adapted for post-extraction validation.
    fn validate_device_bulk_physics(
        &self,
        device_name: &str,
        device_type: &str,
        bulk_net: &str,
        terminal_pours: &FxHashMap<CompactString, String>,
        space: &HardwareSpace,
    ) -> Result<(), BulkValidationError> {
        // Get bulk pour name from terminal_pours
        let bulk_pour_name = match terminal_pours.get("bulk") {
            Some(name) => name,
            None => {
                // No bulk pour metadata - skip validation
                // This shouldn't happen if device extraction succeeded
                return Ok(());
            }
        };

        // Find the bulk pour in space to get its material
        let bulk_material = space
            .pours
            .iter()
            .find(|p| p.name == bulk_pour_name.as_str())
            .map(|p| &p.material_name)
            .ok_or_else(|| BulkValidationError::MissingBulkConnection {
                device_name: device_name.into(),
                device_type: device_type.into(),
                bulk_material: "unknown".into(),
                bias_requirement: "bulk pour not found in space".into(),
            })?;

        // Look up material in database (case-insensitive)
        let semiconductor = match self
            .material_database
            .get_semiconductor(&bulk_material.to_lowercase())
        {
            Ok(semi) => semi,
            Err(_) => {
                // Material not in database - skip validation
                // This allows custom materials without breaking the compiler
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
        let net_classification = space.get_net_classification(bulk_net);

        // Check if net is classified
        if matches!(net_classification, NetClassification::Unclassified) {
            return Err(BulkValidationError::UnclassifiedBulkNet {
                device_name: device_name.into(),
                device_type: device_type.into(),
                bulk_net: bulk_net.into(),
                bulk_material: bulk_material.clone(),
                bias_requirement: format!("{:?}", bias_req).into(),
            });
        }

        // Convert NetClassification to hwc_materials::NetClassification
        let materials_net_class = match net_classification {
            NetClassification::Power => hwc_materials::NetClassification::Power,
            NetClassification::Ground => hwc_materials::NetClassification::Ground,
            NetClassification::Signal => hwc_materials::NetClassification::Signal,
            NetClassification::HighVoltage => hwc_materials::NetClassification::HighVoltage,
            NetClassification::Unclassified => hwc_materials::NetClassification::Unclassified,
        };

        // Validate using the material's bias requirement method (data-driven!)
        if let Err(reason) = bias_req.validate_net_classification(materials_net_class) {
            return Err(BulkValidationError::InvalidBulkBiasing {
                device_name: device_name.into(),
                device_type: format!(
                    "{} ({} bulk: {})",
                    device_type,
                    semiconductor
                        .doping_type
                        .as_ref()
                        .map(|dt| format!("{:?}", dt))
                        .unwrap_or_else(|| "unknown".to_string()),
                    bulk_material
                )
                .into(),
                bulk_net: bulk_net.into(),
                bulk_material: bulk_material.clone(),
                reason: reason.into(),
            });
        }

        Ok(())
    }
}

impl Default for BulkValidator {
    fn default() -> Self {
        // Default requires a symbol table, so we create an empty material database
        Self {
            material_database: hwc_materials::MaterialDatabase::empty(),
        }
    }
}
