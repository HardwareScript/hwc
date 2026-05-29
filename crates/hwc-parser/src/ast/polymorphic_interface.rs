//! Polymorphic Interface definitions for duck-typed module compatibility
//!
//! Enables polymorphic Standard Library components that work with any compatible chip.
//! Example: An I2S_Controller that works with CS4344, PCM5102, or any chip with matching pins.
//!
//! # Design Philosophy
//!
//! Duck-typed interfaces allow developers to write reusable modules that work with
//! any component that has the required pins, without explicit inheritance.
//!
//! # Example
//!
//! ```hardware
//! interface I2S_DAC:
//!     pins:
//!         BCLK: output
//!         LRCLK: output
//!         DATA: output
//!         optional MCLK: output
//!
//! component CS4344 implements I2S_DAC:
//!     pins: BCLK, LRCLK, DATA, MCLK, VCC, GND
//!     ...
//!
//! component PCM5102 implements I2S_DAC:
//!     pins: BCK, LRCK, DIN, SCK, VCC, GND
//!     ...
//!
//! module AudioPlayer:
//!     pins: I2S_BCLK, I2S_LRCLK, I2S_DATA
//!     
//!     add I2S_DAC named DAC  // Can use CS4344 or PCM5102
//!     route I2S_BCLK to DAC.BCLK
//!     route I2S_LRCLK to DAC.LRCLK
//!     route I2S_DATA to DAC.DATA
//! ```

use compact_str::CompactString;
use serde::{Deserialize, Serialize};

use super::common::Identifier;
use crate::lexer::Span;

/// Polymorphic interface definition: `interface InterfaceName:` (v0.1.6)
///
/// Specifies required pins and their types for duck-typed compatibility.
/// Components that implement this interface must have all required pins.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolymorphicInterfaceDefinition {
    /// Interface name (e.g., I2S_DAC, SPI_Flash, UART_Device)
    pub name: Identifier,

    /// Description of the interface (optional)
    pub description: Option<CompactString>,

    /// Required pins with their types
    pub required_pins: Vec<InterfacePin>,

    /// Optional pins (component may or may not have these)
    pub optional_pins: Vec<InterfacePin>,

    /// Span in source code
    pub span: Span,
}

/// Pin specification in an interface
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InterfacePin {
    /// Pin name as specified in the interface
    pub name: CompactString,

    /// Pin direction/type
    pub pin_type: PinType,

    /// Optional description
    pub description: Option<CompactString>,

    /// Span in source code
    pub span: Span,
}

/// Pin type/direction for interface pins
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PinType {
    /// Input pin (data flows into the component.into())
    Input,

    /// Output pin (data flows out of the component.into())
    Output,

    /// Bidirectional pin (data can flow both ways)
    Bidirectional,

    /// Power pin (VCC, VDD, etc.)
    Power,

    /// Ground pin (GND, VSS, etc.)
    Ground,

    /// Any type (for flexible interfaces)
    Any,
}

impl PinType {
    /// Check if two pin types are compatible
    ///
    /// Rules:
    /// - Any is compatible with everything
    /// - Input is compatible with Output (connection)
    /// - Output is compatible with Input (connection)
    /// - Bidirectional is compatible with everything except Power/Ground
    /// - Power is only compatible with Power
    /// - Ground is only compatible with Ground
    pub fn is_compatible_with(&self, other: &PinType) -> bool {
        use PinType::*;

        match (self, other) {
            // Any is compatible with everything
            (Any, _) | (_, Any) => true,

            // Exact matches
            (Input, Input) | (Output, Output) => true,

            // Input/Output are compatible for connections
            (Input, Output) | (Output, Input) => true,

            // Bidirectional is compatible with Input/Output/Bidirectional
            (Bidirectional, Input) | (Bidirectional, Output) | (Bidirectional, Bidirectional) => {
                true
            }
            (Input, Bidirectional) | (Output, Bidirectional) => true,

            // Power/Ground must match exactly
            (Power, Power) | (Ground, Ground) => true,

            // Everything else is incompatible
            _ => false,
        }
    }
}

/// Interface implementation declaration in component definition
///
/// Example: `implements "I2S_DAC"`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InterfaceImplementation {
    /// Name of the interface being implemented
    pub interface_name: CompactString,

    /// Optional pin mappings (if component pins have different names)
    /// Maps interface pin name → component pin name
    /// Example: { "BCLK" → "BCK", "LRCLK" → "LRCK", "DATA" → "DIN" }
    pub pin_mappings: Vec<PinMapping>,

    /// Span in source code
    pub span: Span,
}

/// Pin mapping for interface implementation
///
/// Maps an interface pin name to a component pin name when they differ.
/// Example: `BCLK → BCK` (interface pin "BCLK" maps to component pin "BCK")
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PinMapping {
    /// Interface pin name
    pub interface_pin: CompactString,

    /// Component pin name
    pub component_pin: CompactString,

    /// Span in source code
    pub span: Span,
}

