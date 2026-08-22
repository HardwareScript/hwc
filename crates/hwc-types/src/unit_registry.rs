//! Unit registry for fast lookup and validation.
//!
//! This is the single source of truth for unit conversions across the compiler.
//! The registry is built once from stdlib unit definitions and shared via reference.

use compact_str::CompactString;
use rustc_hash::FxHashMap;

/// Minimal unit information needed for registry operations.
///
/// This is decoupled from the parser's `UnitDefinition` to avoid circular
/// crate dependencies. The compiler converts `UnitDefinition` → `UnitInfo`
/// when building the registry.
#[derive(Debug, Clone)]
pub struct UnitInfo {
    pub symbol: CompactString,
    pub aliases: Vec<CompactString>,
    pub multiplier: Option<f64>,
    pub dimension: CompactString,
}

impl UnitInfo {
    /// Convert a value in this unit to its base SI value.
    pub fn to_base_si(&self, value: f64) -> Option<f64> {
        self.multiplier.map(|m| value * m)
    }
}

/// Fast lookup registry for unit definitions loaded from stdlib/primitives/units.hw.
///
/// Built once during compilation and immutable after construction.
/// Thread-safe via `Arc` in compiler context.
#[derive(Debug, Clone)]
pub struct UnitRegistry {
    units: FxHashMap<CompactString, UnitInfo>,
}

impl UnitRegistry {
    /// Create a new registry from unit definitions.
    pub fn new(definitions: Vec<UnitInfo>) -> Self {
        let mut units = FxHashMap::default();
        for def in definitions {
            units.insert(def.symbol.clone(), def.clone());
            for alias in &def.aliases {
                units.insert(alias.clone(), def.clone());
            }
        }
        Self { units }
    }

    /// Check if a unit string is defined.
    pub fn is_defined(&self, unit_str: &str) -> bool {
        self.units.contains_key(unit_str)
    }

    /// Get the unit info for a unit string.
    pub fn get(&self, unit_str: &str) -> Option<&UnitInfo> {
        self.units.get(unit_str)
    }

    /// Convert a value to its base SI unit.
    pub fn to_base_si(&self, value: f64, unit_str: &str) -> Option<f64> {
        self.get(unit_str).and_then(|def| def.to_base_si(value))
    }

    /// Get the dimension of a unit (e.g., "current", "voltage").
    pub fn get_dimension(&self, unit_str: &str) -> Option<&str> {
        self.get(unit_str).map(|def| def.dimension.as_str())
    }

    /// Validate that a unit matches the expected dimension.
    pub fn validate_dimension(
        &self,
        unit_str: &str,
        expected_dimension: &str,
    ) -> Result<(), String> {
        match self.get_dimension(unit_str) {
            Some(dim) if dim == expected_dimension => Ok(()),
            Some(dim) => Err(format!(
                "Unit '{}' has dimension '{}', expected '{}'",
                unit_str, dim, expected_dimension
            )),
            None => Err(format!("Unit '{}' is not defined", unit_str)),
        }
    }

    /// Convert with dimension validation.
    pub fn convert_with_validation(
        &self,
        value: f64,
        unit_str: &str,
        expected_dimension: &str,
    ) -> Result<f64, String> {
        self.validate_dimension(unit_str, expected_dimension)?;
        self.to_base_si(value, unit_str)
            .ok_or_else(|| format!("Cannot convert unit '{}' (no multiplier defined)", unit_str))
    }

    /// Picometers per SI base meter (1 m = 10^12 pm)
    pub const PICOMETERS_PER_METER: f64 = 1_000_000_000_000.0;

    /// Nanometers per SI base meter (1 m = 10^9 nm)
    pub const NANOMETERS_PER_METER: f64 = 1_000_000_000.0;

    /// Convert a distance measurement value to picometers using base SI meters.
    pub fn to_picometers(&self, value: f64, unit_str: &str) -> Result<i64, String> {
        let meters = self.convert_with_validation(value, unit_str, "length")?;
        Ok((meters * Self::PICOMETERS_PER_METER).round() as i64)
    }

    /// Convert a distance measurement value to nanometers using base SI meters.
    pub fn to_nanometers(&self, value: f64, unit_str: &str) -> Result<i64, String> {
        let meters = self.convert_with_validation(value, unit_str, "length")?;
        Ok((meters * Self::NANOMETERS_PER_METER).round() as i64)
    }

    /// Get all registered unit symbols.
    pub fn all_symbols(&self) -> Vec<&str> {
        self.units.keys().map(|s| s.as_str()).collect()
    }

    /// Get the number of registered units.
    pub fn len(&self) -> usize {
        self.units.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_unit(
        symbol: &str,
        aliases: Vec<&str>,
        multiplier: Option<f64>,
        dimension: &str,
    ) -> UnitInfo {
        UnitInfo {
            symbol: symbol.into(),
            aliases: aliases.into_iter().map(|s| s.into()).collect(),
            multiplier,
            dimension: dimension.into(),
        }
    }

    #[test]
    fn test_registry_lookup() {
        let units = vec![
            test_unit("mA", vec!["milliamp"], Some(1e-3), "current"),
            test_unit("µA", vec!["uA"], Some(1e-6), "current"),
        ];
        let registry = UnitRegistry::new(units);

        assert!(registry.is_defined("mA"));
        assert!(registry.is_defined("milliamp"));
        assert!(registry.is_defined("µA"));
        assert!(!registry.is_defined("kA"));
    }

    #[test]
    fn test_registry_conversion() {
        let units = vec![test_unit("mA", vec![], Some(1e-3), "current")];
        let registry = UnitRegistry::new(units);

        let result = registry.to_base_si(100.0, "mA").unwrap();
        assert!((result - 0.1).abs() < 1e-10);
    }

    #[test]
    fn test_dimension_validation() {
        let units = vec![
            test_unit("mA", vec![], Some(1e-3), "current"),
            test_unit("mV", vec![], Some(1e-3), "voltage"),
        ];
        let registry = UnitRegistry::new(units);

        assert!(registry.validate_dimension("mA", "current").is_ok());
        assert!(registry.validate_dimension("mA", "voltage").is_err());
        assert!(registry.validate_dimension("mV", "voltage").is_ok());
    }

    #[test]
    fn test_convert_with_validation() {
        let units = vec![test_unit("mA", vec![], Some(1e-3), "current")];
        let registry = UnitRegistry::new(units);

        let result = registry
            .convert_with_validation(50.0, "mA", "current")
            .unwrap();
        assert!((result - 0.05).abs() < 1e-10);

        assert!(registry
            .convert_with_validation(50.0, "mA", "voltage")
            .is_err());
    }
}
