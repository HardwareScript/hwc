//! SPICE Model Card Definitions (v0.2.1+)
//!
//! Declares process-specific semiconductor physics parameters that cannot be
//! calculated from geometry alone. These model cards are imported from PDK files
//! and referenced by device definitions.
//!
//! Philosophy: Zero compiler magic. PDKs declare physics, compiler extracts geometry.
//!
//! Example:
//! ```hw
//! export spice_model DMOD:
//!     type: diode
//!     parameters:
//!         IS: 1e-12
//!         N: 1.0
//!         RS: 0.1
//! ```
//!
//! Output:
//! ```spice
//! .model DMOD D (IS=1e-12 N=1.0 RS=0.1)
//! ```

use compact_str::CompactString;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use super::common::Identifier;
use crate::lexer::Span;

/// SPICE model card definition
///
/// Declares semiconductor physics parameters for a device model.
/// ALL fields are REQUIRED - no defaults, fail loudly if missing.
///
/// This represents the PDK-provided physics that the compiler cannot calculate
/// from geometry (e.g., saturation current IS, threshold voltage VTO, etc.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpiceModelDefinition {
    /// Model name (e.g., DMOD, NMOS_180, PMOS_TT)
    /// Used in device definitions: `model: DMOD`
    pub name: Identifier,

    /// SPICE model type (diode, nmos, pmos, npn, pnp, etc.)
    /// Maps to SPICE model card type (.model NAME <TYPE> (...))
    /// REQUIRED - no default guessing
    pub model_type: CompactString,

    /// Model parameters (IS, N, RS, BV, VTO, LAMBDA, etc.)
    /// Key-value pairs: parameter name -> numeric value
    /// REQUIRED - empty map is invalid (why define a model with no params?)
    pub parameters: FxHashMap<CompactString, f64>,

    /// Whether this model is exported (accessible from other modules)
    pub is_exported: bool,

    /// Source location for error reporting
    pub span: Span,
}

impl SpiceModelDefinition {
    /// Create a new SPICE model definition
    ///
    /// ALL parameters are REQUIRED. No defaults. Fail loudly.
    pub fn new(
        name: Identifier,
        model_type: CompactString,
        parameters: FxHashMap<CompactString, f64>,
        is_exported: bool,
        span: Span,
    ) -> Result<Self, String> {
        // Validate: model_type must not be empty
        if model_type.is_empty() {
            return Err(format!(
                "SPICE model '{}' missing REQUIRED field 'type'. Add 'type: diode', 'type: nmos', etc.",
                name.name
            ));
        }

        // Validate: parameters must not be empty
        if parameters.is_empty() {
            return Err(format!(
                "SPICE model '{}' has no parameters. Why define a model with no physics?\n\
                 Add 'parameters:' block with at least one parameter (e.g., IS: 1e-12)",
                name.name
            ));
        }

        Ok(Self {
            name,
            model_type,
            parameters,
            is_exported,
            span,
        })
    }

    /// Get a parameter value by name
    ///
    /// Returns None if parameter not defined (caller decides how to handle)
    pub fn get_parameter(&self, param_name: &str) -> Option<f64> {
        self.parameters.get(param_name).copied()
    }

    /// Format as SPICE model card
    ///
    /// Example: .model DMOD D (IS=1e-12 N=1.0 RS=0.1 BV=40.0)
    pub fn to_spice_card(&self) -> String {
        let mut card = format!(".model {} {}", self.name.name, self.model_type);

        if !self.parameters.is_empty() {
            card.push_str(" (");
            let params: Vec<String> = self
                .parameters
                .iter()
                .map(|(k, v)| format!("{}={}", k, format_spice_value(*v)))
                .collect();
            card.push_str(&params.join(" "));
            card.push(')');
        }

        card
    }
}

/// Format a numeric value for SPICE output
///
/// Uses scientific notation for very small/large values, otherwise fixed-point
fn format_spice_value(value: f64) -> String {
    if value.abs() < 1e-3 || value.abs() > 1e6 {
        format!("{:.2e}", value)
    } else {
        format!("{:.6}", value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spice_model_creation() {
        let name = Identifier::new("DMOD".into(), Span::new(0, 0));
        let mut params = FxHashMap::default();
        params.insert("IS".into(), 1e-12);
        params.insert("N".into(), 1.0);
        params.insert("RS".into(), 0.1);

        let model = SpiceModelDefinition::new(
            name,
            "D".into(),
            params,
            true,
            Span::new(0, 0),
        ).unwrap();

        assert_eq!(model.model_type, "D");
        assert_eq!(model.parameters.len(), 3);
        assert_eq!(model.get_parameter("IS"), Some(1e-12));
    }

    #[test]
    fn test_empty_model_type_fails() {
        let name = Identifier::new("DMOD".into(), Span::new(0, 0));
        let mut params = FxHashMap::default();
        params.insert("IS".into(), 1e-12);

        let result = SpiceModelDefinition::new(
            name,
            "".into(),
            params,
            true,
            Span::new(0, 0),
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("missing REQUIRED field 'type'"));
    }

    #[test]
    fn test_empty_parameters_fails() {
        let name = Identifier::new("DMOD".into(), Span::new(0, 0));
        let params = FxHashMap::default();

        let result = SpiceModelDefinition::new(
            name,
            "D".into(),
            params,
            true,
            Span::new(0, 0),
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("has no parameters"));
    }

    #[test]
    fn test_spice_card_formatting() {
        let name = Identifier::new("DMOD".into(), Span::new(0, 0));
        let mut params = FxHashMap::default();
        params.insert("IS".into(), 1e-12);
        params.insert("N".into(), 1.0);

        let model = SpiceModelDefinition::new(
            name,
            "D".into(),
            params,
            true,
            Span::new(0, 0),
        ).unwrap();

        let card = model.to_spice_card();
        assert!(card.starts_with(".model DMOD D ("));
        assert!(card.contains("IS="));
        assert!(card.contains("N="));
    }
}