/// Interface validation error
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InterfaceValidationError {
    /// Required pin is missing from component
    MissingRequiredPin {
        interface_name: CompactString,
        pin_name: CompactString,
        component_name: CompactString,
    },

    /// Pin type mismatch between interface and component
    PinTypeMismatch {
        interface_name: CompactString,
        pin_name: CompactString,
        expected_type: PinType,
        actual_type: PinType,
        component_name: CompactString,
    },

    /// Component declares it implements an interface that doesn't exist
    InterfaceNotFound {
        interface_name: CompactString,
        component_name: CompactString,
    },

    /// Pin mapping references a pin that doesn't exist in the interface
    InvalidPinMapping {
        interface_name: CompactString,
        interface_pin: CompactString,
        component_name: CompactString,
    },

    /// Pin mapping references a component pin that doesn't exist
    MappedPinNotFound {
        component_name: CompactString,
        component_pin: CompactString,
        interface_pin: CompactString,
    },
}

impl InterfaceValidationError {
    /// Format error message for display
    pub fn format_message(&self) -> CompactString {
        match self {
            InterfaceValidationError::MissingRequiredPin {
                interface_name,
                pin_name,
                component_name,
            } => {
                format!(
                    "Component '{}' implements interface '{}' but is missing required pin '{}'",
                    component_name, interface_name, pin_name
                ).into()
            }
            InterfaceValidationError::PinTypeMismatch {
                interface_name,
                pin_name,
                expected_type,
                actual_type,
                component_name,
            } => {
                format!(
                    "Component '{}' implements interface '{}' but pin '{}' has type {:?}, expected {:?}",
                    component_name, interface_name, pin_name, actual_type, expected_type
                ).into()
            }
            InterfaceValidationError::InterfaceNotFound {
                interface_name,
                component_name,
            } => {
                format!(
                    "Component '{}' declares it implements interface '{}', but that interface is not defined",
                    component_name, interface_name
                ).into()
            }
            InterfaceValidationError::InvalidPinMapping {
                interface_name,
                interface_pin,
                component_name,
            } => {
                format!(
                    "Component '{}' maps pin '{}' which doesn't exist in interface '{}'",
                    component_name, interface_pin, interface_name
                ).into()
            }
            InterfaceValidationError::MappedPinNotFound {
                component_name,
                component_pin,
                interface_pin,
            } => {
                format!(
                    "Component '{}' maps interface pin '{}' to component pin '{}', but '{}' doesn't exist in the component",
                    component_name, interface_pin, component_pin, component_pin
                ).into()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pin_type_compatibility() {
        use PinType::*;

        // Any is compatible with everything
        assert!(Any.is_compatible_with(&Input));
        assert!(Any.is_compatible_with(&Output));
        assert!(Any.is_compatible_with(&Bidirectional));
        assert!(Any.is_compatible_with(&Power));
        assert!(Any.is_compatible_with(&Ground));

        // Input/Output are compatible for connections
        assert!(Input.is_compatible_with(&Output));
        assert!(Output.is_compatible_with(&Input));

        // Bidirectional is compatible with Input/Output
        assert!(Bidirectional.is_compatible_with(&Input));
        assert!(Bidirectional.is_compatible_with(&Output));
        assert!(Input.is_compatible_with(&Bidirectional));
        assert!(Output.is_compatible_with(&Bidirectional));

        // Power/Ground must match exactly
        assert!(Power.is_compatible_with(&Power));
        assert!(Ground.is_compatible_with(&Ground));
        assert!(!Power.is_compatible_with(&Ground));
        assert!(!Ground.is_compatible_with(&Power));

        // Power/Ground not compatible with signal pins
        assert!(!Power.is_compatible_with(&Input));
        assert!(!Power.is_compatible_with(&Output));
        assert!(!Ground.is_compatible_with(&Input));
        assert!(!Ground.is_compatible_with(&Output));
    }

    #[test]
    fn test_error_message_formatting() {
        let error = InterfaceValidationError::MissingRequiredPin {
            interface_name: "I2S_DAC".into(),
            pin_name: "BCLK".into(),
            component_name: "CS4344".into(),
        };

        let message = error.format_message();
        assert!(message.contains("CS4344"));
        assert!(message.contains("I2S_DAC"));
        assert!(message.contains("BCLK"));
        assert!(message.contains("missing required pin"));
    }

    #[test]
    fn test_pin_type_mismatch_error() {
        let error = InterfaceValidationError::PinTypeMismatch {
            interface_name: "SPI_Device".into(),
            pin_name: "MISO".into(),
            expected_type: PinType::Output,
            actual_type: PinType::Input,
            component_name: "Flash_Chip".into(),
        };

        let message = error.format_message();
        assert!(message.contains("Flash_Chip"));
        assert!(message.contains("SPI_Device"));
        assert!(message.contains("MISO"));
        assert!(message.contains("type"));
    }
}
