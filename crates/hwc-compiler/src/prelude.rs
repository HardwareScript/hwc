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

    /// Build a UnitRegistry from the prelude unit definitions.
    pub fn build_unit_registry(&self) -> hwc_types::UnitRegistry {
        let info: Vec<hwc_types::UnitInfo> = self
            .units
            .iter()
            .map(|d| hwc_types::UnitInfo {
                symbol: d.symbol.clone(),
                aliases: d.aliases.clone(),
                multiplier: d.multiplier,
                dimension: d.dimension.clone(),
            })
            .collect();
        hwc_types::UnitRegistry::new(info)
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

        // Extract unit definitions from arena
        let mut units = Vec::new();
        for definition in &program.definitions {
            if let Definition::Unit(unit_id) = definition {
                units.push(program.arena.unit_defs[*unit_id].clone());
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

        // Extract constant definitions from arena
        let mut constants = FxHashMap::default();
        for definition in &program.definitions {
            if let Definition::Const(const_id) = definition {
                let const_def = &program.arena.const_defs[*const_id];
                constants.insert(const_def.name.clone(), const_def.value);
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
