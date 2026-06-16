//! Prelude loader for Hardware Script
//!
//! This module handles automatic loading of primitive definitions (units and math constants)
//! that are available globally without import statements.
//!
//! The prelude consists of:
//! - units.hw: SI unit definitions (µF, MHz, mm, etc.)
//! - math.hw: Mathematical and physical constants (PI, E, SPEED_OF_LIGHT, etc.)

use compact_str::CompactString;
use hwc_parser::{Definition, Lexer, Parser, UnitDefinition};
use rustc_hash::FxHashMap;

/// Embedded primitive files (compiled into binary)
const UNITS_HW: &str = include_str!("../../../stdlib/primitives/units.hw");
const MATH_HW: &str = include_str!("../../../stdlib/primitives/math.hw");

/// Prelude definitions loaded at compiler startup
pub struct Prelude {
    pub units: Vec<UnitDefinition>,
    pub constants: FxHashMap<CompactString, f64>,
}

impl Prelude {
    /// Load the prelude from embedded primitive files
    pub fn load() -> Result<Self, PreludeError> {
        let units = Self::load_units()?;
        let constants = Self::load_constants()?;

        Ok(Self { units, constants })
    }

    /// Load unit definitions from primitives/units.hw
    fn load_units() -> Result<Vec<UnitDefinition>, PreludeError> {
        // Tokenize
        let lexer = Lexer::new(UNITS_HW);
        let tokens = lexer.tokenize().map_err(|e| {
            PreludeError::ParseError(format!("Failed to tokenize units.hw: {:?}", e))
        })?;

        // Parse
        let collector =
            crate::DiagnosticCollector::new_with_file(UNITS_HW, "@std/primitives/units", 20);
        let mut parser = Parser::new(tokens);
        let program = parser.parse(&collector);

        if collector.has_errors() {
            return Err(PreludeError::ParseError(format!(
                "Failed to parse units.hw: {}",
                collector.summary()
            )));
        }

        // Extract unit definitions
        let mut units = Vec::new();
        for definition in program.definitions {
            if let Definition::Unit(unit) = definition {
                units.push(unit);
            }
        }

        if units.is_empty() {
            return Err(PreludeError::NoUnitsFound);
        }

        Ok(units)
    }

    /// Load mathematical constants from primitives/math.hw
    fn load_constants() -> Result<FxHashMap<CompactString, f64>, PreludeError> {
        // Tokenize
        let lexer = Lexer::new(MATH_HW);
        let tokens = lexer.tokenize().map_err(|e| {
            PreludeError::ParseError(format!("Failed to tokenize math.hw: {:?}", e))
        })?;

        // Parse
        let collector =
            crate::DiagnosticCollector::new_with_file(MATH_HW, "@std/primitives/math", 20);
        let mut parser = Parser::new(tokens);
        let program = parser.parse(&collector);

        if collector.has_errors() {
            return Err(PreludeError::ParseError(format!(
                "Failed to parse math.hw: {}",
                collector.summary()
            )));
        }

        // Extract constant definitions
        let mut constants = FxHashMap::default();
        for definition in program.definitions {
            if let Definition::Const(const_def) = definition {
                constants.insert(const_def.name, const_def.value);
            }
        }

        if constants.is_empty() {
            return Err(PreludeError::NoConstantsFound);
        }

        Ok(constants)
    }
}

/// Errors that can occur during prelude loading
#[derive(Debug, Clone)]
pub enum PreludeError {
    ParseError(String),
    NoUnitsFound,
    NoConstantsFound,
}

impl std::fmt::Display for PreludeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::ParseError(msg) => write!(f, "Failed to parse prelude: {}", msg),
            Self::NoUnitsFound => write!(f, "No unit definitions found in primitives/units.hw"),
            Self::NoConstantsFound => write!(f, "No constants found in primitives/math.hw"),
        }
    }
}

impl std::error::Error for PreludeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prelude_loads() {
        let prelude = Prelude::load();
        if let Err(e) = &prelude {
            eprintln!("Prelude load error: {}", e);
        }
        assert!(
            prelude.is_ok(),
            "Prelude should load successfully: {:?}",
            prelude.err()
        );

        let prelude = prelude.unwrap();
        assert!(!prelude.units.is_empty(), "Should have unit definitions");
        assert!(!prelude.constants.is_empty(), "Should have constants");
    }

    #[test]
    fn test_units_loaded() {
        let prelude = Prelude::load().unwrap();

        // Check for some expected units
        let symbols: Vec<&str> = prelude.units.iter().map(|u| u.symbol.as_str()).collect();
        assert!(
            symbols.contains(&"µF") || symbols.contains(&"F"),
            "Should contain capacitance units"
        );
        assert!(
            symbols.contains(&"mm") || symbols.contains(&"m"),
            "Should contain length units"
        );
    }

    #[test]
    fn test_constants_loaded() {
        let prelude = Prelude::load().unwrap();

        // Check for expected constants
        assert!(
            prelude.constants.contains_key("PI"),
            "Should have PI constant"
        );
        assert!(
            prelude.constants.contains_key("E"),
            "Should have E constant"
        );
        assert!(
            prelude.constants.contains_key("SPEED_OF_LIGHT"),
            "Should have SPEED_OF_LIGHT constant"
        );
    }
}
