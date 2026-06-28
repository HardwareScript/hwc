//! PCB stackup definitions for impedance control and layer management
//!
//! This module defines PCB stackup structures that specify layer thicknesses,
//! dielectric heights, and impedance control parameters.
//!
//! # v0.1.4 Status
//!
//! Stackup profiles are currently defined using default values.
//! Future versions will support `define stackup` blocks in .hw files
//! similar to how profiles work with the Symbol Table.
//!
//! For now, use `StackupProfile::default()` or create custom stackups programmatically.

use compact_str::CompactString;
use miette::Diagnostic;
use thiserror::Error;

#[derive(Error, Diagnostic, Debug)]
pub enum StackupError {
    #[error("Stackup profile not found: {0}")]
    #[diagnostic(
        code(M23),
        url("https://docs.hw-script.org/errors/M23"),
        help("Use StackupProfile::default() or create a custom stackup programmatically")
    )]
    NotFound(String),

    #[error("Failed to parse stackup definition: {0}")]
    #[diagnostic(
        code(M31),
        url("https://docs.hw-script.org/errors/M31"),
        help("Stackup definitions are not yet supported in v0.1.4 - use default or programmatic creation")
    )]
    ParseError(String),

    #[error("Failed to read file: {0}")]
    #[diagnostic(
        code(M32),
        url("https://docs.hw-script.org/errors/M32"),
        help("Verify file path exists and has read permissions")
    )]
    IoError(#[from] std::io::Error),

    #[error("Invalid stackup configuration: {0}")]
    #[diagnostic(
        code(M13),
        url("https://docs.hw-script.org/errors/M13"),
        help("Stackup must have valid layer thicknesses and dielectric constants")
    )]
    InvalidConfiguration(String),
}

/// Complete PCB stackup profile
#[derive(Debug, Clone)]
pub struct StackupProfile {
    pub name: CompactString,
    pub description: CompactString,
    pub board: BoardSpecification,
    pub layers: Vec<Layer>,
    pub impedance: ImpedanceParameters,
    pub notes: Option<Vec<CompactString>>,
}

/// Board-level specifications
#[derive(Debug, Clone)]
pub struct BoardSpecification {
    /// Total board thickness in nanometers
    pub total_thickness_nm: i64,

    /// Manufacturing tolerance as percentage
    pub tolerance_percent: f64,

    /// Copper weight in ounces (1oz = 35µm)
    pub copper_weight_oz: i32,
}

/// Individual layer in the stackup
#[derive(Debug, Clone)]
pub struct Layer {
    pub name: CompactString,

    /// Layer type: "signal", "plane", "dielectric"
    pub layer_type: CompactString,

    /// Material: "copper", "fr4", etc.
    pub material: CompactString,

    /// Layer thickness in nanometers
    pub thickness_nm: i64,

    /// Dielectric constant (for dielectric layers only)
    pub dielectric_constant: Option<f64>,
}

/// Impedance control parameters
#[derive(Debug, Clone)]
pub struct ImpedanceParameters {
    /// Dielectric height for microstrip calculations (nanometers)
    pub microstrip_dielectric_height_nm: i64,

    /// Dielectric height for stripline calculations (nanometers, optional)
    pub stripline_dielectric_height_nm: Option<i64>,

    /// Typical trace width for 50Ω single-ended (nanometers, optional)
    pub single_ended_50ohm_trace_width_nm: Option<i64>,

    /// Typical trace width for 75Ω single-ended (nanometers, optional)
    pub single_ended_75ohm_trace_width_nm: Option<i64>,

    /// Typical trace width for 90Ω differential (nanometers, optional)
    pub differential_90ohm_trace_width_nm: Option<i64>,

    /// Typical spacing for 90Ω differential (nanometers, optional)
    pub differential_90ohm_spacing_nm: Option<i64>,
}

impl StackupProfile {
    /// Create stackup from profile name (v0.1.4)
    ///
    /// Currently returns an error - stackup definitions are not yet supported.
    /// Use `StackupProfile::default()` or create programmatically.
    pub fn from_profile_name(name: &str) -> Result<Self, StackupError> {
        Err(StackupError::NotFound(format!(
            "Stackup profile '{}' not found. Use StackupProfile::default() for now.",
            name
        )))
    }

    /// Create default 2-layer stackup
    ///
    /// Returns the default 2-layer PCB stackup (1.6mm FR4, 1oz copper).
    pub fn default_2layer() -> Self {
        Self::default()
    }

