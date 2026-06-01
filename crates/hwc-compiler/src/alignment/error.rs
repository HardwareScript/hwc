//! Alignment Validation Error Types
//!
//! This module defines error types for alignment validation failures.
//! Errors are designed to be actionable - they tell the user exactly what's
//! wrong and how to fix it.
//!
//! Phase 5: Enhanced with spatial information (pour coordinates) for precise
//! error localization.

use super::netlist::DeviceTypeId;
use compact_str::CompactString;
use hwc_engine::geometry::BoundingBox;
use miette::Diagnostic;
use thiserror::Error;

/// Spatial location information for error reporting
#[derive(Debug, Clone)]
pub struct SpatialInfo {
    /// Pour name that caused the error
    pub pour_name: CompactString,
    /// Bounding box in nanometers (if available)
    pub bbox: Option<BoundingBox>,
    /// Bottom Z elevation in nanometers
    pub z_bottom_nm: Option<i64>,
}

impl SpatialInfo {
    /// Format spatial information for error messages
    pub fn format(&self) -> CompactString {
        let mut parts = vec![format!("pour '{}'", self.pour_name)];

        if let Some(z_nm) = self.z_bottom_nm {
            let z_mm = z_nm as f64 / 1_000_000.0;
            parts.push(format!("at z = {:.4}mm", z_mm));
        }

        if let Some(ref bbox) = self.bbox {
            // Convert nanometers to micrometers for readability
            let x_min_um = bbox.min.x as f64 / 1000.0;
            let y_min_um = bbox.min.y as f64 / 1000.0;
            let x_max_um = bbox.max.x as f64 / 1000.0;
            let y_max_um = bbox.max.y as f64 / 1000.0;

            parts.push(format!(
                "at [{:.2}um, {:.2}um] to [{:.2}um, {:.2}um]",
                x_min_um, y_min_um, x_max_um, y_max_um
            ));
        }

        parts.join(" ").into()
    }
}

/// Alignment validation errors
#[derive(Debug, Clone, Error, Diagnostic)]
pub enum AlignmentError {
    /// Device count mismatch between logical and physical
    #[error("Device count mismatch: expected {expected} devices, found {found}")]
    #[diagnostic(
        code(alignment::device_count_mismatch),
        help("Check that all devices from the module are implemented in the space")
    )]
    DeviceCountMismatch { expected: usize, found: usize },

    /// Device type mismatch
    #[error("Device '{device_name}' type mismatch: expected {expected_type_name}, found {found_type_name}")]
    #[diagnostic(
        code(alignment::device_type_mismatch),
        help("Verify the device type in the physical layout matches the logical specification")
    )]
    DeviceTypeMismatch {
        device_name: CompactString,
        expected_type_id: DeviceTypeId,
        found_type_id: DeviceTypeId,
        expected_type_name: CompactString, // For error messages
        found_type_name: CompactString,    // For error messages
    },

    /// Terminal connection mismatch
    #[error("Device '{device_name}' terminal '{terminal_name}' mismatch: connected to '{found_net}', expected '{expected_net}'", device_name = .0.device_name, terminal_name = .0.terminal_name, expected_net = .0.expected_net, found_net = .0.found_net)]
    #[diagnostic(
        code(alignment::terminal_mismatch),
        help("{}{}", .0.suggestion, if let Some(ref spatial) = .0.spatial_info {
            format!("\n  Location: {}", spatial.format())
        } else {
            String::new()
        })
    )]
    TerminalMismatch(Box<TerminalMismatchDetails>),

    #[error("Device '{device_name}' parameter '{parameter}' mismatch: expected {expected:.2}, found {found:.2} (tolerance: {tolerance:.1}%)", device_name = .0.device_name, parameter = .0.parameter, expected = .0.expected, found = .0.found, tolerance = .0.tolerance)]
    #[diagnostic(
        code(alignment::parameter_mismatch),
        help("Adjust the device geometry to match the specified parameter within tolerance{}",
            if let Some(ref spatial) = .0.spatial_info {
                format!("\n  Location: {}", spatial.format())
            } else {
                String::new()
            })
    )]
    ParameterMismatch(Box<ParameterMismatchDetails>),

    /// Device missing in physical layout
    #[error("Device '{device_name}' not found in physical layout")]
    #[diagnostic(
        code(alignment::device_missing),
        help("Add the missing device to the space definition")
    )]
    DeviceMissing { device_name: CompactString },

    /// Terminal missing on device
    #[error("Device '{device_name}' missing terminal '{terminal}'")]
    #[diagnostic(
        code(alignment::terminal_missing),
        help("Ensure all required terminals are connected in the physical layout")
    )]
    TerminalMissing {
        device_name: CompactString,
        terminal: CompactString,
    },

    /// Port missing in physical layout
    #[error("Port '{port_name}' missing in physical layout")]
    #[diagnostic(
        code(alignment::port_missing),
        help("Add the missing port to the space definition")
    )]
    PortMissing { port_name: CompactString },

    /// Port direction mismatch
    #[error("Port '{port_name}' direction mismatch: expected {expected}, found {actual}")]
    #[diagnostic(
        code(alignment::port_direction_mismatch),
        help("Verify the port direction matches the module specification")
    )]
    PortDirectionMismatch {
        port_name: CompactString,
        expected: CompactString,
        actual: CompactString,
    },

    /// Port connected to wrong net
    #[error("Port '{port_name}' mismatch: connected to '{found_net}', expected '{expected_net}'")]
    #[diagnostic(
        code(alignment::port_mismatch),
        help("Check the net assignment for this port")
    )]
    PortMismatch {
        port_name: CompactString,
        expected_net: CompactString,
        found_net: CompactString,
    },

    #[error("Net name mismatch on device '{device_name}' terminal '{terminal_name}': logical netlist expects '{logical_net}', physical layout has '{physical_net}'", device_name = .0.device_name, terminal_name = .0.terminal_name, logical_net = .0.logical_net, physical_net = .0.physical_net)]
    #[diagnostic(
        code(alignment::net_name_mismatch),
        help("Net names must match exactly (case-sensitive). {}{}",
            .0.suggestion.as_deref().unwrap_or(""),
            if let Some(ref spatial) = .0.spatial_info {
                format!("\n  Location: {}", spatial.format())
            } else {
                String::new()
            })
    )]
    NetNameMismatch(Box<NetNameMismatchDetails>),

    /// Device not found in physical layout
    #[error("Device '{device_name}' not found in physical layout")]
    #[diagnostic(
        code(alignment::device_not_found),
        help("Implement the missing device in the space definition")
    )]
    DeviceNotFound { device_name: CompactString },

    /// Unconnected port
    #[error("Port '{port_name}' is not connected")]
    #[diagnostic(
        code(alignment::unconnected_port),
        help("Connect this port to a net in the physical layout")
    )]
    UnconnectedPort { port_name: CompactString },

    /// Module not found in symbol table
    #[error("Module '{module_name}' not found in symbol table")]
    #[diagnostic(
        code(alignment::module_not_found),
        help("Ensure the module is defined before the space that implements it")
    )]
    ModuleNotFound { module_name: CompactString },

    /// Synthesis error
    #[error("Synthesis error: {message}")]
    #[diagnostic(
        code(alignment::synthesis_error),
        help("Check the module definition syntax")
    )]
    SynthesisError { message: CompactString },
}

