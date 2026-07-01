//! Symbol table authority-stack layers (Local / HPM / Prelude / Core).
//!
//! **Not** PCB Z-axis layers — those are resolved by
//! [`crate::ir::stackup_manager::StackupManager`] from `Elevation` + profile `stackup`.

use compact_str::CompactString;
use hwc_parser::{
    logic::{EnumDefinition, LogicDefinition, StructDefinition},
    ComponentDefinition, ConstDefinition, DeviceDefinition, InterfaceDefinition,
    MaterialAliasDefinition, MaterialDefinition, MechanicalDefinition, ModuleDefinition,
    PatternDefinition, ProfileDefinition, ShapeDefinition, SignalGroupDefinition,
    StrategyDefinition, TestDefinition, UnitDefinition,
};
use rustc_hash::FxHashMap;

/// A single layer in the authority stack
#[derive(Debug, Clone)]
pub struct SymbolLayer {
    pub(super) materials: FxHashMap<CompactString, MaterialDefinition>,
    pub(super) material_aliases: FxHashMap<CompactString, MaterialAliasDefinition>,
    pub(super) profiles: FxHashMap<CompactString, ProfileDefinition>,
    pub(super) components: FxHashMap<CompactString, ComponentDefinition>,
    pub(super) modules: FxHashMap<CompactString, ModuleDefinition>,
    pub(super) mechanicals: FxHashMap<CompactString, MechanicalDefinition>,
    pub(super) interfaces: FxHashMap<CompactString, InterfaceDefinition>,
    pub(super) tests: FxHashMap<CompactString, TestDefinition>,
    pub(super) signal_groups: FxHashMap<CompactString, SignalGroupDefinition>,
    pub(super) patterns: FxHashMap<CompactString, PatternDefinition>,
    pub(super) strategies: FxHashMap<CompactString, StrategyDefinition>,
    pub(super) logic_blocks: FxHashMap<CompactString, LogicDefinition>,
    pub(super) enums: FxHashMap<CompactString, EnumDefinition>,
    pub(super) structs: FxHashMap<CompactString, StructDefinition>,
    pub(super) units: FxHashMap<CompactString, UnitDefinition>,
    pub(super) devices: FxHashMap<CompactString, DeviceDefinition>,
    pub(super) constants: FxHashMap<CompactString, ConstDefinition>,
    pub(super) shapes: FxHashMap<CompactString, ShapeDefinition>,
}

impl SymbolLayer {
    pub fn new() -> Self {
        Self {
            materials: FxHashMap::default(),
            material_aliases: FxHashMap::default(),
            profiles: FxHashMap::default(),
            components: FxHashMap::default(),
            modules: FxHashMap::default(),
            mechanicals: FxHashMap::default(),
            interfaces: FxHashMap::default(),
            tests: FxHashMap::default(),
            signal_groups: FxHashMap::default(),
            patterns: FxHashMap::default(),
            strategies: FxHashMap::default(),
            logic_blocks: FxHashMap::default(),
            enums: FxHashMap::default(),
            structs: FxHashMap::default(),
            units: FxHashMap::default(),
            devices: FxHashMap::default(),
            constants: FxHashMap::default(),
            shapes: FxHashMap::default(),
        }
    }
}

impl Default for SymbolLayer {
    fn default() -> Self {
        Self::new()
    }
}

/// Symbol Table for two-pass compilation
///
/// v0.1.6 Update: Implements layered authority stack (Local > HPM > Prelude > Core)
/// for proper symbol resolution and property-level shadowing.
///
/// v0.1.6 Performance: Semantic Baking Cache
/// Components are "baked" (parsed once) during registration and cached as pure integers.
/// This eliminates repeated lexer invocations during placement loops.
#[derive(Debug, Clone)]
pub struct SymbolTable {
    // Core Layer: Hardcoded engine bootstraps (currently unused, reserved for future)
    pub(super) core: SymbolLayer,

