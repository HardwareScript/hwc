//! Bridge Resolver: Material Transition Management
//!
//! This module implements the Bridge System from BRIDGE-IMPLEMENTATION.md Phase 1.
//! It handles the resolution of material transitions (e.g., Silicon-to-Metal contacts)
//! using a three-tier priority system:
//!
//! 1. Explicit user override (highest priority)
//! 2. Profile bridge table (standard)
//! 3. Standard library default (fallback)
//!
//! The compiler remains a "dumb enforcer" - all chemistry knowledge lives in profiles.

use compact_str::CompactString;
use hwc_parser::ast::profile::ProfileDefinition;
use rustc_hash::FxHashMap;

/// A compound bridge stack: interface material + fill material
///
/// Physical Reality: In real chips, the bridge (e.g., Silicide) is only a thin
/// "crust" (typically ~50nm) at the interface. The rest of the via is
/// filled with a different material (e.g., Tungsten).
///

#[derive(Debug, Clone, PartialEq)]
pub struct BridgeStack {
    /// The bridge interface material (e.g., "Titanium_Silicide")
    /// This is the thin layer that touches the source material
    pub interface_material: CompactString,

    /// Interface thickness in nanometers (typically ~50nm)
    pub interface_thickness_nm: f64,

    /// The via fill material (e.g., "Tungsten")
    /// This fills the rest of the via body
    pub fill_material: CompactString,
}

/// Bridge lookup table: maps (from_material, to_material) → BridgeStack
///
/// This is built from profile definitions and used during via insertion.
#[derive(Debug, Clone, Default)]
pub struct BridgeTable {
    /// Map of (from, to) → BridgeStack
    /// Key format: "Silicon_N:Copper" → BridgeStack
    rules: FxHashMap<CompactString, BridgeStack>,

    /// Default via fill material (used when no specific fill is defined)
    pub default_via_fill: Option<CompactString>,
}

impl BridgeTable {
    /// Create a new empty bridge table
    pub fn new() -> Self {
        Self {
            rules: FxHashMap::default(),
            default_via_fill: None,
        }
    }

    /// Build a bridge table from a profile definition and global symbol table (v0.2.0)
    ///
    /// v0.2.0: Bridge elevation - bridges can now be defined as top-level definitions
    /// and queried from the symbol table. Profile-nested bridges are still supported
    /// for backward compatibility but are deprecated.
    ///
    /// Priority: Symbol table bridges > Profile-nested bridges
    pub fn from_profile_and_symbol_table(
        profile: Option<&ProfileDefinition>,
        symbol_table: Option<&crate::SymbolTable>,
    ) -> Self {
        let mut table = Self::new();

        // Extract default via fill from profile
        if let Some(p) = profile {
            table.default_via_fill = p
                .via
                .as_ref()
                .and_then(|v| v.default_via_fill.as_ref())
                .map(|id| id.name.clone());
        }

        // v0.2.0: Load bridges from global symbol table (highest priority)
        if let Some(st) = symbol_table {
            for bridge_def in st.get_all_bridges() {
                let key = Self::make_key(&bridge_def.from, &bridge_def.to);

                // Extract interface thickness (default to 50nm if not specified)
                let interface_thickness_nm = bridge_def
                    .interface_thickness
                    .as_ref()
                    .map(|m| match m.unit {
                        hwc_parser::Unit::Millimeter => m.value * 1_000_000.0,
                        hwc_parser::Unit::Micrometer => m.value * 1_000.0,
                        hwc_parser::Unit::Centimeter => m.value * 10_000_000.0,
                        _ => m.value,
                    })
                    .unwrap_or(50.0);

                // Extract fill material (default to interface material if not specified)
                let fill_material = bridge_def
                    .fill_material
                    .clone()
                    .unwrap_or_else(|| bridge_def.interface_material.clone());

                let stack = BridgeStack {
                    interface_material: bridge_def.interface_material.clone(),
                    interface_thickness_nm,
                    fill_material,
                };

                table.rules.insert(key, stack);
            }
        }

        // Legacy: Load bridges from profile (backward compatibility, lower priority)
        // Only add if not already present from symbol table
        if let Some(p) = profile {
            for bridge_rule in &p.bridges {
                let key = Self::make_key(&bridge_rule.from, &bridge_rule.to);

                // Skip if already defined by symbol table
                if table.rules.contains_key(&key) {
                    continue;
                }

                // Extract interface thickness (default to 50nm if not specified)
                let interface_thickness_nm = bridge_rule
                    .interface_thickness
                    .as_ref()
                    .map(|m| match m.unit {
                        hwc_parser::Unit::Millimeter => m.value * 1_000_000.0,
                        hwc_parser::Unit::Micrometer => m.value * 1_000.0,
                        hwc_parser::Unit::Centimeter => m.value * 10_000_000.0,
                        _ => m.value,
                    })
                    .unwrap_or(50.0);

                // Extract fill material (default to interface material if not specified)
                let fill_material = bridge_rule
                    .fill_material
                    .clone()
                    .unwrap_or_else(|| bridge_rule.interface_material.clone());

                let stack = BridgeStack {
                    interface_material: bridge_rule.interface_material.clone(),
                    interface_thickness_nm,
                    fill_material,
                };

                table.rules.insert(key, stack);
            }
        }

        table
    }

