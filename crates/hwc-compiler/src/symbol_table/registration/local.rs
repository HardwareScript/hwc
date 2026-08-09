use super::super::{error::SymbolError, layer::SymbolTable, Definition};
use compact_str::CompactString;
use hwc_diagnostics::DiagnosticCollector;
use hwc_parser::{
    logic::{EnumDefinition, LogicDefinition, StructDefinition},
    BridgeDefinition, ComponentDefinition, DeviceDefinition, InterfaceDefinition,
    MaterialAliasDefinition, MaterialDefinition, MechanicalDefinition, ModuleDefinition,
    PatternDefinition, ProfileDefinition, ShapeDefinition, SignalGroupDefinition,
    SpiceModelDefinition, StrategyDefinition, SubcircuitDefinition, TestDefinition, UnitDefinition,
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
                Some((
                    self.arena.material_alias_defs[*existing].span.start,
                    self.arena.material_alias_defs[*existing].span.end,
                )),
            ));
            return;
        }

        let id = self.arena.material_alias_defs.push(def);
        self.local
            .insert(name_str.into(), Definition::MaterialAlias(id));
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
                Some((
                    self.arena.material_defs[*existing].span.start,
                    self.arena.material_defs[*existing].span.end,
                )),
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

        let id = self.arena.material_defs.push(material_to_register);
        self.local.insert(name_str.into(), Definition::Material(id));
    }

    /// Find a material in lower layers (HPM > Prelude > Core), excluding local layer
    fn find_material_in_lower_layers(&self, name: &str) -> Option<&MaterialDefinition> {
        for layer in self.hpm.iter().rev() {
            if let Some(Definition::Material(mat)) = layer.get(name) {
                return Some(&self.arena.material_defs[*mat]);
            }
        }

        if let Some(Definition::Material(mat)) = self.prelude.get(name) {
            return Some(&self.arena.material_defs[*mat]);
        }

        if let Some(Definition::Material(mat)) = self.core.get(name) {
            return Some(&self.arena.material_defs[*mat]);
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
        let name_str = def.name.as_str().to_string();
        if let Some(Definition::Profile(existing)) = self.local.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "profile",
                (def.span.start, def.span.end),
                Some((
                    self.arena.profile_defs[*existing].span.start,
                    self.arena.profile_defs[*existing].span.end,
                )),
            ));
            return;
        }

        if let Some(import_source) = self.check_profile_shadowing(&name_str) {
            collector.report(SymbolError::shadowing(
                def.name.to_string().into(),
                "profile",
                (def.span.start, def.span.end),
                import_source,
            ));
        }

        let id = self.arena.profile_defs.push(def);
        self.local.insert(name_str.into(), Definition::Profile(id));
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
                Some((
                    self.arena.component_defs[*existing].span.start,
                    self.arena.component_defs[*existing].span.end,
                )),
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

        let id = self.arena.component_defs.push(def);
        self.local
            .insert(name_str.clone().into(), Definition::Component(id));
    }

    /// Register a module definition (in local layer)
    ///
    /// Rule 1 (GAP3): Local Beats Global - warns if shadowing an import
    pub fn register_module(&mut self, collector: &DiagnosticCollector, def: ModuleDefinition) {
        let name_str = def.name.as_str().to_string();
        if let Some(Definition::Module(existing)) = self.local.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "module",
                (def.span.start, def.span.end),
                Some((
                    self.arena.module_defs[*existing].span.start,
                    self.arena.module_defs[*existing].span.end,
                )),
            ));
            return;
        }

        if let Some(import_source) = self.check_module_shadowing(&name_str) {
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

        let id = self.arena.module_defs.push(def);
        self.local.insert(name_str.into(), Definition::Module(id));
    }

    /// Register a mechanical definition (in local layer)
    pub fn register_mechanical(
        &mut self,
        collector: &DiagnosticCollector,
        def: MechanicalDefinition,
    ) {
        let name_str = def.name.as_str().to_string();
        if let Some(Definition::Mechanical(existing)) = self.local.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "mechanical",
                (def.span.start, def.span.end),
                Some((
                    self.arena.mechanical_defs[*existing].span.start,
                    self.arena.mechanical_defs[*existing].span.end,
                )),
            ));
            return;
        }
        let id = self.arena.mechanical_defs.push(def);
        self.local
            .insert(name_str.into(), Definition::Mechanical(id));
    }

    /// Register an interface definition (in local layer)
    pub fn register_interface(
        &mut self,
        collector: &DiagnosticCollector,
        def: InterfaceDefinition,
    ) {
        let name_str = def.name.as_str().to_string();
        if let Some(Definition::Interface(existing)) = self.local.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "interface",
                (def.span.start, def.span.end),
                Some((
                    self.arena.interface_defs[*existing].span.start,
                    self.arena.interface_defs[*existing].span.end,
                )),
            ));
            return;
        }
        let id = self.arena.interface_defs.push(def);
        self.local
            .insert(name_str.into(), Definition::Interface(id));
    }

    /// Register a test definition (in local layer)
    pub fn register_test(&mut self, collector: &DiagnosticCollector, def: TestDefinition) {
        let name_str = def.name.as_str().to_string();
        if let Some(Definition::Test(existing)) = self.local.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "test",
                (def.span.start, def.span.end),
                Some((
                    self.arena.test_defs[*existing].span.start,
                    self.arena.test_defs[*existing].span.end,
                )),
            ));
            return;
        }
        let id = self.arena.test_defs.push(def);
        self.local.insert(name_str.into(), Definition::Test(id));
    }

    /// Register a signal group definition (in local layer)
    pub fn register_signal_group(
        &mut self,
        collector: &DiagnosticCollector,
        def: SignalGroupDefinition,
    ) {
        let name_str = def.name.as_str().to_string();
        if let Some(Definition::SignalGroup(existing)) = self.local.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "signal_group",
                (def.span.start, def.span.end),
                Some((
                    self.arena.signal_group_defs[*existing].span.start,
                    self.arena.signal_group_defs[*existing].span.end,
                )),
            ));
            return;
        }
        let id = self.arena.signal_group_defs.push(def);
        self.local
            .insert(name_str.into(), Definition::SignalGroup(id));
    }

    /// Register a pattern definition (in local layer)
    pub fn register_pattern(&mut self, collector: &DiagnosticCollector, def: PatternDefinition) {
        let name_str = def.name.as_str().to_string();
        if let Some(Definition::Pattern(existing)) = self.local.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "pattern",
                (def.span.start, def.span.end),
                Some((
                    self.arena.pattern_defs[*existing].span.start,
                    self.arena.pattern_defs[*existing].span.end,
                )),
            ));
            return;
        }
        let id = self.arena.pattern_defs.push(def);
        self.local.insert(name_str.into(), Definition::Pattern(id));
    }

    /// Register a strategy definition (in local layer)
    pub fn register_strategy(&mut self, collector: &DiagnosticCollector, def: StrategyDefinition) {
        let name_str = def.name.as_str().to_string();
        if let Some(Definition::Strategy(existing)) = self.local.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "strategy",
                (def.span.start, def.span.end),
                Some((
                    self.arena.strategy_defs[*existing].span.start,
                    self.arena.strategy_defs[*existing].span.end,
                )),
            ));
            return;
        }
        let id = self.arena.strategy_defs.push(def);
        self.local.insert(name_str.into(), Definition::Strategy(id));
    }

    /// Register a logic block definition (in local layer)
    pub fn register_logic(&mut self, collector: &DiagnosticCollector, def: LogicDefinition) {
        let name_str = def.name.as_str().to_string();
        if let Some(Definition::Logic(existing)) = self.local.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "logic",
                (def.span.start, def.span.end),
                Some((
                    self.arena.logic_defs[*existing].span.start,
                    self.arena.logic_defs[*existing].span.end,
                )),
            ));
            return;
        }
        let id = self.arena.logic_defs.push(def);
        self.local.insert(name_str.into(), Definition::Logic(id));
    }

    /// Register an enum definition (in local layer)
    pub fn register_enum(&mut self, collector: &DiagnosticCollector, def: EnumDefinition) {
        let name_str = def.name.as_str().to_string();
        if let Some(Definition::Enum(existing)) = self.local.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "enum",
                (def.span.start, def.span.end),
                Some((
                    self.arena.enum_defs[*existing].span.start,
                    self.arena.enum_defs[*existing].span.end,
                )),
            ));
            return;
        }
        let id = self.arena.enum_defs.push(def);
        self.local.insert(name_str.into(), Definition::Enum(id));
    }

    /// Register a struct definition (in local layer)
    pub fn register_struct(&mut self, collector: &DiagnosticCollector, def: StructDefinition) {
        let name_str = def.name.as_str().to_string();
        if let Some(Definition::Struct(existing)) = self.local.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "struct",
                (def.span.start, def.span.end),
                Some((
                    self.arena.struct_defs[*existing].span.start,
                    self.arena.struct_defs[*existing].span.end,
                )),
            ));
            return;
        }
        let id = self.arena.struct_defs.push(def);
        self.local.insert(name_str.into(), Definition::Struct(id));
    }

    /// Register a shape definition (in local layer)
    pub fn register_shape(&mut self, collector: &DiagnosticCollector, def: ShapeDefinition) {
        let name_str = def.name.as_str().to_string();
        if let Some(Definition::Shape(existing)) = self.local.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "shape",
                (def.span.start, def.span.end),
                Some((
                    self.arena.shape_defs[*existing].span.start,
                    self.arena.shape_defs[*existing].span.end,
                )),
            ));
            return;
        }
        let id = self.arena.shape_defs.push(def);
        self.local.insert(name_str.into(), Definition::Shape(id));
    }

    /// Register a unit definition in the local layer (user-defined units in current file)
    ///
    /// This allows users to define custom units in their .hw files that will be
    /// available for use in measurements throughout the file.
    pub fn register_unit(&mut self, collector: &DiagnosticCollector, def: UnitDefinition) {
        let symbol = def.symbol.clone();

        if let Some(Definition::Unit(existing)) = self
            .local
            .iter()
            .find(|(_, d)| matches!(d, Definition::Unit(u) if self.arena.unit_defs[*u].symbol == symbol))
            .map(|(_, d)| d)
        {
            collector.report(SymbolError::duplicate(
                symbol.clone(),
                "unit",
                (def.span.start, def.span.end),
                Some((self.arena.unit_defs[*existing].span.start, self.arena.unit_defs[*existing].span.end)),
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

        let id = self.arena.unit_defs.push(def);
        self.local.insert(symbol, Definition::Unit(id));
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
        let id = self.arena.bridge_defs.push(def);
        self.local.insert(key, Definition::Bridge(id));
    }

    /// Register a device definition (in local layer)
    ///
    /// Devices define the physical contract for foundry primitives (transistors,
    /// diodes, resistors, etc.), specifying required terminals and expected
    /// materials for each terminal.
    ///
    /// Rule 1 (GAP3): Local Beats Global - warns if shadowing an import
    pub fn register_device(&mut self, collector: &DiagnosticCollector, def: DeviceDefinition) {
        let name_str = def.name.as_str().to_string();
        if let Some(Definition::Device(existing)) = self.local.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "device",
                (def.span.start, def.span.end),
                Some((
                    self.arena.device_defs[*existing].span.start,
                    self.arena.device_defs[*existing].span.end,
                )),
            ));
            return;
        }

        // Check if shadowing imported device
        if self.check_device_shadowing(&name_str) {
            collector.report(SymbolError::shadowing(
                def.name.to_string().into(),
                "device",
                (def.span.start, def.span.end),
                "imported library".into(),
            ));
        }

        let id = self.arena.device_defs.push(def);
        self.local.insert(name_str.into(), Definition::Device(id));
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

    /// Register a SPICE model definition (in local layer)
    ///
    /// SPICE models provide analytical models for device simulation.
    /// Example: .model NMOS NMOS (VTO=0.7 KP=120u)
    pub fn register_spice_model(
        &mut self,
        collector: &DiagnosticCollector,
        def: SpiceModelDefinition,
    ) {
        let name_str = def.name.as_str().to_string();
        if let Some(Definition::SpiceModel(existing)) = self.local.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "spice_model",
                (def.span.start, def.span.end),
                Some((
                    self.arena.spice_model_defs[*existing].span.start,
                    self.arena.spice_model_defs[*existing].span.end,
                )),
            ));
            return;
        }
        let id = self.arena.spice_model_defs.push(def);
        self.local
            .insert(name_str.into(), Definition::SpiceModel(id));
    }

    /// Register a subcircuit definition (in local layer)
    ///
    /// Subcircuits provide foundry-supplied compact models for devices.
    /// These are typed, validated AST structures (not raw SPICE strings).
    ///
    /// Example:
    /// ```hw
    /// subcircuit sky130_fd_pr__res_high_po:
    ///     terminals: [PLUS, MINUS, BULK]
    ///     parameters: [W = 1.0um, L = 1.0um]
    ///     elements:
    ///         R_head: Resistor(PLUS, node_1, val: 362.0ohm)
    ///         R_body: Resistor(node_1, node_2, val: 350.0ohm_sq * (L / W))
    ///         ...
    /// ```
    pub fn register_subcircuit(
        &mut self,
        collector: &DiagnosticCollector,
        def: SubcircuitDefinition,
    ) {
        let name_str = def.name.as_str().to_string();
        if let Some(Definition::Subcircuit(existing)) = self.local.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                def.name.to_string().into(),
                "subcircuit",
                (def.span.start, def.span.end),
                Some((
                    self.arena.subcircuit_defs[*existing].span.start,
                    self.arena.subcircuit_defs[*existing].span.end,
                )),
            ));
            return;
        }
        let id = self.arena.subcircuit_defs.push(def);
        self.local
            .insert(name_str.into(), Definition::Subcircuit(id));
    }
}
