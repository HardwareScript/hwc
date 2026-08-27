//! Unified Symbol Table - Zero Boilerplate Architecture
//!
//! v0.2.1 Arena Refactor: Stores 4-byte Definition IDs, arena holds actual data.
//! This eliminates struct field explosion and achieves blazing fast speeds.

use compact_str::CompactString;
use hwc_parser::ast::AstArena;
use rustc_hash::FxHashMap;

use super::definition::{Definition, DefinitionExt};

/// A single layer in the authority stack (Local / HPM / Prelude / Core)
///
/// v0.2.1: Stores 4-byte Definition IDs (Copy types!) instead of full struct clones
#[derive(Debug, Clone, Default)]
pub struct SymbolLayer {
    /// Universal symbol index: Name → Definition ID
    /// Definition is now a Copy 4-byte ID pointing into the shared arena
    pub(super) symbols: FxHashMap<CompactString, Definition>,
}

impl SymbolLayer {
    pub fn new() -> Self {
        Self {
            symbols: FxHashMap::default(),
        }
    }

    /// Insert any definition dynamically (zero boilerplate!)
    pub fn insert(&mut self, name: CompactString, def: Definition) {
        self.symbols.insert(name, def);
    }

    /// Get a definition by name
    pub fn get(&self, name: &str) -> Option<&Definition> {
        self.symbols.get(name)
    }

    /// Check if a symbol exists
    pub fn contains(&self, name: &str) -> bool {
        self.symbols.contains_key(name)
    }

    /// Get total number of definitions in this layer
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// Iterate over all definitions
    pub fn iter(&self) -> impl Iterator<Item = (&CompactString, &Definition)> {
        self.symbols.iter()
    }
}

/// Symbol Table for two-pass compilation
///
/// v0.1.6 Update: Implements layered authority stack (Local > HPM > Prelude > Core)
/// for proper symbol resolution and property-level shadowing.
///
/// v0.2.1 Arena Refactor: 100% arena-based - stores 4-byte Definition IDs, arena holds actual data
///
/// v0.1.6 Performance: Semantic Baking Cache
/// Components are "baked" (parsed once) during registration and cached as pure integers.
/// This eliminates repeated lexer invocations during placement loops.
#[derive(Debug, Clone)]
pub struct SymbolTable {
    // Shared arena containing all definition data
    pub(super) arena: AstArena,

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
}

impl SymbolTable {
    /// Create a new empty symbol table with a given arena
    pub fn new(arena: AstArena) -> Self {
        Self {
            arena,
            core: SymbolLayer::new(),
            prelude: SymbolLayer::new(),
            hpm: Vec::new(),
            local: SymbolLayer::new(),
            namespaces: FxHashMap::default(),
        }
    }

    /// Get immutable reference to the arena
    pub fn arena(&self) -> &AstArena {
        &self.arena
    }

    /// Get mutable reference to the arena
    pub fn arena_mut(&mut self) -> &mut AstArena {
        &mut self.arena
    }

    /// Merge another AstArena into this symbol table's arena and return the offsets
    pub fn merge_arena(&mut self, arena: AstArena) -> hwc_parser::ast::arena::AstArenaOffsets {
        self.arena.merge(arena)
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

    /// UNIVERSAL SYMBOL LOOKUP ENGINE
    /// Replaces 20+ separate has_xxx/get_xxx methods with ONE lookup function
    ///
    /// Searches: Local > HPM (reverse order) > Prelude > Core
    /// Supports namespaced lookups ("Metals.Copper") and export filtering
    ///
    /// v0.2.1: Returns Definition by value (it's just a Copy 4-byte ID!)
    pub fn get_symbol(&self, name: &str) -> Option<Definition> {
        // 1. Namespaced lookup (e.g. "Metals.Copper")
        if let Some((layer_index, identifier)) = self.resolve_namespace(name) {
            return self
                .hpm
                .get(layer_index)
                .and_then(|layer| layer.get(identifier))
                .copied()
                .filter(|def| def.is_exported(&self.arena));
        }

        // 2. Regular lookup: Local (highest priority)
        if let Some(&def) = self.local.get(name) {
            return Some(def);
        }

        // 3. HPM Layers (reverse order, last import wins; filter exported only)
        for layer in self.hpm.iter().rev() {
            if let Some(&def) = layer.get(name) {
                if def.is_exported(&self.arena) {
                    return Some(def);
                }
            }
        }

        // 4. Prelude layer
        if let Some(&def) = self.prelude.get(name) {
            return Some(def);
        }

        // 5. Core layer (lowest priority)
        self.core.get(name).copied()
    }

    /// Generic "has" check replacing 20 separate `has_xxx` methods!
    pub fn has_symbol(&self, name: &str) -> bool {
        self.get_symbol(name).is_some()
    }

    /// Check if a symbol exists AND matches a specific kind
    pub fn has_symbol_of_kind(&self, name: &str, expected_kind: &str) -> bool {
        self.get_symbol(name)
            .map(|def| def.kind_str() == expected_kind)
            .unwrap_or(false)
    }

    /// Count total definitions across all layers (ONE line instead of 20!)
    pub fn definition_count(&self) -> usize {
        self.local.len()
            + self.hpm.iter().map(|layer| layer.len()).sum::<usize>()
            + self.prelude.len()
            + self.core.len()
    }

    /// Iterate over all definitions in priority order
    pub fn iter_all_symbols(&self) -> impl Iterator<Item = (&CompactString, &Definition)> {
        let local = self.local.iter();
        let hpm = self.hpm.iter().rev().flat_map(|layer| layer.iter());
        let prelude = self.prelude.iter();
        let core = self.core.iter();

        local.chain(hpm).chain(prelude).chain(core)
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new(AstArena::new())
    }
}