    /// Build a bridge table from a profile definition (legacy method for backward compat)
    ///
    /// Deprecated: Use `from_profile_and_symbol_table` instead
    pub fn from_profile(profile: &ProfileDefinition) -> Self {
        Self::from_profile_and_symbol_table(Some(profile), None)
    }

    /// Look up a bridge stack for a material transition
    ///
    /// Returns None if no bridge is defined for this transition.
    pub fn lookup(&self, from_material: &str, to_material: &str) -> Option<&BridgeStack> {
        let key = Self::make_key(from_material, to_material);
        self.rules.get(&key)
    }

    /// Add a bridge rule to the table
    pub fn add_rule(&mut self, from: CompactString, to: CompactString, stack: BridgeStack) {
        let key = Self::make_key(&from, &to);
        self.rules.insert(key, stack);
    }

    /// Create a lookup key from material names
    fn make_key(from: &str, to: &str) -> CompactString {
        format!("{}:{}", from, to).into()
    }

    /// Get all bridge rules (for debugging/inspection)
    pub fn all_rules(&self) -> &FxHashMap<CompactString, BridgeStack> {
        &self.rules
    }
}

/// Bridge resolution errors
#[derive(Debug, Clone, PartialEq)]
pub enum BridgeError {
    /// No bridge defined for this material transition
    ForbiddenJunction {
        from: CompactString,
        to: CompactString,
        suggestion: CompactString,
    },

    /// Bridge material not found in material database
    UnknownBridgeMaterial { material: CompactString },
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BridgeError::ForbiddenJunction {
                from,
                to,
                suggestion,
            } => {
                write!(f, "Forbidden junction: {} to {} ({})", from, to, suggestion)
            }
            BridgeError::UnknownBridgeMaterial { material } => {
                write!(f, "Unknown bridge material: {}", material)
            }
        }
    }
}

impl std::error::Error for BridgeError {}

