//! Symbol resolution methods (Pass 2)

use super::{
    error::SymbolError,
    layer::{SymbolLayer, SymbolTable},
};
use compact_str::CompactString;
use hwc_parser::{
    logic::{EnumDefinition, LogicDefinition, StructDefinition},
    ComponentDefinition, DeviceDefinition, InterfaceDefinition, MaterialDefinition,
    MechanicalDefinition, ModuleDefinition, PatternDefinition, ProfileDefinition,
    SignalGroupDefinition, StrategyDefinition, TestDefinition,
};

impl SymbolTable {
    /// Generic namespace-aware symbol resolver (God-Tier Refactoring Pattern)
    ///
    /// This centralizes the namespace resolution logic so ALL definition types
    /// get namespace support "for free" without duplicating the logic.
    ///
    /// # How it works:
    /// 1. If `full_name` contains a dot (e.g., "Metals.Copper"), resolve via namespace
    /// 2. Otherwise, use regular authority stack resolution (Local > HPM > Prelude > Core)
    ///
    /// # Returns:
    /// A reference to the definition, avoiding clones and maintaining zero-cost abstraction
    ///
    /// # Example:
    /// ```
    /// // Namespaced: "Metals.Copper" -> Look in "Metals" namespace layer
    /// // Regular: "Copper" -> Search all layers in order
    /// ```
    fn resolve_namespaced_symbol<'a, T>(
        &'a self,
        full_name: &str,
        lookup_fn: impl Fn(&'a SymbolLayer, &str) -> Option<&'a T>,
    ) -> Option<&'a T> {
        // Check for namespaced lookup first (e.g., "Metals.Copper")
        if let Some((layer_index, identifier)) = self.resolve_namespace(full_name) {
            // NAMESPACED LOOKUP: Go straight to the aliased HPM layer
            return self
                .hpm
                .get(layer_index)
                .and_then(|layer| lookup_fn(layer, identifier));
        }

        // REGULAR LOOKUP: Search local -> hpm (rev) -> prelude -> core
        // Local layer (highest priority)
        if let Some(def) = lookup_fn(&self.local, full_name) {
            return Some(def);
        }

        // HPM layers in reverse order (last import wins)
        for layer in self.hpm.iter().rev() {
            if let Some(def) = lookup_fn(layer, full_name) {
                return Some(def);
            }
        }

        // Prelude layer
        if let Some(def) = lookup_fn(&self.prelude, full_name) {
            return Some(def);
        }

