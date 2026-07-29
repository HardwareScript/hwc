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
    /// # v0.2.0: Export Filtering
    /// - HPM layers contain ALL definitions (exported AND private) from imported files
    /// - When resolving from HPM layers, we ONLY return definitions that are exported
    /// - This allows private definitions to be accessible for scoped resolution within
    ///   their home module (e.g., a profile resolving its private material)
    /// - Local, Prelude, and Core layers don't have export restrictions (always accessible)
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
        is_exported_fn: impl Fn(&T) -> bool,
    ) -> Option<&'a T> {
       
        
        // Check for namespaced lookup first (e.g., "Metals.Copper")
        if let Some((layer_index, identifier)) = self.resolve_namespace(full_name) {
            
            // v0.2.0: For HPM layers, filter by export status
            return self.hpm.get(layer_index).and_then(|layer| {
                lookup_fn(layer, identifier).filter(|def| is_exported_fn(def))
            });
        }

        // REGULAR LOOKUP: Search local -> hpm (rev) -> prelude -> core
        // Local layer (highest priority) - no export filtering needed
        if let Some(def) = lookup_fn(&self.local, full_name) {
           
            return Some(def);
        }

        // HPM layers in reverse order (last import wins)
        // v0.2.0: ONLY return exported definitions from HPM layers
        for (_i, layer) in self.hpm.iter().rev().enumerate() {
            if let Some(def) = lookup_fn(layer, full_name) {
                let is_exported = is_exported_fn(def);
                
                if is_exported {
                    return Some(def);
                } else {
                    
                }
            }
        }

        // Prelude layer - no export filtering needed
        if let Some(def) = lookup_fn(&self.prelude, full_name) {
           
            return Some(def);
        }

        // Core layer (lowest priority) - no export filtering needed
        let result = lookup_fn(&self.core, full_name);
        if result.is_some() {
           
        } else {
            
        }
        result
    }

    /// Resolve a material within the same HPM layer as a given profile (v0.2.0)
    ///
    /// **Critical for Export Keyword Feature:**
    /// When a profile references materials in its stackup (e.g., `material: _InternalSilicon`),
    /// those materials should be resolvable even if they're private, AS LONG AS they're defined
    /// in the same file as the profile.
    ///
    /// This method finds which HPM layer contains the given profile, then searches ONLY that
    /// layer for the material WITHOUT export filtering. This allows exported profiles to use
    /// private materials from their home module.
    ///
    /// # Search order (within the profile's home layer):
    /// 1. Look for direct material definition
    /// 2. If not found, check material aliases
    /// 3. If still not found, fall back to regular cross-layer resolution
    ///
    /// # Example:
    /// ```hw
    /// // File: pdk_library.hw
    /// material _InternalSilicon: ...  // Private
    /// export profile PublicPDK:
    ///     stackup:
    ///         layer l1: material: _InternalSilicon  // Should resolve!
    /// ```
    pub fn resolve_material_in_profile_context(
        &self,
        profile_name: &str,
        material_name: &str,
    ) -> Result<&MaterialDefinition, SymbolError> {
        // First, find which layer contains this profile
        let profile_layer_index = self
            .hpm
            .iter()
            .enumerate()
            .rev() // Search in reverse (last import wins)
            .find(|(_idx, layer)| layer.profiles.contains_key(profile_name))
            .map(|(idx, _)| idx);

        // If profile is in an HPM layer, try to resolve material in that same layer first
        if let Some(layer_idx) = profile_layer_index {
            if let Some(layer) = self.hpm.get(layer_idx) {
                // Try direct material lookup (NO export filtering - same file context)
                if let Some(mat) = layer.materials.get(material_name) {
                    return Ok(mat);
                }

                // Try material alias lookup (NO export filtering)
                if let Some(alias) = layer.material_aliases.get(material_name) {
                    let target_name = alias.target.as_str();
                    // Recursively resolve the alias target within the same layer
                    if let Some(mat) = layer.materials.get(target_name) {
                        return Ok(mat);
                    }
                }
            }
        }

        // Profile not found in HPM layers, or material not in profile's home layer
        // Fall back to regular cross-layer resolution (with export filtering)
        self.get_material(material_name)
    }

    /// Get a material definition by name (searches all layers: Local > HPM > Prelude > Core)
    /// Supports namespaced lookups: "Metals.Copper" will look in the "Metals" namespace
    ///
    /// v0.1.6: Supports material aliases (e.g. "M1" -> "Copper").
    /// Alias resolution is recursive and detects circular dependencies.
    /// 
    /// v0.2.0: Only returns exported materials from HPM layers
    pub fn get_material(&self, name: &str) -> Result<&MaterialDefinition, SymbolError> {
        let mut current_name = name;
        let mut visited = rustc_hash::FxHashSet::default();
        visited.insert(current_name.to_string());

        // Recursive alias resolution
        loop {
            // Try to find as a material first
            if let Some(mat) = self.resolve_namespaced_symbol(
                current_name,
                |layer, n| layer.materials.get(n),
                |mat| mat.is_exported,
            ) {
                return Ok(mat);
            }

            // If not a material, check if it's an alias
            if let Some(alias) = self.resolve_namespaced_symbol(
                current_name,
                |layer, n| layer.material_aliases.get(n),
                |alias| alias.is_exported,
            ) {
                let next_name = alias.target.as_str();

                // Detect circular aliases
                if visited.contains(next_name) {
                    return Err(SymbolError::circular(
                        current_name.to_string().into(),
                        next_name.to_string().into(),
                        (alias.span.start, alias.span.end),
                    ));
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
            return Err(SymbolError::undefined(
                name.to_string().into(),
                "material",
                None,
            ));
        }
    }

    /// Get a profile definition by name (searches all layers: Local > HPM > Prelude > Core)
    /// Supports namespaced lookups: "Foundry.TSMC_180nm"
    /// v0.2.0: Only returns exported profiles from HPM layers
    pub fn get_profile(&self, name: &str) -> Result<&ProfileDefinition, SymbolError> {
        self.resolve_namespaced_symbol(
            name,
            |layer, n| layer.profiles.get(n),
            |prof| prof.is_exported,
        )
        .ok_or_else(|| SymbolError::undefined(name.to_string().into(), "profile", None))
    }

    /// Get a component definition by name (searches all layers: Local > HPM > Prelude > Core)
    /// Supports namespaced lookups: "Parts.MCU"
    /// v0.2.0: Only returns exported components from HPM layers
    pub fn get_component(&self, name: &str) -> Result<&ComponentDefinition, SymbolError> {
        self.resolve_namespaced_symbol(
            name,
            |layer, n| layer.components.get(n),
            |comp| comp.is_exported,
        )
        .ok_or_else(|| SymbolError::undefined(name.to_string().into(), "component", None))
    }

    /// Get a module definition by name (searches all layers: Local > HPM > Prelude > Core)
    /// Supports namespaced lookups: "Logic.Adder64"
    /// v0.2.0: Only returns exported modules from HPM layers
    pub fn get_module(&self, name: &str) -> Result<&ModuleDefinition, SymbolError> {
        self.resolve_namespaced_symbol(
            name,
            |layer, n| layer.modules.get(n),
            |mod_def| mod_def.is_exported,
        )
        .ok_or_else(|| SymbolError::undefined(name.to_string().into(), "module", None))
    }

    /// Get a mechanical definition by name (searches all layers: Local > HPM > Prelude > Core)
    /// Supports namespaced lookups: "Enclosures.StandardCase"
    /// v0.2.0: Only returns exported mechanicals from HPM layers
    pub fn get_mechanical(&self, name: &str) -> Result<&MechanicalDefinition, SymbolError> {
        self.resolve_namespaced_symbol(
            name,
            |layer, n| layer.mechanicals.get(n),
            |mech| mech.is_exported,
        )
        .ok_or_else(|| SymbolError::undefined(name.to_string().into(), "mechanical", None))
    }

    /// Get an interface definition by name (searches all layers: Local > HPM > Prelude > Core)
    /// Supports namespaced lookups: "Protocols.SPI"
    /// v0.2.0: Only returns exported interfaces from HPM layers
    pub fn get_interface(&self, name: &str) -> Result<&InterfaceDefinition, SymbolError> {
        self.resolve_namespaced_symbol(
            name,
            |layer, n| layer.interfaces.get(n),
            |iface| iface.is_exported,
        )
        .ok_or_else(|| SymbolError::undefined(name.to_string().into(), "interface", None))
    }

    /// Get a test definition by name (searches all layers: Local > HPM > Prelude > Core)
    /// Supports namespaced lookups: "TestSuites.UnitTests"
    /// v0.2.0: Only returns exported tests from HPM layers
    pub fn get_test(&self, name: &str) -> Result<&TestDefinition, SymbolError> {
        self.resolve_namespaced_symbol(
            name,
            |layer, n| layer.tests.get(n),
            |test| test.is_exported,
        )
        .ok_or_else(|| SymbolError::undefined(name.to_string().into(), "test", None))
    }

    /// Get a signal group definition by name (searches all layers: Local > HPM > Prelude > Core)
    /// Supports namespaced lookups: "Buses.DataBus"
    /// v0.2.0: Only returns exported signal groups from HPM layers
    pub fn get_signal_group(&self, name: &str) -> Result<&SignalGroupDefinition, SymbolError> {
        self.resolve_namespaced_symbol(
            name,
            |layer, n| layer.signal_groups.get(n),
            |sg| sg.is_exported,
        )
        .ok_or_else(|| SymbolError::undefined(name.to_string().into(), "signal_group", None))
    }

    /// Get a pattern definition by name (searches all layers: Local > HPM > Prelude > Core)
    /// Supports namespaced lookups: "Layouts.GridPattern"
    /// v0.2.0: Only returns exported patterns from HPM layers
    pub fn get_pattern(&self, name: &str) -> Result<&PatternDefinition, SymbolError> {
        self.resolve_namespaced_symbol(
            name,
            |layer, n| layer.patterns.get(n),
            |pat| pat.is_exported,
        )
        .ok_or_else(|| SymbolError::undefined(name.to_string().into(), "pattern", None))
    }

    /// Get a strategy definition by name (searches all layers: Local > HPM > Prelude > Core)
    /// Supports namespaced lookups: "Routing.ManhattanStrategy"
    /// v0.2.0: Only returns exported strategies from HPM layers
    pub fn get_strategy(&self, name: &str) -> Result<&StrategyDefinition, SymbolError> {
        self.resolve_namespaced_symbol(
            name,
            |layer, n| layer.strategies.get(n),
            |strat| strat.is_exported,
        )
        .ok_or_else(|| SymbolError::undefined(name.to_string().into(), "strategy", None))
    }

    /// Get a logic block definition by name (searches all layers: Local > HPM > Prelude > Core)
    /// Supports namespaced lookups: "CPU.ALU"
    /// v0.2.0: Only returns exported logic blocks from HPM layers
    pub fn get_logic(&self, name: &str) -> Result<&LogicDefinition, SymbolError> {
        self.resolve_namespaced_symbol(
            name,
            |layer, n| layer.logic_blocks.get(n),
            |logic| logic.is_exported,
        )
        .ok_or_else(|| SymbolError::undefined(name.to_string().into(), "logic", None))
    }

    /// Get an enum definition by name (searches all layers: Local > HPM > Prelude > Core)
    /// Supports namespaced lookups: "Types.State"
    /// v0.2.0: Only returns exported enums from HPM layers
    pub fn get_enum(&self, name: &str) -> Result<&EnumDefinition, SymbolError> {
        self.resolve_namespaced_symbol(
            name,
            |layer, n| layer.enums.get(n),
            |enum_def| enum_def.is_exported,
        )
        .ok_or_else(|| SymbolError::undefined(name.to_string().into(), "enum", None))
    }

    /// Get a struct definition by name (searches all layers: Local > HPM > Prelude > Core)
    /// Supports namespaced lookups: "CPU.Instruction"
    /// v0.2.0: Only returns exported structs from HPM layers
    pub fn get_struct(&self, name: &str) -> Result<&StructDefinition, SymbolError> {
        self.resolve_namespaced_symbol(
            name,
            |layer, n| layer.structs.get(n),
            |struct_def| struct_def.is_exported,
        )
        .ok_or_else(|| SymbolError::undefined(name.to_string().into(), "struct", None))
    }

    /// Get a device definition by name (searches all layers: Local > HPM > Prelude > Core)
    /// Supports namespaced lookups: "Foundry.NMOS"
    ///
    /// Device definitions specify the physical contract for foundry primitives (transistors, diodes, etc.)
    /// including required terminals and expected materials for each terminal.
    /// v0.2.0: Only returns exported devices from HPM layers
    pub fn get_device(&self, name: &str) -> Result<&DeviceDefinition, SymbolError> {
        self.resolve_namespaced_symbol(
            name,
            |layer, n| layer.devices.get(n),
            |dev| dev.is_exported,
        )
        .ok_or_else(|| SymbolError::undefined(name.to_string().into(), "device", None))
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
    pub fn measurement_to_nm(&self, measurement: &hwc_parser::Measurement) -> Result<i64, String> {
        use hwc_parser::Unit;

        let value_nm = match &measurement.unit {
            // Built-in units - fast path (no symbol table lookup needed)
            Unit::Millimeter => measurement.value * 1_000_000.0,
            Unit::Centimeter => measurement.value * 10_000_000.0,
            Unit::Micrometer => measurement.value * 1_000.0,
            Unit::Nanometer => measurement.value,

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

    /// Bake a component definition into a BakedComponent (Pass 2)
    ///
    /// Resolves dimensions and pins into pure nanometer integers.
    pub fn bake_component(&self, name: &str) -> Result<crate::BakedComponent, String> {
        let def = self.get_component(name).map_err(|e| e.to_string())?;

        let layout = def.layout.as_ref().ok_or_else(|| {
            format!(
                "Component '{}' has no layout block and cannot be baked",
                name
            )
        })?;

        // 1. Resolve dimensions strictly from shape string (e.g., "Rectangle(6mm, 6mm, 1mm)")
        let (width_nm, height_nm) = if let Some(shape_str) = &layout.shape {
            let (w, h, _) =
                crate::ir::placement::helpers::parse_rectangle_dimensions(shape_str, self)
                    .ok_or_else(|| {
                        format!(
                            "Invalid shape format for component '{}': '{}'",
                            name, shape_str
                        )
                    })?;
            (w, h)
        } else {
            // If no explicit shape, derive bounds from pins exactly
            let mut min_x = i64::MAX;
            let mut max_x = i64::MIN;
            let mut min_y = i64::MAX;
            let mut max_y = i64::MIN;

            for pin_pos in layout.pin_positions.values() {
                let x_nm = (pin_pos.x * 1_000_000.0) as i64;
                let y_nm = (pin_pos.y * 1_000_000.0) as i64;
                min_x = min_x.min(x_nm);
                max_x = max_x.max(x_nm);
                min_y = min_y.min(y_nm);
                max_y = max_y.max(y_nm);
            }

            if min_x == i64::MAX {
                return Err(format!(
                    "Component '{}' has no shape and no pins to derive dimensions from",
                    name
                ));
            }

            (max_x - min_x, max_y - min_y)
        };

        let mut pins = Vec::new();
        for pin_name in &def.pins {
            // Find pin position in layout
            let pin_pos = layout
                .pin_positions
                .get(pin_name)
                .ok_or_else(|| format!("Pin '{}' not found in layout of '{}'", pin_name, name))?;

            let x_nm = (pin_pos.x * 1_000_000.0) as i64;
            let y_nm = (pin_pos.y * 1_000_000.0) as i64;
            let z_nm = (pin_pos.z.unwrap_or(0.0) * 1_000_000.0) as i64;

            // Resolve pad shape strictly from layout.pad_shapes (e.g., "Circle(0.5mm)")
            let pad_shape_str = layout.pad_shapes.get(pin_name).ok_or_else(|| {
                format!(
                    "Pin '{}' in component '{}' is missing a pad shape definition",
                    pin_name, name
                )
            })?;

            let (w, h, _) =
                crate::ir::placement::helpers::parse_rectangle_dimensions(pad_shape_str, self)
                    .ok_or_else(|| {
                        format!(
                            "Invalid pad shape format for pin '{}' in component '{}': '{}'",
                            pin_name, name, pad_shape_str
                        )
                    })?;

            let pad_shape = if pad_shape_str.starts_with("Circle") {
                hwc_engine::placement::PadShape::Circle { diameter_nm: w }
            } else if pad_shape_str.starts_with("Obround") {
                hwc_engine::placement::PadShape::Obround {
                    width_nm: w,
                    height_nm: h,
                }
            } else if pad_shape_str.starts_with("RoundedRect") {
                // For RoundedRect, we'd need to parse the corner radius too if supported by helper
                hwc_engine::placement::PadShape::Rectangle {
                    width_nm: w,
                    height_nm: h,
                }
            } else {
                hwc_engine::placement::PadShape::Rectangle {
                    width_nm: w,
                    height_nm: h,
                }
            };

            pins.push(hwc_engine::placement::BakedPin {
                name: pin_name.clone(),
                local_offset: hwc_engine::geometry::Point3D::new(x_nm, y_nm, z_nm),
                pad_shape,
            });
        }

        Ok(crate::BakedComponent {
            name: name.into(),
            width_nm,
            height_nm,
            pins,
        })
    }

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
