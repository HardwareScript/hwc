//! Prelude loader for Hardware Script

use compact_str::CompactString;
use hwc_types::{UnitInfo, UnitRegistry};
use rustc_hash::FxHashMap;

/// Prelude definitions loaded at compiler startup
pub struct Prelude {
    pub units: Vec<UnitInfo>,
    pub constants: FxHashMap<CompactString, f64>,
}

impl Prelude {
    /// Load the prelude from embedded primitive files
    pub fn load() -> Result<Self, PreludeError> {
        let units = hwc_stdlib::load_stdlib().unwrap_or_default();

        let mut constants = FxHashMap::default();
        constants.insert("PI".into(), std::f64::consts::PI);
        constants.insert("E".into(), std::f64::consts::E);
        constants.insert("SPEED_OF_LIGHT".into(), 299_792_458.0);

        Ok(Self { units, constants })
    }

    /// Build a UnitRegistry from the prelude unit definitions, guaranteed to include standard SI units.
    pub fn build_unit_registry(&self) -> UnitRegistry {
        if self.units.is_empty() {
            UnitRegistry::standard()
        } else {
            let reg = UnitRegistry::standard();
            let mut all_defs = Vec::new();
            for sym in reg.all_symbols() {
                if let Some(info) = reg.get(sym) {
                    all_defs.push(info.clone());
                }
            }
            for u in &self.units {
                if !all_defs.iter().any(|d| d.symbol == u.symbol) {
                    all_defs.push(u.clone());
                }
            }
            UnitRegistry::new(all_defs)
        }
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
