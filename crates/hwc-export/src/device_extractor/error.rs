use compact_str::CompactString;
use std::fmt;

/// Device extraction errors
#[derive(Debug, Clone)]
pub enum DeviceExtractionError {
    /// Invalid geometry
    InvalidGeometry {
        device_name: CompactString,
        device_type: CompactString,
        reason: CompactString,
    },
    /// Missing bulk contact (GAP 5)
    MissingBulkContact {
        transistor: CompactString,
        device_type_name: CompactString,
        expected_bulk_net: CompactString,
    },
    /// Bulk biasing violation (GAP 5)
    BiasViolation {
        transistor: CompactString,
        device_type_name: CompactString,
        bulk_net: CompactString,
        expected_net: CompactString,
    },
}

impl fmt::Display for DeviceExtractionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidGeometry {
                device_name,
                device_type,
                reason,
            } => {
                write!(
                    f,
                    "Invalid geometry for {} '{}': {}",
                    device_type, device_name, reason
                )
            }
            Self::MissingBulkContact {
                transistor,
                device_type_name,
                expected_bulk_net,
            } => {
                write!(
                    f,
                    "Missing bulk contact for {} '{}': expected connection to {}",
                    device_type_name, transistor, expected_bulk_net
                )
            }
            Self::BiasViolation {
                transistor,
                device_type_name,
                bulk_net,
                expected_net,
            } => {
                write!(
                    f,
                    "Bulk biasing violation for {} '{}': bulk connected to '{}', expected '{}'",
                    device_type_name, transistor, bulk_net, expected_net
                )
            }
        }
    }
}

impl std::error::Error for DeviceExtractionError {}