    // Prelude Layer: Auto-loaded primitives from stdlib/primitives/
    pub(super) prelude: SymbolLayer,

    // HPM Layer: Imported definitions from external libraries (last import wins)
    pub(super) hpm: Vec<SymbolLayer>,

    // Local Layer: Definitions in the current .hw file (highest priority)
    pub(super) local: SymbolLayer,

    // Namespace Aliases: Maps alias names to HPM layer indices
    // Example: "Metals" -> (layer_index, original_path)
    // This enables: import @std/materials/conductors as Metals
    pub(super) namespaces: FxHashMap<CompactString, usize>,

    // SEMANTIC BAKING CACHE: Pre-parsed component definitions
    // Key: component name, Value: BakedComponent (pure integers, no strings)
    // This cache is populated during registration and used during placement
    // Performance: Eliminates O(N × parsing_cost) overhead in placement loops
    pub(super) baked_components: FxHashMap<CompactString, crate::BakedComponent>,
}

impl SymbolTable {
    /// Create a new empty symbol table
    pub fn new() -> Self {
        Self {
            core: SymbolLayer::new(),
            prelude: SymbolLayer::new(),
            hpm: Vec::new(),
            local: SymbolLayer::new(),
            namespaces: FxHashMap::default(),
            baked_components: FxHashMap::default(),
        }
    }

    /// Add a new HPM layer (for imported libraries)
    pub fn push_hpm_layer(&mut self) {
        self.hpm.push(SymbolLayer::new());
    }

    /// Register a namespace alias for the most recent HPM layer
    /// Example: import @std/materials/conductors as Metals
    pub fn register_namespace_alias(&mut self, alias: CompactString) {
        if !self.hpm.is_empty() {
            let layer_index = self.hpm.len() - 1;
            self.namespaces.insert(alias, layer_index);
        }
    }

