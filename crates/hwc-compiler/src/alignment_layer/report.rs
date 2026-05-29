//! Alignment Layer Report Generation
//!
//! This module defines the Alignment Report structure and violation types.
//! 
//! The Alignment Layer replaces traditional LVS by integrating three validation checks:
//! - Layer 1: Symbolic Alignment (device names and types match)
//! - Layer 2: Physical Continuity (nets form single conductive islands)
//! - Layer 3: Device Extraction (parameters match within tolerance)

use compact_str::CompactString;
use std::fmt;

/// Alignment Layer verification report
#[derive(Debug, Clone)]
pub struct AlignmentReport {
    /// Whether alignment validation passed (no violations)
    pub passed: bool,

    /// List of violations found
    pub violations: Vec<AlignmentViolation>,

    /// Number of devices in physical netlist
    pub physical_device_count: usize,

    /// Number of devices in logical netlist
    pub logical_device_count: usize,

    /// Number of nets in physical netlist
    pub physical_net_count: usize,

    /// Number of nets in logical netlist
    pub logical_net_count: usize,
}

impl AlignmentReport {
    /// Format report as human-readable text
    pub fn format(&self) -> String {
        let mut output = String::new();

        output.push_str("═══════════════════════════════════════════════════════════════\n");
        output.push_str("                 ALIGNMENT LAYER VALIDATION REPORT             \n");
        output.push_str("═══════════════════════════════════════════════════════════════\n\n");

        // Summary
        if self.passed {
            output.push_str("✅ ALIGNMENT PASSED - Layout implements module correctly\n\n");
        } else {
            output.push_str(&format!(
                "❌ ALIGNMENT FAILED - {} violation(s) found\n\n",
                self.violations.len()
            ));
        }

        // Statistics
        output.push_str("Statistics:\n");
        output.push_str(&format!(
            "  Devices (Physical): {}\n",
            self.physical_device_count
        ));
        output.push_str(&format!(
            "  Devices (Logical):  {}\n",
            self.logical_device_count
        ));
        output.push_str(&format!(
            "  Nets (Physical):    {}\n",
            self.physical_net_count
        ));
        output.push_str(&format!(
            "  Nets (Logical):     {}\n\n",
            self.logical_net_count
        ));

        // Violations
        if !self.violations.is_empty() {
            output.push_str("Violations:\n");
            output.push_str("───────────────────────────────────────────────────────────────\n");

            for (i, violation) in self.violations.iter().enumerate() {
                output.push_str(&format!("\n{}. {}\n", i + 1, violation));
            }

            output.push_str("\n───────────────────────────────────────────────────────────────\n");
        }

        output.push_str("\n═══════════════════════════════════════════════════════════════\n");

        output
    }
}

impl fmt::Display for AlignmentReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format())
    }
}

/// Alignment Layer violation types
/// 
/// These violations represent failures in the Triple-Check Architecture:
/// - Symbolic mismatches (Layer 1)
/// - Parameter mismatches (Layer 3)
/// - Connection mismatches (caught by Physical Continuity - Layer 2)
#[derive(Debug, Clone)]
pub enum AlignmentViolation {
    /// Device count mismatch between physical and logical
    DeviceCountMismatch {
        physical_count: usize,
        logical_count: usize,
    },

    /// Net count mismatch between physical and logical
    NetCountMismatch {
        physical_count: usize,
        logical_count: usize,
    },

    /// Device type mismatch
    DeviceTypeMismatch {
        device_name: CompactString,
        physical_type: CompactString,
        logical_type: CompactString,
    },

    /// Device exists in schematic but not in layout
    MissingPhysicalDevice {
        device_name: CompactString,
        device_type: CompactString,
    },

    /// Device exists in layout but not in schematic
    ExtraPhysicalDevice {
        device_name: CompactString,
        device_type: CompactString,
    },

    /// Connection mismatch between physical and logical
    ConnectionMismatch {
        device_name: CompactString,
        terminal: CompactString,
        physical_net: CompactString,
        logical_net: CompactString,
    },

    /// Parameter missing in physical device
    MissingParameter {
        device_name: CompactString,
        parameter: CompactString,
    },

    /// Parameter value mismatch (outside tolerance)
    ParameterMismatch {
        device_name: CompactString,
        parameter: CompactString,
        physical_value: f64,
        logical_value: f64,
        tolerance: f64,
        relative_error: f64,
    },
}

