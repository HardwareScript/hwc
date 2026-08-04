use super::super::{error::SymbolError, layer::SymbolTable, Definition};
use compact_str::CompactString;
use hwc_diagnostics::DiagnosticCollector;
use hwc_parser::{
    logic::{EnumDefinition, LogicDefinition, StructDefinition},
    BridgeDefinition, ComponentDefinition, DeviceDefinition, InterfaceDefinition,
    MaterialAliasDefinition, MaterialDefinition, MechanicalDefinition, ModuleDefinition,
    PatternDefinition, ProfileDefinition, ShapeDefinition, SignalGroupDefinition,
    StrategyDefinition, TestDefinition, UnitDefinition,
};

impl SymbolTable {
    /// Register a material alias (in local layer)
    pub fn register_material_alias(
        &mut self,
        collector: &DiagnosticCollector,
        def: MaterialAliasDefinition,
    ) {
        let name_str = def.name.as_str().to_string();

        if let Some(Definition::MaterialAlias(existing)) = self.local.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "material_alias",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }

        self.local.insert(name_str.into(), Definition::MaterialAlias(def));
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

        if let Some(Definition::Material(existing)) = self.local.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "material",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }

        if let Some(import_source) = self.check_material_shadowing(&name_str) {
            collector.report(SymbolError::shadowing(
                def.name.to_string().into(),
                "material",
                (def.span.start, def.span.end),
                import_source,
            ));
        }

        let material_to_register =
            if let Some(base_material) = self.find_material_in_lower_layers(&name_str) {
                self.merge_properties(base_material, &def)
            } else {
                def
            };

        self.local.insert(name_str.into(), Definition::Material(material_to_register));
    }

    /// Find a material in lower layers (HPM > Prelude > Core), excluding local layer
    fn find_material_in_lower_layers(&self, name: &str) -> Option<&MaterialDefinition> {
        for layer in self.hpm.iter().rev() {
            if let Some(Definition::Material(mat)) = layer.get(name) {
                return Some(mat);
            }
        }

        if let Some(Definition::Material(mat)) = self.prelude.get(name) {
            return Some(mat);
        }

        if let Some(Definition::Material(mat)) = self.core.get(name) {
            return Some(mat);
        }

        None
    }

    /// Check if a material exists in lower layers and return the source layer name
    fn check_material_shadowing(&self, name: &str) -> Option<CompactString> {
        for layer in self.hpm.iter().rev() {
            if layer.get(name).is_some() {
                return Some("imported library".into());
            }
        }
        if self.prelude.get(name).is_some() {
            return Some("@std/materials".into());
        }
        if self.core.get(name).is_some() {
            return Some("core library".into());
        }
        None
    }

    /// Check if a profile exists in lower layers and return the source layer name
    fn check_profile_shadowing(&self, name: &str) -> Option<CompactString> {
        for layer in self.hpm.iter().rev() {
            if layer.get(name).is_some() {
                return Some("imported library".into());
            }
        }
        if self.prelude.get(name).is_some() {
            return Some("@std/profiles".into());
        }
        if self.core.get(name).is_some() {
            return Some("core library".into());
        }
        None
    }

    /// Check if a component exists in lower layers and return the source layer name
    fn check_component_shadowing(&self, name: &str) -> Option<CompactString> {
        for layer in self.hpm.iter().rev() {
            if layer.get(name).is_some() {
                return Some("imported library".into());
            }
        }
        if self.prelude.get(name).is_some() {
            return Some("@std/components".into());
        }
        if self.core.get(name).is_some() {
            return Some("core library".into());
        }
        None
    }

    /// Check if a module exists in lower layers and return the source layer name
    fn check_module_shadowing(&self, name: &str) -> Option<CompactString> {
        for layer in self.hpm.iter().rev() {
            if layer.get(name).is_some() {
                return Some("imported library".into());
            }
        }
        if self.prelude.get(name).is_some() {
            return Some("@std/modules".into());
        }
        if self.core.get(name).is_some() {
            return Some("core library".into());
        }
        None
    }

    /// Register a profile definition (in local layer)
    ///
    /// Rule 1 (GAP3): Local Beats Global - warns if shadowing an import
    pub fn register_profile(&mut self, collector: &DiagnosticCollector, def: ProfileDefinition) {
        let name_str = def.name.as_str();
        if let Some(Definition::Profile(existing)) = self.local.get(name_str) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "profile",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }

        if let Some(import_source) = self.check_profile_shadowing(name_str) {
            collector.report(SymbolError::shadowing(
                def.name.to_string().into(),
                "profile",
                (def.span.start, def.span.end),
                import_source,
            ));
        }

        self.local.insert(name_str.into(), Definition::Profile(def));
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
        if let Some(Definition::Component(existing)) = self.local.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "component",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }

        if let Some(import_source) = self.check_component_shadowing(&name_str) {
            collector.report(SymbolError::shadowing(
                def.name.to_string().into(),
                "component",
                (def.span.start, def.span.end),
                import_source,
            ));
        }

        self.local.insert(name_str.clone().into(), Definition::Component(def));
    }

    /// Register a module definition (in local layer)
    ///
    /// Rule 1 (GAP3): Local Beats Global - warns if shadowing an import
    pub fn register_module(&mut self, collector: &DiagnosticCollector, def: ModuleDefinition) {
        let name_str = def.name.as_str();
        if let Some(Definition::Module(existing)) = self.local.get(name_str) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "module",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }

        if let Some(import_source) = self.check_module_shadowing(name_str) {
            collector.report(SymbolError::shadowing(
                def.name.to_string().into(),
                "module",
                (def.span.start, def.span.end),
                import_source,
            ));
        }

        if let Some(ref _logic_block) = def.logic {
            let _module_pins: Vec<(String, Option<usize>)> = def
                .pins
                .iter()
                .map(|pin| (pin.name.clone().into(), pin.array_size))
                .collect();

            if collector.has_errors() {
                return;
            }
        }

        self.local.insert(name_str.into(), Definition::Module(def));
    }

    /// Register a mechanical definition (in local layer)
    pub fn register_mechanical(
        &mut self,
        collector: &DiagnosticCollector,
        def: MechanicalDefinition,
    ) {
        let name_str = def.name.as_str();
        if let Some(Definition::Mechanical(existing)) = self.local.get(name_str) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "mechanical",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }
        self.local.insert(name_str.into(), Definition::Mechanical(def));
    }

    /// Register an interface definition (in local layer)
    pub fn register_interface(
        &mut self,
        collector: &DiagnosticCollector,
        def: InterfaceDefinition,
    ) {
        let name_str = def.name.as_str();
        if let Some(Definition::Interface(existing)) = self.local.get(name_str) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "interface",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }
        self.local.insert(name_str.into(), Definition::Interface(def));
    }

    /// Register a test definition (in local layer)
    pub fn register_test(&mut self, collector: &DiagnosticCollector, def: TestDefinition) {
        let name_str = def.name.as_str();
        if let Some(Definition::Test(existing)) = self.local.get(name_str) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "test",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }
        self.local.insert(name_str.into(), Definition::Test(def));
    }

    /// Register a signal group definition (in local layer)
    pub fn register_signal_group(
        &mut self,
        collector: &DiagnosticCollector,
        def: SignalGroupDefinition,
    ) {
        let name_str = def.name.as_str();
        if let Some(Definition::SignalGroup(existing)) = self.local.get(name_str) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "signal_group",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }
        self.local.insert(name_str.into(), Definition::SignalGroup(def));
    }

    /// Register a pattern definition (in local layer)
    pub fn register_pattern(&mut self, collector: &DiagnosticCollector, def: PatternDefinition) {
        let name_str = def.name.as_str();
        if let Some(Definition::Pattern(existing)) = self.local.get(name_str) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "pattern",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }
        self.local.insert(name_str.into(), Definition::Pattern(def));
    }

    /// Register a strategy definition (in local layer)
    pub fn register_strategy(&mut self, collector: &DiagnosticCollector, def: StrategyDefinition) {
        let name_str = def.name.as_str();
        if let Some(Definition::Strategy(existing)) = self.local.get(name_str) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "strategy",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }
        self.local.insert(name_str.into(), Definition::Strategy(def));
    }

    /// Register a logic block definition (in local layer)
    pub fn register_logic(&mut self, collector: &DiagnosticCollector, def: LogicDefinition) {
        let name_str = def.name.as_str();
        if let Some(Definition::Logic(existing)) = self.local.get(name_str) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "logic",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }
        self.local.insert(name_str.into(), Definition::Logic(def));
    }

    /// Register an enum definition (in local layer)
    pub fn register_enum(&mut self, collector: &DiagnosticCollector, def: EnumDefinition) {
        let name_str = def.name.as_str();
        if let Some(Definition::Enum(existing)) = self.local.get(name_str) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "enum",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }
        self.local.insert(name_str.into(), Definition::Enum(def));
    }

    /// Register a struct definition (in local layer)
    pub fn register_struct(&mut self, collector: &DiagnosticCollector, def: StructDefinition) {
        let name_str = def.name.as_str();
        if let Some(Definition::Struct(existing)) = self.local.get(name_str) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "struct",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }
        self.local.insert(name_str.into(), Definition::Struct(def));
    }

    /// Register a shape definition (in local layer)
    pub fn register_shape(&mut self, collector: &DiagnosticCollector, def: ShapeDefinition) {
        let name_str = def.name.as_str();
        if let Some(Definition::Shape(existing)) = self.local.get(name_str) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "shape",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }
        self.local.insert(name_str.into(), Definition::Shape(def));
    }

    /// Register a unit definition in the local layer (user-defined units in current file)
    ///
    /// This allows users to define custom units in their .hw files that will be
    /// available for use in measurements throughout the file.
    pub fn register_unit(&mut self, collector: &DiagnosticCollector, def: UnitDefinition) {
        let symbol = def.symbol.clone();

        if let Some(Definition::Unit(existing)) = self.local.iter()
            .find(|(_, d)| matches!(d, Definition::Unit(u) if u.symbol == symbol))
            .map(|(_, d)| d) 
        {
            collector.report(SymbolError::duplicate(
                symbol.clone(),
                "unit",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }

        if self.resolve_unit_symbol(&symbol).is_some() {
            collector.report(SymbolError::shadowing(
                symbol.clone(),
                "unit",
                (def.span.start, def.span.end),
                "stdlib or imported library".into(),
            ));
        }

        self.local.insert(symbol, Definition::Unit(def));
    }

    /// Register a bridge definition (in local layer) (v0.2.0)
    ///
    /// Bridges define physical material transitions for via generation.
    /// Multiple bridges for the same material pair are allowed (e.g., different
    /// interface/fill combinations for different process nodes).
    ///
    /// The via resolver will query all registered bridges and select the most
    /// appropriate one based on the stackup and manufacturing constraints.
    pub fn register_bridge(&mut self, _collector: &DiagnosticCollector, def: BridgeDefinition) {
        let key = CompactString::from(format!("{}_{}", def.from, def.to));
        self.local.insert(key, Definition::Bridge(def));
    }

    /// Register a device definition (in local layer)
    ///
    /// Devices define the physical contract for foundry primitives (transistors,
    /// diodes, resistors, etc.), specifying required terminals and expected
    /// materials for each terminal.
    ///
    /// Rule 1 (GAP3): Local Beats Global - warns if shadowing an import
    pub fn register_device(&mut self, collector: &DiagnosticCollector, def: DeviceDefinition) {
        let name_str = def.name.as_str();
        if let Some(Definition::Device(existing)) = self.local.get(name_str) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "device",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
            return;
        }

        // Check if shadowing imported device
        if self.check_device_shadowing(name_str) {
            collector.report(SymbolError::shadowing(
                def.name.to_string().into(),
                "device",
                (def.span.start, def.span.end),
                "imported library".into(),
            ));
        }

        self.local.insert(name_str.into(), Definition::Device(def));
    }

    /// Check if a device exists in lower layers and return true if shadowing
    fn check_device_shadowing(&self, name: &str) -> bool {
        for layer in self.hpm.iter().rev() {
            if layer.get(name).is_some() {
                return true;
            }
        }
        self.prelude.get(name).is_some() || self.core.get(name).is_some()
    }
}