/// Details for terminal mismatch errors (boxed to reduce enum size)
#[derive(Debug, Clone)]
pub struct TerminalMismatchDetails {
    pub device_name: CompactString,
    pub terminal_name: CompactString,
    pub expected_net: CompactString,
    pub found_net: CompactString,
    pub suggestion: CompactString,
    pub spatial_info: Option<SpatialInfo>,
}

/// Details for parameter mismatch errors (boxed to reduce enum size)
#[derive(Debug, Clone)]
pub struct ParameterMismatchDetails {
    pub device_name: CompactString,
    pub parameter: CompactString,
    pub expected: f64,
    pub found: f64,
    pub tolerance: f64,
    pub spatial_info: Option<SpatialInfo>,
}

/// Details for net name mismatch errors (boxed to reduce enum size)
#[derive(Debug, Clone)]
pub struct NetNameMismatchDetails {
    pub device_name: CompactString,
    pub terminal_name: CompactString,
    pub logical_net: CompactString,
    pub physical_net: CompactString,
    pub suggestion: Option<CompactString>,
    pub spatial_info: Option<SpatialInfo>,
}

/// Calculate Levenshtein distance between two strings (for similarity suggestions)
fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    let len1 = s1.len();
    let len2 = s2.len();
    let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];

    for (i, row) in matrix.iter_mut().enumerate().take(len1 + 1) {
        row[0] = i;
    }
    for (j, cell) in matrix[0].iter_mut().enumerate().take(len2 + 1) {
        *cell = j;
    }

    for i in 1..=len1 {
        for j in 1..=len2 {
            let cost = if s1.chars().nth(i - 1) == s2.chars().nth(j - 1) {
                0
            } else {
                1
            };
            matrix[i][j] = (matrix[i - 1][j] + 1)
                .min(matrix[i][j - 1] + 1)
                .min(matrix[i - 1][j - 1] + cost);
        }
    }

    matrix[len1][len2]
}

/// Find the most similar net name from a list (for error suggestions)
pub fn suggest_similar_net(target: &str, candidates: &[String]) -> Option<CompactString> {
    if candidates.is_empty() {
        return None;
    }

    let mut best_match = None;
    let mut best_distance = usize::MAX;

    for candidate in candidates {
        let distance = levenshtein_distance(target, candidate);

        // Only suggest if reasonably similar (distance < 3 or < 30% of length)
        let max_distance = (target.len().max(candidate.len()) * 3 / 10).max(2);

        if distance < best_distance && distance <= max_distance {
            best_distance = distance;
            best_match = Some(candidate.clone());
        }
    }

    best_match.map(Into::into)
}