impl fmt::Display for AlignmentViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeviceCountMismatch {
                physical_count,
                logical_count,
            } => {
                write!(
                    f,
                    "Device Count Mismatch\n  \
                     Physical: {} devices\n  \
                     Logical:  {} devices\n  \
                     → Layout has {} devices than schematic",
                    physical_count,
                    logical_count,
                    if physical_count > logical_count {
                        "MORE"
                    } else {
                        "FEWER"
                    }
                )
            }
            Self::NetCountMismatch {
                physical_count,
                logical_count,
            } => {
                write!(
                    f,
                    "Net Count Mismatch\n  \
                     Physical: {} nets\n  \
                     Logical:  {} nets\n  \
                     → Layout has {} nets than schematic",
                    physical_count,
                    logical_count,
                    if physical_count > logical_count {
                        "MORE"
                    } else {
                        "FEWER"
                    }
                )
            }
            Self::DeviceTypeMismatch {
                device_name,
                physical_type,
                logical_type,
            } => {
                write!(
                    f,
                    "Device Type Mismatch: {}\n  \
                     Physical: {}\n  \
                     Logical:  {}\n  \
                     → Device '{}' has wrong type in layout",
                    device_name, physical_type, logical_type, device_name
                )
            }
            Self::MissingPhysicalDevice {
                device_name,
                device_type,
            } => {
                write!(
                    f,
                    "Missing Physical Device: {} ({})\n  \
                     → Device declared in schematic but not found in layout\n  \
                     → Add geometry with 'device: {}.terminal' bindings",
                    device_name, device_type, device_name
                )
            }
            Self::ExtraPhysicalDevice {
                device_name,
                device_type,
            } => {
                write!(
                    f,
                    "Extra Physical Device: {} ({})\n  \
                     → Device found in layout but not declared in schematic\n  \
                     → Add 'add {} named {}' to module",
                    device_name, device_type, device_type, device_name
                )
            }
            Self::ConnectionMismatch {
                device_name,
                terminal,
                physical_net,
                logical_net,
            } => {
                write!(
                    f,
                    "Connection Mismatch: {}.{}\n  \
                     Physical: {} → {}\n  \
                     Logical:  {} → {}\n  \
                     → Terminal connected to wrong net in layout",
                    device_name, terminal, terminal, physical_net, terminal, logical_net
                )
            }
            Self::MissingParameter {
                device_name,
                parameter,
            } => {
                write!(
                    f,
                    "Missing Parameter: {}.{}\n  \
                     → Parameter specified in schematic but not extracted from layout\n  \
                     → Check device geometry and extraction rules",
                    device_name, parameter
                )
            }
            Self::ParameterMismatch {
                device_name,
                parameter,
                physical_value,
                logical_value,
                tolerance,
                relative_error,
            } => {
                write!(
                    f,
                    "Parameter Mismatch: {}.{}\n  \
                     Physical: {:.3}um\n  \
                     Logical:  {:.3}um\n  \
                     Error:    {:.2}% (tolerance: {:.2}%)\n  \
                     → Parameter value outside acceptable tolerance",
                    device_name,
                    parameter,
                    physical_value,
                    logical_value,
                    relative_error * 100.0,
                    tolerance * 100.0
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alignment_report_passed() {
        let report = AlignmentReport {
            passed: true,
            violations: Vec::new(),
            physical_device_count: 2,
            logical_device_count: 2,
            physical_net_count: 4,
            logical_net_count: 4,
        };

        let formatted = report.format();
        assert!(formatted.contains("ALIGNMENT PASSED"));
        assert!(formatted.contains("Devices (Physical): 2"));
        assert!(formatted.contains("Devices (Logical):  2"));
    }

    #[test]
    fn test_alignment_report_failed() {
        let report = AlignmentReport {
            passed: false,
            violations: vec![AlignmentViolation::DeviceCountMismatch {
                physical_count: 1,
                logical_count: 2,
            }],
            physical_device_count: 1,
            logical_device_count: 2,
            physical_net_count: 3,
            logical_net_count: 4,
        };

        let formatted = report.format();
        assert!(formatted.contains("ALIGNMENT FAILED"));
        assert!(formatted.contains("1 violation(s)"));
        assert!(formatted.contains("Device Count Mismatch"));
    }

    #[test]
    fn test_alignment_violation_display() {
        let violation = AlignmentViolation::ConnectionMismatch {
            device_name: "M1".into(),
            terminal: "gate".into(),
            physical_net: "VDD".into(),
            logical_net: "VIN".into(),
        };

        let formatted = format!("{}", violation);
        assert!(formatted.contains("Connection Mismatch"));
        assert!(formatted.contains("M1.gate"));
        assert!(formatted.contains("VDD"));
        assert!(formatted.contains("VIN"));
    }
}