        // Core layer (lowest priority)
        lookup_fn(&self.core, full_name)
    }

    /// Get a material definition by name (searches all layers: Local > HPM > Prelude > Core)
    /// Supports namespaced lookups: "Metals.Copper" will look in the "Metals" namespace
    ///
    /// v0.1.6: Supports material aliases (e.g. "M1" -> "Copper").
    /// Alias resolution is recursive and detects circular dependencies.
    pub fn get_material(&self, name: &str) -> Result<&MaterialDefinition, SymbolError> {
        let mut current_name = name;
        let mut visited = rustc_hash::FxHashSet::default();
        visited.insert(current_name.to_string());

        // Recursive alias resolution
        loop {
            // Try to find as a material first
            if let Some(mat) =
                self.resolve_namespaced_symbol(current_name, |layer, n| layer.materials.get(n))
            {
                return Ok(mat);
            }

            // If not a material, check if it's an alias
            if let Some(alias) = self
                .resolve_namespaced_symbol(current_name, |layer, n| layer.material_aliases.get(n))
            {
                let next_name = alias.target.as_str();

                // Detect circular aliases
                if visited.contains(next_name) {
                    return Err(SymbolError::CircularAlias {
                        name: current_name.to_string().into(),
                        target: next_name.to_string().into(),
                        span: (alias.span.start, alias.span.end),
                    });
                }

                // Check depth limit (e.g. 10) to prevent infinite loops even without exact cycles
                if visited.len() > 10 {
                    return Err(SymbolError::AliasDepthExceeded {
                        name: name.to_string().into(),
                        depth: visited.len(),
                    });
                }

                visited.insert(next_name.to_string());
                current_name = next_name;
                continue;
            }

            // Neither material nor alias found
            return Err(SymbolError::UndefinedSymbol {
                name: name.to_string().into(),
                kind: "material",
                span: None,
            });
        }
    }

    /// Get a profile definition by name (searches all layers: Local > HPM > Prelude > Core)
    /// Supports namespaced lookups: "Foundry.TSMC_180nm"
    pub fn get_profile(&self, name: &str) -> Result<&ProfileDefinition, SymbolError> {
        self.resolve_namespaced_symbol(name, |layer, n| layer.profiles.get(n))
            .ok_or_else(|| SymbolError::UndefinedSymbol {
                name: name.to_string().into(),
                kind: "profile",
                span: None,
            })
    }

    /// Get a component definition by name (searches all layers: Local > HPM > Prelude > Core)
    /// Supports namespaced lookups: "Parts.MCU"
    pub fn get_component(&self, name: &str) -> Result<&ComponentDefinition, SymbolError> {
        self.resolve_namespaced_symbol(name, |layer, n| layer.components.get(n))
            .ok_or_else(|| SymbolError::UndefinedSymbol {
                name: name.to_string().into(),
                kind: "component",
                span: None,
            })
    }

    /// Get a module definition by name (searches all layers: Local > HPM > Prelude > Core)
    /// Supports namespaced lookups: "Logic.Adder64"
    pub fn get_module(&self, name: &str) -> Result<&ModuleDefinition, SymbolError> {
        self.resolve_namespaced_symbol(name, |layer, n| layer.modules.get(n))
            .ok_or_else(|| SymbolError::UndefinedSymbol {
                name: name.to_string().into(),
                kind: "module",
                span: None,
            })
    }

    /// Get a mechanical definition by name (searches all layers: Local > HPM > Prelude > Core)
    /// Supports namespaced lookups: "Enclosures.StandardCase"
    pub fn get_mechanical(&self, name: &str) -> Result<&MechanicalDefinition, SymbolError> {
        self.resolve_namespaced_symbol(name, |layer, n| layer.mechanicals.get(n))
            .ok_or_else(|| SymbolError::UndefinedSymbol {
                name: name.to_string().into(),
                kind: "mechanical",
                span: None,
            })
    }

    /// Get an interface definition by name (searches all layers: Local > HPM > Prelude > Core)
    /// Supports namespaced lookups: "Protocols.SPI"
    pub fn get_interface(&self, name: &str) -> Result<&InterfaceDefinition, SymbolError> {
        self.resolve_namespaced_symbol(name, |layer, n| layer.interfaces.get(n))
            .ok_or_else(|| SymbolError::UndefinedSymbol {
                name: name.to_string().into(),
                kind: "interface",
                span: None,
            })
    }

    /// Get a test definition by name (searches all layers: Local > HPM > Prelude > Core)
    /// Supports namespaced lookups: "TestSuites.UnitTests"
    pub fn get_test(&self, name: &str) -> Result<&TestDefinition, SymbolError> {
        self.resolve_namespaced_symbol(name, |layer, n| layer.tests.get(n))
            .ok_or_else(|| SymbolError::UndefinedSymbol {
                name: name.to_string().into(),
                kind: "test",
                span: None,
            })
    }

    /// Get a signal group definition by name (searches all layers: Local > HPM > Prelude > Core)
    /// Supports namespaced lookups: "Buses.DataBus"
    pub fn get_signal_group(&self, name: &str) -> Result<&SignalGroupDefinition, SymbolError> {
        self.resolve_namespaced_symbol(name, |layer, n| layer.signal_groups.get(n))
            .ok_or_else(|| SymbolError::UndefinedSymbol {
                name: name.to_string().into(),
                kind: "signal_group",
                span: None,
            })
    }

    /// Get a pattern definition by name (searches all layers: Local > HPM > Prelude > Core)
    /// Supports namespaced lookups: "Layouts.GridPattern"
    pub fn get_pattern(&self, name: &str) -> Result<&PatternDefinition, SymbolError> {
        self.resolve_namespaced_symbol(name, |layer, n| layer.patterns.get(n))
            .ok_or_else(|| SymbolError::UndefinedSymbol {
                name: name.to_string().into(),
                kind: "pattern",
                span: None,
            })
    }

    /// Get a strategy definition by name (searches all layers: Local > HPM > Prelude > Core)
    /// Supports namespaced lookups: "Routing.ManhattanStrategy"
    pub fn get_strategy(&self, name: &str) -> Result<&StrategyDefinition, SymbolError> {
        self.resolve_namespaced_symbol(name, |layer, n| layer.strategies.get(n))
            .ok_or_else(|| SymbolError::UndefinedSymbol {
                name: name.to_string().into(),
                kind: "strategy",
                span: None,
            })
    }

    /// Get a logic block definition by name (searches all layers: Local > HPM > Prelude > Core)
    /// Supports namespaced lookups: "CPU.ALU"
    pub fn get_logic(&self, name: &str) -> Result<&LogicDefinition, SymbolError> {
        self.resolve_namespaced_symbol(name, |layer, n| layer.logic_blocks.get(n))
            .ok_or_else(|| SymbolError::UndefinedSymbol {
                name: name.to_string().into(),
                kind: "logic",
                span: None,
            })
    }

    /// Get an enum definition by name (searches all layers: Local > HPM > Prelude > Core)
    /// Supports namespaced lookups: "Types.State"
    pub fn get_enum(&self, name: &str) -> Result<&EnumDefinition, SymbolError> {
        self.resolve_namespaced_symbol(name, |layer, n| layer.enums.get(n))
            .ok_or_else(|| SymbolError::UndefinedSymbol {
                name: name.to_string().into(),
                kind: "enum",
                span: None,
            })
    }

    /// Get a struct definition by name (searches all layers: Local > HPM > Prelude > Core)
    /// Supports namespaced lookups: "CPU.Instruction"
    pub fn get_struct(&self, name: &str) -> Result<&StructDefinition, SymbolError> {
        self.resolve_namespaced_symbol(name, |layer, n| layer.structs.get(n))
            .ok_or_else(|| SymbolError::UndefinedSymbol {
                name: name.to_string().into(),
                kind: "struct",
                span: None,
            })
    }

    /// Get a device definition by name (searches all layers: Local > HPM > Prelude > Core)
    /// Supports namespaced lookups: "Foundry.NMOS"
    ///
    /// Device definitions specify the physical contract for foundry primitives (transistors, diodes, etc.)
    /// including required terminals and expected materials for each terminal.
    pub fn get_device(&self, name: &str) -> Result<&DeviceDefinition, SymbolError> {
        self.resolve_namespaced_symbol(name, |layer, n| layer.devices.get(n))
            .ok_or_else(|| SymbolError::UndefinedSymbol {
                name: name.to_string().into(),
                kind: "device",
                span: None,
            })
    }

    /// Native v0.1.6 Unit Resolution
    ///
    /// Searches the authority stack for a unit that matches the given symbol or alias.
    /// This enables "Late Unit Binding" - the lexer creates Unit::Custom("um"), and this
    /// method resolves it to Unit::Micrometer by consulting the prelude (stdlib/primitives/units.hw).
    ///
    /// Search order: Local > HPM (last to first) > Prelude > Core
    ///
    /// # Examples
    /// ```
    /// // Prelude defines: unit Micrometer { symbol: "µm", aliases: ["um"], ... }
    /// // User writes: [x: 400um, y: 500um, z: 6]
    /// // Lexer produces: Unit::Custom("um")
    /// // This method resolves: Unit::Custom("um") → UnitDefinition for Micrometer
    /// ```
    pub fn resolve_unit_symbol(&self, symbol: &str) -> Option<&hwc_parser::UnitDefinition> {
        // 1. Check Local layer (user-defined units in current file)
        if let Some(unit) = self.local.units.values().find(|u| u.matches(symbol)) {
            return Some(unit);
        }

        // 2. Check HPM layers (imported unit libraries, reverse for shadowing)
        for layer in self.hpm.iter().rev() {
            if let Some(unit) = layer.units.values().find(|u| u.matches(symbol)) {
                return Some(unit);
            }
        }

        // 3. Check Prelude (Standard Library - stdlib/primitives/units.hw)
        if let Some(unit) = self.prelude.units.values().find(|u| u.matches(symbol)) {
            return Some(unit);
        }

        // 4. Check Core (reserved for future hardcoded bootstraps)
        if let Some(unit) = self.core.units.values().find(|u| u.matches(symbol)) {
            return Some(unit);
        }

        None
    }

    /// Get all constants from all layers (Local > HPM > Prelude > Core)
    ///
    /// Returns a map of constant names to their values for populating EvaluationContext.
    /// This enables "Late Math Binding" - expressions like `PI * 5mm` are evaluated
    /// using constants from the prelude (stdlib/primitives/math.hw) or user-defined constants.
    ///
    /// Search order: Core < Prelude < HPM < Local (so Local shadows everything)
    ///
    /// # Examples
    /// ```
    /// // Prelude defines: const PI: 3.14159265359
    /// // User writes: add LED at [x: PI * 5mm, y: 10mm, z: 1]
    /// // Evaluator gets PI from this method and computes: 3.14159 * 5mm = 15.7mm
    /// ```
    pub fn get_all_constants(&self) -> rustc_hash::FxHashMap<CompactString, f64> {
        let mut constants = rustc_hash::FxHashMap::default();

        // Start from lowest priority (Core) and work up so Local shadows correctly
        for (name, def) in &self.core.constants {
            constants.insert(name.clone(), def.value);
        }

        for (name, def) in &self.prelude.constants {
            constants.insert(name.clone(), def.value);
        }

        // HPM layers in order (last import wins)
        for layer in &self.hpm {
            for (name, def) in &layer.constants {
                constants.insert(name.clone(), def.value);
            }
        }

        // Local has highest priority
        for (name, def) in &self.local.constants {
            constants.insert(name.clone(), def.value);
        }

        constants
    }

    /// **CANONICAL UNIT CONVERSION METHOD**
    ///
    /// Convert a measurement to nanometers using the symbol table.
    /// This is the SINGLE SOURCE OF TRUTH for all unit conversions in the compiler.
    ///
    /// # Architecture
    /// - ALL unit conversions MUST go through this method
    /// - NO hardcoded conversion logic anywhere else
    /// - Supports built-in units (mm, cm, um) AND custom user-defined units
    /// - Uses symbol table to resolve units dynamically
    ///
    /// # Why This Exists
    /// Previously, unit conversions were scattered across multiple files with hardcoded
    /// logic (mm/cm/um only). This made it impossible to add custom units without
    /// modifying multiple places. Now, adding a new unit in stdlib/primitives/units.hw
    /// automatically makes it work everywhere.
    ///
    /// # Examples
    /// ```
    /// // Built-in units (fast path)
    /// let m = Measurement { value: 1.5, unit: Unit::Millimeter };
    /// assert_eq!(symbol_table.measurement_to_nm(&m), 1_500_000);
    ///
    /// // Custom units (resolved via symbol table)
    /// let m = Measurement { value: 0.1, unit: Unit::Custom("inch") };
    /// assert_eq!(symbol_table.measurement_to_nm(&m), 2_540_000); // 0.1 inch = 2.54mm
    /// ```
    pub fn measurement_to_nm(&self, measurement: &hwc_parser::Measurement) -> Result<i64, String> {
        use hwc_parser::Unit;

        let value_nm = match &measurement.unit {
            // Built-in units - fast path (no symbol table lookup needed)
            Unit::Millimeter => measurement.value * 1_000_000.0,
            Unit::Centimeter => measurement.value * 10_000_000.0,
            Unit::Micrometer => measurement.value * 1_000.0,

            // Custom units - resolve via symbol table
            // This includes: nm, m, inch, mil, and any user-defined units
            Unit::Custom(symbol) => {
                if let Some(unit_def) = self.resolve_unit_symbol(symbol) {
                    let multiplier = unit_def.multiplier.unwrap_or(1.0);
                    // multiplier is relative to meters, convert to nanometers
                    measurement.value * multiplier * 1_000_000_000.0
                } else {
                    return Err(format!("Unknown unit symbol: '{}'", symbol));
                }
            }

            // Non-length units
            _ => {
                return Err(format!(
                    "Cannot convert {:?} to nanometers (not a length unit)",
                    measurement.unit
                ));
            }
        };

        Ok(value_nm as i64)
    }

    // ========== SEMANTIC BAKING CACHE ==========

    /// Get a baked component definition (pre-parsed integers).
    ///
    /// PERFORMANCE: This returns a cached BakedComponent that was parsed once during
    /// registration. No string parsing happens here - just a HashMap lookup.
    ///
    /// Returns None if the component hasn't been baked yet (call bake_component first).
    pub fn get_baked_component(&self, name: &str) -> Option<&crate::BakedComponent> {
        self.baked_components.get(name)
    }

    /// Store a baked component definition in the cache.
    ///
    /// SEMANTIC BAKING: This is called during component registration to pre-parse
    /// the component's dimensions into pure integers. Placement loops then use
    /// get_baked_component() to avoid repeated parsing.
    ///
    /// Performance Impact:
    /// - Before: O(N × parsing_cost) where N = number of instances
    /// - After: O(1 × parsing_cost) + O(N × HashMap_lookup)
    pub fn cache_baked_component(&mut self, name: CompactString, baked: crate::BakedComponent) {
        self.baked_components.insert(name, baked);
    }
}
