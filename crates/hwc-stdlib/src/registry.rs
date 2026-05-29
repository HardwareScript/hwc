//! Unit registry for fast lookup and validation

use compact_str::CompactString;
use hwc_parser::UnitDefinition;
use rustc_hash::FxHashMap;

/// Fast lookup registry for unit definitions
pub struct UnitRegistry {
    /// Map from unit symbol/alias to unit definition
    units: FxHashMap<CompactString, UnitDefinition>,
}

impl UnitRegistry {
    /// Create a new registry from unit definitions
    pub fn new(definitions: Vec<UnitDefinition>) -> Self {
        let mut units = FxHashMap::default();

        for def in definitions {
            // Register by symbol
            units.insert(def.symbol.clone(), def.clone());

            // Register by aliases
            for alias in &def.aliases {
                units.insert(alias.clone(), def.clone());
            }
        }

        Self { units }
    }

    /// Check if a unit string is defined in the registry
    pub fn is_defined(&self, unit_str: &str) -> bool {
        self.units.contains_key(unit_str)
    }

    /// Get the unit definition for a unit string
    pub fn get(&self, unit_str: &str) -> Option<&UnitDefinition> {
        self.units.get(unit_str)
    }

    /// Convert a value to its base SI unit
    pub fn to_base_si(&self, value: f64, unit_str: &str) -> Option<f64> {
        self.get(unit_str).and_then(|def| def.to_base_si(value))
    }

    /// Get the dimension of a unit
    pub fn get_dimension(&self, unit_str: &str) -> Option<&str> {
        self.get(unit_str).map(|def| def.dimension.as_str())
    }

    /// Get all registered unit symbols
    pub fn all_symbols(&self) -> Vec<&str> {
        self.units.keys().map(|s| s.as_str()).collect()
    }

    /// Get the number of registered units
    pub fn len(&self) -> usize {
        self.units.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hwc_parser::{Identifier, Span};

    fn create_test_unit(
        name: &str,
        symbol: &str,
        aliases: Vec<&str>,
        multiplier: Option<f64>,
    ) -> UnitDefinition {
        UnitDefinition {
            name: Identifier::with_dummy_span(name),
            symbol: symbol.to_string().into(),
            aliases: aliases.iter().map(|s| (*s).into()).collect(),
            base_si: Some("F".into()),
            multiplier,
            dimension: "capacitance".into(),
            description: None,
            note: None,
            examples: vec![],
            span: Span::new(0, 0),
        }
    }

    #[test]
    fn test_registry_lookup() {
        let units = vec![
            create_test_unit("Microfarad", "µF", vec!["uF"], Some(1e-6)),
            create_test_unit("Nanofarad", "nF", vec![], Some(1e-9)),
        ];

        let registry = UnitRegistry::new(units);

        assert!(registry.is_defined("µF"));
        assert!(registry.is_defined("uF")); // alias
        assert!(registry.is_defined("nF"));
        assert!(!registry.is_defined("pF")); // not registered
    }

    #[test]
    fn test_registry_conversion() {
        let units = vec![create_test_unit("Microfarad", "µF", vec!["uF"], Some(1e-6))];

        let registry = UnitRegistry::new(units);

        // 100µF = 100 * 1e-6 = 1e-4 F
        let result = registry.to_base_si(100.0, "µF").unwrap();
        assert!(
            (result - 1e-4).abs() < 1e-10,
            "Expected ~1e-4, got {}",
            result
        );

        let result_alias = registry.to_base_si(100.0, "uF").unwrap();
        assert!((result_alias - 1e-4).abs() < 1e-10, "Alias should work");
    }

    #[test]
    fn test_registry_dimension() {
        let units = vec![create_test_unit("Microfarad", "µF", vec![], Some(1e-6))];

        let registry = UnitRegistry::new(units);

        assert_eq!(registry.get_dimension("µF"), Some("capacitance"));
    }
}
