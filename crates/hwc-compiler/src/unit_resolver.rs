//! Unit resolver for Hardware Script
//!
//! This module resolves Custom units to their core equivalents using the prelude.
//! For example, "um" (Custom) → Micrometer (core unit) via alias lookup.

use compact_str::CompactString;
use hwc_parser::{Unit, UnitDefinition};
use rustc_hash::FxHashMap;

/// Unit resolver that maps custom unit strings to core units
pub struct UnitResolver {
    /// Map from unit string (symbol or alias) to core unit
    alias_map: FxHashMap<CompactString, Unit>,
}

impl UnitResolver {
    /// Create a new unit resolver from prelude unit definitions
    pub fn from_prelude(units: &[UnitDefinition]) -> Self {
        let mut alias_map = FxHashMap::default();

        for unit_def in units {
            // Determine the core unit type based on dimension
            let core_unit = match unit_def.dimension.as_str() {
                "length" => {
                    // Map length units to their core equivalents
                    match unit_def.symbol.as_str() {
                        "m" => Some(Unit::Millimeter), // Will be converted with multiplier
                        "mm" => Some(Unit::Millimeter),
                        "cm" => Some(Unit::Centimeter),
                        "µm" | "um" => Some(Unit::Micrometer),
                        "nm" => Some(Unit::Micrometer), // Will be converted with multiplier
                        _ => None,
                    }
                }
                _ => None, // Non-length units stay as Custom
            };

            if let Some(core) = core_unit {
                // Register the symbol
                alias_map.insert(unit_def.symbol.clone(), core.clone());

                // Register all aliases
                for alias in &unit_def.aliases {
                    alias_map.insert(alias.clone(), core.clone());
                }
            }
        }

        Self { alias_map }
    }

    /// Resolve a unit, converting Custom units to core units if possible
    pub fn resolve(&self, unit: &Unit) -> Unit {
        match unit {
            Unit::Custom(s) => {
                // Try to resolve via alias map
                self.alias_map
                    .get(s.as_str())
                    .cloned()
                    .unwrap_or_else(|| unit.clone())
            }
            // Core units pass through unchanged
            _ => unit.clone(),
        }
    }

    /// Check if a custom unit string can be resolved
    pub fn can_resolve(&self, unit_str: &str) -> bool {
        self.alias_map.contains_key(unit_str)
    }

    /// Get the core unit for a custom unit string (if resolvable)
    pub fn get_core_unit(&self, unit_str: &str) -> Option<&Unit> {
        self.alias_map.get(unit_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hwc_parser::lexer::Span;

    use hwc_parser::Identifier;

    fn create_test_unit_def(symbol: &str, aliases: Vec<&str>, dimension: &str) -> UnitDefinition {
        UnitDefinition {
            name: Identifier::with_dummy_span(symbol),
            symbol: symbol.to_string().into(),
            aliases: aliases.iter().map(|s| (*s).into()).collect(),
            base_si: Some("m".into()),
            multiplier: Some(1.0),
            dimension: dimension.to_string().into(),
            description: None,
            note: None,
            examples: Vec::new(),
            span: Span::new(0, 0),
        }
    }

    #[test]
    fn test_resolve_micrometer_alias() {
        let units = vec![create_test_unit_def("µm", vec!["um"], "length")];
        let resolver = UnitResolver::from_prelude(&units);

        let custom_um = Unit::Custom("um".into());
        let resolved = resolver.resolve(&custom_um);

        assert_eq!(resolved, Unit::Micrometer);
    }

    #[test]
    fn test_resolve_symbol() {
        let units = vec![create_test_unit_def("µm", vec!["um"], "length")];
        let resolver = UnitResolver::from_prelude(&units);

        let custom_um = Unit::Custom("µm".into());
        let resolved = resolver.resolve(&custom_um);

        assert_eq!(resolved, Unit::Micrometer);
    }

    #[test]
    fn test_core_unit_unchanged() {
        let units = vec![create_test_unit_def("µm", vec!["um"], "length")];
        let resolver = UnitResolver::from_prelude(&units);

        let core_mm = Unit::Millimeter;
        let resolved = resolver.resolve(&core_mm);

        assert_eq!(resolved, Unit::Millimeter);
    }

    #[test]
    fn test_unresolvable_custom_unchanged() {
        let units = vec![create_test_unit_def("µm", vec!["um"], "length")];
        let resolver = UnitResolver::from_prelude(&units);

        let custom_unknown = Unit::Custom("xyz".into());
        let resolved = resolver.resolve(&custom_unknown);

        assert_eq!(resolved, Unit::Custom("xyz".into()));
    }

    #[test]
    fn test_can_resolve() {
        let units = vec![create_test_unit_def("µm", vec!["um"], "length")];
        let resolver = UnitResolver::from_prelude(&units);

        assert!(resolver.can_resolve("um"));
        assert!(resolver.can_resolve("µm"));
        assert!(!resolver.can_resolve("xyz"));
    }
}