    /// Resolve a namespaced identifier (e.g., "Metals.Copper" -> "Copper" in namespace "Metals")
    /// Returns (namespace_layer_index, identifier) if namespaced, or None if not
    pub fn resolve_namespace<'a>(&self, full_name: &'a str) -> Option<(usize, &'a str)> {
        if let Some(dot_pos) = full_name.find('.') {
            let namespace = &full_name[..dot_pos];
            let identifier = &full_name[dot_pos + 1..];

            if let Some(&layer_index) = self.namespaces.get(namespace) {
                return Some((layer_index, identifier));
            }
        }
        None
    }

    /// Add a unit to the prelude layer (for standard library loading)
    pub fn add_prelude_unit(&mut self, name: CompactString, unit: UnitDefinition) {
        self.prelude.units.insert(name, unit);
    }

    /// Add a constant to the prelude layer (for standard library loading)
    pub fn add_prelude_constant(&mut self, name: CompactString, constant: ConstDefinition) {
        self.prelude.constants.insert(name, constant);
    }

    /// Check if a material exists in any layer
    /// Supports namespaced lookups: "Metals.Copper" will look in the "Metals" namespace
    /// Supports material aliases.
    pub fn has_material(&self, name: &str) -> bool {
        // Check for namespaced lookup first
        if let Some((layer_index, identifier)) = self.resolve_namespace(name) {
            return self
                .hpm
                .get(layer_index)
                .map(|layer| {
                    layer.materials.contains_key(identifier)
                        || layer.material_aliases.contains_key(identifier)
                })
                .unwrap_or(false);
        }

        // Regular lookup across all layers
        self.local.materials.contains_key(name)
            || self.local.material_aliases.contains_key(name)
            || self.hpm.iter().any(|layer| {
                layer.materials.contains_key(name) || layer.material_aliases.contains_key(name)
            })
            || self.prelude.materials.contains_key(name)
            || self.prelude.material_aliases.contains_key(name)
            || self.core.materials.contains_key(name)
            || self.core.material_aliases.contains_key(name)
    }

    /// Check if a profile exists in any layer
    /// Supports namespaced lookups: "Foundry.TSMC_180nm"
    pub fn has_profile(&self, name: &str) -> bool {
        // Check for namespaced lookup first
        if let Some((layer_index, identifier)) = self.resolve_namespace(name) {
            return self
                .hpm
                .get(layer_index)
                .map(|layer| layer.profiles.contains_key(identifier))
                .unwrap_or(false);
        }

        // Regular lookup across all layers
        self.local.profiles.contains_key(name)
            || self
                .hpm
                .iter()
                .any(|layer| layer.profiles.contains_key(name))
            || self.prelude.profiles.contains_key(name)
            || self.core.profiles.contains_key(name)
    }

    /// Debug method to list all profile names in all layers
    pub fn debug_list_profiles(&self) -> Vec<String> {
        let mut profiles = Vec::new();
        for key in self.local.profiles.keys() {
            profiles.push(format!("local:{}", key));
        }
        for (idx, layer) in self.hpm.iter().enumerate() {
            for key in layer.profiles.keys() {
                profiles.push(format!("hpm[{}]:{}", idx, key));
            }
        }
        for key in self.prelude.profiles.keys() {
            profiles.push(format!("prelude:{}", key));
        }
        for key in self.core.profiles.keys() {
            profiles.push(format!("core:{}", key));
        }
        profiles
    }

    /// Check if a component exists in any layer
    /// Supports namespaced lookups: "Parts.MCU"
    pub fn has_component(&self, name: &str) -> bool {
        // Check for namespaced lookup first
        if let Some((layer_index, identifier)) = self.resolve_namespace(name) {
            return self
                .hpm
                .get(layer_index)
                .map(|layer| layer.components.contains_key(identifier))
                .unwrap_or(false);
        }

        // Regular lookup across all layers
        self.local.components.contains_key(name)
            || self
                .hpm
                .iter()
                .any(|layer| layer.components.contains_key(name))
            || self.prelude.components.contains_key(name)
            || self.core.components.contains_key(name)
    }

    /// Check if a module exists in any layer
    /// Supports namespaced lookups: "Logic.Adder64"
    pub fn has_module(&self, name: &str) -> bool {
        // Check for namespaced lookup first
        if let Some((layer_index, identifier)) = self.resolve_namespace(name) {
            return self
                .hpm
                .get(layer_index)
                .map(|layer| layer.modules.contains_key(identifier))
                .unwrap_or(false);
        }

        // Regular lookup across all layers
        self.local.modules.contains_key(name)
            || self
                .hpm
                .iter()
                .any(|layer| layer.modules.contains_key(name))
            || self.prelude.modules.contains_key(name)
            || self.core.modules.contains_key(name)
    }

    /// Check if a mechanical exists in any layer
    pub fn has_mechanical(&self, name: &str) -> bool {
        self.local.mechanicals.contains_key(name)
            || self
                .hpm
                .iter()
                .any(|layer| layer.mechanicals.contains_key(name))
            || self.prelude.mechanicals.contains_key(name)
            || self.core.mechanicals.contains_key(name)
    }

    /// Check if an interface exists in any layer
    pub fn has_interface(&self, name: &str) -> bool {
        self.local.interfaces.contains_key(name)
            || self
                .hpm
                .iter()
                .any(|layer| layer.interfaces.contains_key(name))
            || self.prelude.interfaces.contains_key(name)
            || self.core.interfaces.contains_key(name)
    }

    /// Check if a test exists in any layer
    pub fn has_test(&self, name: &str) -> bool {
        self.local.tests.contains_key(name)
            || self.hpm.iter().any(|layer| layer.tests.contains_key(name))
            || self.prelude.tests.contains_key(name)
            || self.core.tests.contains_key(name)
    }

    /// Check if a signal group exists in any layer
    pub fn has_signal_group(&self, name: &str) -> bool {
        self.local.signal_groups.contains_key(name)
            || self
                .hpm
                .iter()
                .any(|layer| layer.signal_groups.contains_key(name))
            || self.prelude.signal_groups.contains_key(name)
            || self.core.signal_groups.contains_key(name)
    }

    /// Check if a pattern exists in any layer
    pub fn has_pattern(&self, name: &str) -> bool {
        self.local.patterns.contains_key(name)
            || self
                .hpm
                .iter()
                .any(|layer| layer.patterns.contains_key(name))
            || self.prelude.patterns.contains_key(name)
            || self.core.patterns.contains_key(name)
    }

    /// Check if a strategy exists in any layer
    pub fn has_strategy(&self, name: &str) -> bool {
        self.local.strategies.contains_key(name)
            || self
                .hpm
                .iter()
                .any(|layer| layer.strategies.contains_key(name))
            || self.prelude.strategies.contains_key(name)
            || self.core.strategies.contains_key(name)
    }

    /// Check if a logic block exists in any layer
    pub fn has_logic(&self, name: &str) -> bool {
        self.local.logic_blocks.contains_key(name)
            || self
                .hpm
                .iter()
                .any(|layer| layer.logic_blocks.contains_key(name))
            || self.prelude.logic_blocks.contains_key(name)
            || self.core.logic_blocks.contains_key(name)
    }

    /// Check if an enum exists in any layer
    pub fn has_enum(&self, name: &str) -> bool {
        self.local.enums.contains_key(name)
            || self.hpm.iter().any(|layer| layer.enums.contains_key(name))
            || self.prelude.enums.contains_key(name)
            || self.core.enums.contains_key(name)
    }

    /// Check if a struct exists in any layer
    pub fn has_struct(&self, name: &str) -> bool {
        self.local.structs.contains_key(name)
            || self
                .hpm
                .iter()
                .any(|layer| layer.structs.contains_key(name))
            || self.prelude.structs.contains_key(name)
            || self.core.structs.contains_key(name)
    }

    /// Check if a shape exists in any layer
    pub fn has_shape(&self, name: &str) -> bool {
        self.local.shapes.contains_key(name)
            || self.hpm.iter().any(|layer| layer.shapes.contains_key(name))
            || self.prelude.shapes.contains_key(name)
            || self.core.shapes.contains_key(name)
    }

    /// Look up a shape definition by name across all layers
    /// Returns the first match found in priority order: local > hpm > prelude > core
    pub fn get_shape(&self, name: &str) -> Option<&ShapeDefinition> {
        self.local
            .shapes
            .get(name)
            .or_else(|| self.hpm.iter().find_map(|layer| layer.shapes.get(name)))
            .or_else(|| self.prelude.shapes.get(name))
            .or_else(|| self.core.shapes.get(name))
    }

    /// Count total definitions across all layers
    pub fn definition_count(&self) -> usize {
        let count_layer = |layer: &SymbolLayer| {
            layer.materials.len()
                + layer.profiles.len()
                + layer.components.len()
                + layer.modules.len()
                + layer.mechanicals.len()
                + layer.interfaces.len()
                + layer.tests.len()
                + layer.signal_groups.len()
                + layer.patterns.len()
                + layer.strategies.len()
                + layer.logic_blocks.len()
                + layer.enums.len()
                + layer.structs.len()
                + layer.units.len()
                + layer.constants.len()
                + layer.shapes.len()
        };

        count_layer(&self.local)
            + self.hpm.iter().map(count_layer).sum::<usize>()
            + count_layer(&self.prelude)
            + count_layer(&self.core)
    }

    /// Get all materials (for database population) - collects from all layers
    pub fn materials(&self) -> FxHashMap<CompactString, MaterialDefinition> {
        let mut all_materials = FxHashMap::default();

        // Start with core (lowest priority)
        all_materials.extend(self.core.materials.clone());

        // Add prelude (overrides core)
        all_materials.extend(self.prelude.materials.clone());

        // Add HPM layers (in order, so last import wins)
        for layer in &self.hpm {
            all_materials.extend(layer.materials.clone());
        }

        // Add local (highest priority, overrides everything)
        all_materials.extend(self.local.materials.clone());

        all_materials
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}