/// Resolve a bridge for a material transition using the three-tier priority system
///
/// Priority 1: Explicit user override (highest)
/// Priority 2: Profile bridge table (standard)
/// Priority 3: Standard library default (fallback)
///
/// Returns an error if no bridge is defined for this transition.
pub fn resolve_bridge(
    from_material: &str,
    to_material: &str,
    profile_table: Option<&BridgeTable>,
    stdlib_table: Option<&BridgeTable>,
    explicit_override: Option<&str>,
) -> Result<BridgeStack, BridgeError> {
    // v0.1.7: Fast path for same materials (no bridge needed)
    if from_material == to_material {
        return Ok(BridgeStack {
            interface_material: from_material.into(),
            interface_thickness_nm: 0.0,
            fill_material: from_material.into(),
        });
    }

    // Priority 1: Explicit user override
    if let Some(bridge_material) = explicit_override {
        // Use the override as interface material, with default fill
        let fill_material = profile_table
            .and_then(|t| t.default_via_fill.clone())
            .or_else(|| stdlib_table.and_then(|t| t.default_via_fill.clone()))
            .unwrap_or_else(|| bridge_material.into());

        return Ok(BridgeStack {
            interface_material: bridge_material.into(),
            interface_thickness_nm: 50.0, // Default thickness
            fill_material,
        });
    }

    // Priority 2: Profile bridge table
    if let Some(table) = profile_table {
        if let Some(stack) = table.lookup(from_material, to_material) {
            return Ok(stack.clone());
        }
    }

    // Priority 3: Standard library default
    if let Some(table) = stdlib_table {
        if let Some(stack) = table.lookup(from_material, to_material) {
            return Ok(stack.clone());
        }
    }

    // Error: No bridge defined
    Err(BridgeError::ForbiddenJunction {
        from: from_material.into(),
        to: to_material.into(),
        suggestion: "Define a bridge in your profile or use an explicit bridge: declaration".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bridge_table_lookup() {
        let mut table = BridgeTable::new();

        table.add_rule(
            "Silicon".into(),
            "Copper".into(),
            BridgeStack {
                interface_material: "Titanium_Silicide".into(),
                interface_thickness_nm: 50.0,
                fill_material: "Tungsten".into(),
            },
        );

        let stack = table.lookup("Silicon", "Copper").unwrap();
        assert_eq!(stack.interface_material, "Titanium_Silicide");
        assert_eq!(stack.fill_material, "Tungsten");
    }

    #[test]
    fn test_bridge_resolution_priority() {
        let mut profile_table = BridgeTable::new();
        profile_table.add_rule(
            "Silicon".into(),
            "Copper".into(),
            BridgeStack {
                interface_material: "Cobalt_Silicide".into(),
                interface_thickness_nm: 50.0,
                fill_material: "Tungsten".into(),
            },
        );

        let mut stdlib_table = BridgeTable::new();
        stdlib_table.add_rule(
            "Silicon".into(),
            "Copper".into(),
            BridgeStack {
                interface_material: "Generic_Silicide".into(),
                interface_thickness_nm: 50.0,
                fill_material: "Generic_Via_Fill".into(),
            },
        );

        // Test Priority 2: Profile table
        let stack = resolve_bridge(
            "Silicon",
            "Copper",
            Some(&profile_table),
            Some(&stdlib_table),
            None,
        )
        .unwrap();
        assert_eq!(stack.interface_material, "Cobalt_Silicide");

        // Test Priority 1: Explicit override
        let stack = resolve_bridge(
            "Silicon",
            "Copper",
            Some(&profile_table),
            Some(&stdlib_table),
            Some("Tungsten_Silicide"),
        )
        .unwrap();
        assert_eq!(stack.interface_material, "Tungsten_Silicide");

        // Test Priority 3: Stdlib fallback
        let stack = resolve_bridge("Silicon", "Copper", None, Some(&stdlib_table), None).unwrap();
        assert_eq!(stack.interface_material, "Generic_Silicide");
    }

    #[test]
    fn test_forbidden_junction() {
        let result = resolve_bridge("Silicon", "Copper", None, None, None);

        assert!(result.is_err());
        match result {
            Err(BridgeError::ForbiddenJunction { from, to, .. }) => {
                assert_eq!(from, "Silicon");
                assert_eq!(to, "Copper");
            }
            _ => panic!("Expected ForbiddenJunction error"),
        }
    }
}
