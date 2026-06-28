use super::super::{error::SymbolError, layer::SymbolTable};
use compact_str::CompactString;
use hwc_diagnostics::DiagnosticCollector;
use hwc_parser::{
    logic::{EnumDefinition, LogicDefinition, StructDefinition},
    ComponentDefinition, InterfaceDefinition, MaterialAliasDefinition, MaterialDefinition,
    MechanicalDefinition, ModuleDefinition, PatternDefinition, ProfileDefinition, ShapeDefinition,
    SignalGroupDefinition, StrategyDefinition, TestDefinition, UnitDefinition,
};

impl SymbolTable {
    /// Register a material alias (in local layer)
    pub fn register_material_alias(
        &mut self,
        collector: &DiagnosticCollector,
        def: MaterialAliasDefinition,
    ) {
        let name_str = def.name.as_str().to_string();

        // Check for duplicate in local layer
        if let Some(existing) = self.local.material_aliases.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "material_alias",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }

        self.local.material_aliases.insert(name_str.into(), def);
    }

    /// Register a material definition (in local layer)
    ///
    /// v0.1.6 Property-Level Shadowing:
    /// If a material with the same name exists in a lower layer (HPM/Prelude/Core),
    /// this method will merge the properties instead of replacing completely.
    ///
    /// Rule 1 (GAP3): Local Beats Global
    /// If a local definition shadows an import, emit a warning to the user.
    pub fn register_material(&mut self, collector: &DiagnosticCollector, def: MaterialDefinition) {
        let name_str = def.name.as_str().to_string();

        // Check for duplicate in local layer (same layer = error)
        if let Some(existing) = self.local.materials.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "material",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }

        // Rule 1: Check if this local definition shadows an import (GAP3)
        if let Some(import_source) = self.check_material_shadowing(&name_str) {
            collector.report(SymbolError::shadowing(
                def.name.to_string().into(),
                "material",
                (def.span.start, def.span.end),
                import_source,
            ));
        }

        // Check if material exists in lower layers (HPM/Prelude/Core)
        // If yes, merge properties (property-level shadowing)
        let material_to_register =
            if let Some(base_material) = self.find_material_in_lower_layers(&name_str) {
                // Property-level shadowing: merge base with override
                self.merge_properties(base_material, &def)
            } else {
                // No base material found - use definition as-is
                def
            };

        self.local
            .materials
            .insert(name_str.into(), material_to_register);
    }

    /// Find a material in lower layers (HPM > Prelude > Core), excluding local layer
    fn find_material_in_lower_layers(&self, name: &str) -> Option<&MaterialDefinition> {
        // Search HPM layers in reverse order (last import wins)
        for layer in self.hpm.iter().rev() {
            if let Some(def) = layer.materials.get(name) {
                return Some(def);
            }
        }

        // Search prelude
        if let Some(def) = self.prelude.materials.get(name) {
            return Some(def);
        }

        // Search core
        if let Some(def) = self.core.materials.get(name) {
            return Some(def);
        }

        None
    }

    /// Check if a material exists in lower layers and return the source layer name
    /// Returns: Some("imported library"), Some("@std/materials"), or None
    /// This is used for Rule 1 shadowing warnings (GAP3)
    fn check_material_shadowing(&self, name: &str) -> Option<CompactString> {
        // Check HPM layers (imported definitions)
        for layer in self.hpm.iter().rev() {
            if layer.materials.contains_key(name) {
                return Some("imported library".into());
            }
        }

        // Check prelude (stdlib)
        if self.prelude.materials.contains_key(name) {
            return Some("@std/materials".into());
        }

        // Check core
        if self.core.materials.contains_key(name) {
            return Some("core library".into());
        }

        None
    }

    /// Check if a profile exists in lower layers and return the source layer name
    fn check_profile_shadowing(&self, name: &str) -> Option<CompactString> {
        for layer in self.hpm.iter().rev() {
            if layer.profiles.contains_key(name) {
                return Some("imported library".into());
            }
        }
        if self.prelude.profiles.contains_key(name) {
            return Some("@std/profiles".into());
        }
        if self.core.profiles.contains_key(name) {
            return Some("core library".into());
        }
        None
    }

    /// Check if a component exists in lower layers and return the source layer name
    fn check_component_shadowing(&self, name: &str) -> Option<CompactString> {
        for layer in self.hpm.iter().rev() {
            if layer.components.contains_key(name) {
                return Some("imported library".into());
            }
        }
        if self.prelude.components.contains_key(name) {
            return Some("@std/components".into());
        }
        if self.core.components.contains_key(name) {
            return Some("core library".into());
        }
        None
    }

    /// Check if a module exists in lower layers and return the source layer name
    fn check_module_shadowing(&self, name: &str) -> Option<CompactString> {
        for layer in self.hpm.iter().rev() {
            if layer.modules.contains_key(name) {
                return Some("imported library".into());
            }
        }
        if self.prelude.modules.contains_key(name) {
            return Some("@std/modules".into());
        }
        if self.core.modules.contains_key(name) {
            return Some("core library".into());
        }
        None
    }

    /// Register a profile definition (in local layer)
    ///
    /// Rule 1 (GAP3): Local Beats Global - warns if shadowing an import
    pub fn register_profile(&mut self, collector: &DiagnosticCollector, def: ProfileDefinition) {
        let name_str = def.name.as_str();
        if let Some(existing) = self.local.profiles.get(name_str) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "profile",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }

        // Rule 1: Check if this local definition shadows an import (GAP3)
        if let Some(import_source) = self.check_profile_shadowing(name_str) {
            collector.report(SymbolError::shadowing(
                def.name.to_string().into(),
                "profile",
                (def.span.start, def.span.end),
                import_source,
            ));
        }

        self.local.profiles.insert(name_str.into(), def);
    }

    /// Register a component definition (in local layer)
    ///
    /// Rule 1 (GAP3): Local Beats Global - warns if shadowing an import
    ///
    /// v0.1.6 SEMANTIC BAKING: After registering the component AST, immediately
    /// bake it into pre-parsed integers and cache it. This eliminates repeated
    /// parsing during placement loops.
    pub fn register_component(
        &mut self,
        collector: &DiagnosticCollector,
        def: ComponentDefinition,
    ) {
        let name_str = def.name.as_str().to_string();
        if let Some(existing) = self.local.components.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "component",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }

        // Rule 1: Check if this local definition shadows an import (GAP3)
        if let Some(import_source) = self.check_component_shadowing(&name_str) {
            collector.report(SymbolError::shadowing(
                def.name.to_string().into(),
                "component",
                (def.span.start, def.span.end),
                import_source,
            ));
        }

        // Register the AST definition
        self.local.components.insert(name_str.clone().into(), def);

        // SEMANTIC BAKING: Bake local components too
        match self.bake_component(&name_str) {
            Ok(baked) => {
                self.cache_baked_component(name_str.into(), baked);
            }
            Err(e) => {
                eprintln!(
                    "[WARN] Failed to bake local component '{}': {:?}",
                    name_str, e
                );
            }
        }
    }

    /// Register a module definition (in local layer)
    ///
    /// Rule 1 (GAP3): Local Beats Global - warns if shadowing an import
    pub fn register_module(&mut self, collector: &DiagnosticCollector, def: ModuleDefinition) {
        let name_str = def.name.as_str();
        if let Some(existing) = self.local.modules.get(name_str) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "module",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }

        // Rule 1: Check if this local definition shadows an import (GAP3)
        if let Some(import_source) = self.check_module_shadowing(name_str) {
            collector.report(SymbolError::shadowing(
                def.name.to_string().into(),
                "module",
                (def.span.start, def.span.end),
                import_source,
            ));
        }

        // Validate logic block if present (Gap 5.8: Combinational Loop Detection)
        if let Some(ref _logic_block) = def.logic {
            // Extract module pins for validation
            let _module_pins: Vec<(String, Option<usize>)> = def
                .pins
                .iter()
                .map(|pin| (pin.name.clone().into(), pin.array_size))
                .collect();

            // TODO: Logic validation during registration
            // This requires a HardwareSpace which we don't have during registration
            // Validation will happen during actual synthesis
            /*
            // Create a logic synthesizer to validate the logic block
            use crate::logic_synthesizer::LogicSynthesizer;
            let mut synthesizer = LogicSynthesizer::new(self);

            // Run validation (this will check for combinational loops, width mismatches, etc.)
            // Errors are reported to the collector
            let (_comps, _nets, warnings) =
                synthesizer.synthesize_logic_block(collector, logic_block, &module_pins);

            // Report warnings to collector
            for warning in warnings {
                collector.report(warning);
            }
            */

            // If validation had errors, skip registration
            if collector.has_errors() {
                return;
            }
        }

        self.local.modules.insert(name_str.into(), def);
    }

    /// Register a mechanical definition (in local layer)
    pub fn register_mechanical(
        &mut self,
        collector: &DiagnosticCollector,
        def: MechanicalDefinition,
    ) {
        let name_str = def.name.as_str();
        if let Some(existing) = self.local.mechanicals.get(name_str) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "mechanical",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }
        self.local.mechanicals.insert(name_str.into(), def);
    }

    /// Register an interface definition (in local layer)
    pub fn register_interface(
        &mut self,
        collector: &DiagnosticCollector,
        def: InterfaceDefinition,
    ) {
        let name_str = def.name.as_str();
        if let Some(existing) = self.local.interfaces.get(name_str) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "interface",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }
        self.local.interfaces.insert(name_str.into(), def);
    }

    /// Register a test definition (in local layer)
    pub fn register_test(&mut self, collector: &DiagnosticCollector, def: TestDefinition) {
        let name_str = def.name.as_str();
        if let Some(existing) = self.local.tests.get(name_str) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "test",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }
        self.local.tests.insert(name_str.into(), def);
    }

    /// Register a signal group definition (in local layer)
    pub fn register_signal_group(
        &mut self,
        collector: &DiagnosticCollector,
        def: SignalGroupDefinition,
    ) {
        let name_str = def.name.as_str();
        if let Some(existing) = self.local.signal_groups.get(name_str) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "signal_group",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }
        self.local.signal_groups.insert(name_str.into(), def);
    }

    /// Register a pattern definition (in local layer)
    pub fn register_pattern(&mut self, collector: &DiagnosticCollector, def: PatternDefinition) {
        let name_str = def.name.as_str();
        if let Some(existing) = self.local.patterns.get(name_str) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "pattern",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }
        self.local.patterns.insert(name_str.into(), def);
    }

    /// Register a strategy definition (in local layer)
    pub fn register_strategy(&mut self, collector: &DiagnosticCollector, def: StrategyDefinition) {
        let name_str = def.name.as_str();
        if let Some(existing) = self.local.strategies.get(name_str) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "strategy",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }
        self.local.strategies.insert(name_str.into(), def);
    }

    /// Register a logic block definition (in local layer)
    pub fn register_logic(&mut self, collector: &DiagnosticCollector, def: LogicDefinition) {
        let name_str = def.name.as_str();
        if let Some(existing) = self.local.logic_blocks.get(name_str) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "logic",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }
        self.local.logic_blocks.insert(name_str.into(), def);
    }

    /// Register an enum definition (in local layer)
    pub fn register_enum(&mut self, collector: &DiagnosticCollector, def: EnumDefinition) {
        let name_str = def.name.as_str();
        if let Some(existing) = self.local.enums.get(name_str) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "enum",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }
        self.local.enums.insert(name_str.into(), def);
    }

    /// Register a struct definition (in local layer)
    pub fn register_struct(&mut self, collector: &DiagnosticCollector, def: StructDefinition) {
        let name_str = def.name.as_str();
        if let Some(existing) = self.local.structs.get(name_str) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "struct",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }
        self.local.structs.insert(name_str.into(), def);
    }

    /// Register a shape definition (in local layer)
    pub fn register_shape(&mut self, collector: &DiagnosticCollector, def: ShapeDefinition) {
        let name_str = def.name.as_str();
        if let Some(existing) = self.local.shapes.get(name_str) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "shape",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }
        self.local.shapes.insert(name_str.into(), def);
    }

    /// Register a unit definition in the local layer (user-defined units in current file)
    ///
    /// This allows users to define custom units in their .hw files that will be
    /// available for use in measurements throughout the file.
    pub fn register_unit(&mut self, collector: &DiagnosticCollector, def: UnitDefinition) {
        let symbol = def.symbol.clone();

        // Check for duplicate in local layer
        if let Some(existing) = self.local.units.values().find(|u| u.symbol == symbol) {
            collector.report(SymbolError::duplicate(
                symbol.clone(),
                "unit",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }

        // Check if this shadows an imported or prelude unit
        if self.resolve_unit_symbol(&symbol).is_some() {
            collector.report(SymbolError::shadowing(
                symbol.clone(),
                "unit",
                (def.span.start, def.span.end),
                "stdlib or imported library".into(),
            ));
        }

        self.local.units.insert(symbol, def);
    }
}