    /// Create default 4-layer stackup
    ///
    /// Returns a standard 4-layer PCB stackup with signal/plane/plane/signal configuration.
    pub fn default_4layer() -> Self {
        Self {
            name: "Default 4-Layer".into(),
            description: "Default 4-layer stackup with power planes".into(),
            board: BoardSpecification {
                total_thickness_nm: 1_600_000,
                tolerance_percent: 10.0,
                copper_weight_oz: 1,
            },
            layers: vec![
                Layer {
                    name: "Top".into(),
                    layer_type: "signal".into(),
                    material: "copper".into(),
                    thickness_nm: 35_000,
                    dielectric_constant: None,
                },
                Layer {
                    name: "Prepreg1".into(),
                    layer_type: "dielectric".into(),
                    material: "fr4".into(),
                    thickness_nm: 200_000,
                    dielectric_constant: Some(4.2),
                },
                Layer {
                    name: "GND".into(),
                    layer_type: "plane".into(),
                    material: "copper".into(),
                    thickness_nm: 35_000,
                    dielectric_constant: None,
                },
                Layer {
                    name: "Core".into(),
                    layer_type: "dielectric".into(),
                    material: "fr4".into(),
                    thickness_nm: 1_060_000,
                    dielectric_constant: Some(4.2),
                },
                Layer {
                    name: "VCC".into(),
                    layer_type: "plane".into(),
                    material: "copper".into(),
                    thickness_nm: 35_000,
                    dielectric_constant: None,
                },
                Layer {
                    name: "Prepreg2".into(),
                    layer_type: "dielectric".into(),
                    material: "fr4".into(),
                    thickness_nm: 200_000,
                    dielectric_constant: Some(4.2),
                },
                Layer {
                    name: "Bottom".into(),
                    layer_type: "signal".into(),
                    material: "copper".into(),
                    thickness_nm: 35_000,
                    dielectric_constant: None,
                },
            ],
            impedance: ImpedanceParameters {
                microstrip_dielectric_height_nm: 200_000,
                stripline_dielectric_height_nm: Some(1_060_000),
                single_ended_50ohm_trace_width_nm: Some(350_000),
                single_ended_75ohm_trace_width_nm: Some(175_000),
                differential_90ohm_trace_width_nm: Some(250_000),
                differential_90ohm_spacing_nm: Some(150_000),
            },
            notes: Some(vec![
                "Standard 4-layer stackup with power planes".into(),
                "Top/Bottom: Signal layers".into(),
                "GND/VCC: Power planes".into(),
            ]),
        }
    }

    /// Get copper thickness in nanometers based on copper weight.
    /// 1 oz copper = 35 µm = 35,000 nm (IPC standard weight-to-thickness conversion).
    pub fn get_copper_thickness_nm(&self) -> i64 {
        (self.board.copper_weight_oz as i64) * 35_000
    }

    /// Get dielectric constant for a specific layer
    pub fn get_layer_dielectric_constant(&self, layer_name: &str) -> Option<f64> {
        self.layers
            .iter()
            .find(|l| l.name == layer_name)
            .and_then(|l| l.dielectric_constant)
    }

    /// Get total dielectric thickness (sum of all dielectric layers)
    pub fn get_total_dielectric_thickness_nm(&self) -> i64 {
        self.layers
            .iter()
            .filter(|l| l.layer_type == "dielectric")
            .map(|l| l.thickness_nm)
            .sum()
    }

    /// Validate stackup configuration
    pub fn validate(&self) -> Result<(), StackupError> {
        // Check that total thickness matches sum of layers
        let calculated_thickness: i64 = self.layers.iter().map(|l| l.thickness_nm).sum();
        let tolerance =
            (self.board.total_thickness_nm as f64 * self.board.tolerance_percent / 100.0) as i64;

        if (calculated_thickness - self.board.total_thickness_nm).abs() > tolerance {
            return Err(StackupError::InvalidConfiguration(format!(
                "Layer thicknesses sum to {}nm but total thickness is {}nm",
                calculated_thickness, self.board.total_thickness_nm
            )));
        }

        // Check that dielectric layers have dielectric constants
        for layer in &self.layers {
            if layer.layer_type == "dielectric" && layer.dielectric_constant.is_none() {
                return Err(StackupError::InvalidConfiguration(format!(
                    "Dielectric layer '{}' missing dielectric_constant",
                    layer.name
                )));
            }
        }

        Ok(())
    }
}

impl Default for StackupProfile {
    fn default() -> Self {
        Self {
            name: "Default 2-Layer".into(),
            description: "Default 2-layer stackup".into(),
            board: BoardSpecification {
                total_thickness_nm: 1_600_000,
                tolerance_percent: 10.0,
                copper_weight_oz: 1,
            },
            layers: vec![
                Layer {
                    name: "Top".into(),
                    layer_type: "signal".into(),
                    material: "copper".into(),
                    thickness_nm: 35_000,
                    dielectric_constant: None,
                },
                Layer {
                    name: "Core".into(),
                    layer_type: "dielectric".into(),
                    material: "fr4".into(),
                    thickness_nm: 1_530_000,
                    dielectric_constant: Some(4.2),
                },
                Layer {
                    name: "Bottom".into(),
                    layer_type: "signal".into(),
                    material: "copper".into(),
                    thickness_nm: 35_000,
                    dielectric_constant: None,
                },
            ],
            impedance: ImpedanceParameters {
                microstrip_dielectric_height_nm: 1_530_000,
                stripline_dielectric_height_nm: None,
                single_ended_50ohm_trace_width_nm: Some(3_000_000),
                single_ended_75ohm_trace_width_nm: Some(1_500_000),
                differential_90ohm_trace_width_nm: None,
                differential_90ohm_spacing_nm: None,
            },
            notes: None,
        }
    }
}
