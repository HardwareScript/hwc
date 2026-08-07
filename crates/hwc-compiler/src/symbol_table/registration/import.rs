use super::super::layer::SymbolTable;
use hwc_parser::{
    logic::{EnumDefinition, LogicDefinition, StructDefinition},
    BridgeDefinition, ComponentDefinition, InterfaceDefinition, MaterialAliasDefinition,
    MaterialDefinition, MechanicalDefinition, ModuleDefinition, PatternDefinition,
    ProfileDefinition, ShapeDefinition, SignalGroupDefinition, SpaceDefinition,
    SpiceModelDefinition, SubcircuitDefinition, StrategyDefinition, TestDefinition, UnitDefinition,
};

use super::super::Definition;

impl SymbolTable {
    /// Register an imported space definition (in HPM layer) (v0.2.1)
    ///
    /// v0.2.1: Hierarchical Space Composition support
    /// Stores ALL definitions (exported and private) for proper scoped resolution
    pub fn register_import_space(&mut self, def: SpaceDefinition) {
        let name_str = def.name.as_str().to_string();

        eprintln!("[DEBUG] register_import_space called for space: {}", name_str);

        if self.hpm.is_empty() {
            eprintln!("[DEBUG] HPM is empty, creating new layer");
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }

        self.hpm
            .last_mut()
            .unwrap()
            .insert(name_str.clone().into(), Definition::Space(def));

        eprintln!("[DEBUG] Space '{}' registered in HPM layer. Total HPM layers: {}", name_str, self.hpm.len());
    }

    /// Register an imported bridge definition (in HPM layer) (v0.2.0)
    /// v0.2.0: Stores ALL definitions (exported and private) for proper scoped resolution
    pub fn register_import_bridge(&mut self, def: BridgeDefinition) {
        let key = format!("{}_{}", def.from, def.to);

        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }

        self.hpm
            .last_mut()
            .unwrap()
            .insert(key.into(), Definition::Bridge(def));
    }

    /// Register an imported material alias (in HPM layer)
    /// v0.2.0: Stores ALL definitions (exported and private) for proper scoped resolution
    pub fn register_import_material_alias(&mut self, def: MaterialAliasDefinition) {
        let name_str = def.name.as_str().to_string();

        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }

        self.hpm
            .last_mut()
            .unwrap()
            .insert(name_str.into(), Definition::MaterialAlias(def));
    }

    /// Register an imported unit definition (in HPM layer)
    /// v0.2.0: Stores ALL definitions (exported and private) for proper scoped resolution
    pub fn register_import_unit(&mut self, def: UnitDefinition) {
        let symbol = def.symbol.clone();
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        self.hpm.last_mut().unwrap().insert(symbol, Definition::Unit(def));
    }

    /// Register an imported device definition (in HPM layer)
    /// v0.2.0: Stores ALL definitions (exported and private) for proper scoped resolution
    pub fn register_import_device(&mut self, def: hwc_parser::DeviceDefinition) {
        let name = def.name.as_str().to_string();
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        self.hpm
            .last_mut()
            .unwrap()
            .insert(name.into(), Definition::Device(def));
    }

    /// Register an imported constant (in HPM layer)
    /// v0.2.0: Stores ALL definitions (exported and private) for proper scoped resolution
    pub fn register_import_constant(&mut self, def: hwc_parser::ConstDefinition) {
        let name = def.name.clone();
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        self.hpm.last_mut().unwrap().insert(name, Definition::Const(def));
    }

    /// Register an imported shape definition (in HPM layer)
    /// v0.2.0: Stores ALL definitions (exported and private) for proper scoped resolution
    pub fn register_import_shape(&mut self, def: ShapeDefinition) {
        let name_str = def.name.as_str().to_string();
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        self.hpm
            .last_mut()
            .unwrap()
            .insert(name_str.into(), Definition::Shape(def));
    }

    /// Register an imported material definition (in HPM layer)
    ///
    /// This is used by the ModuleResolver when processing import statements.
    /// Imported materials go into the HPM layer, not the local layer.
    ///
    /// v0.2.0: Stores ALL definitions (exported and private) for proper scoped resolution.
    /// Export filtering happens during name resolution, not during registration.
    pub fn register_import_material(&mut self, def: MaterialDefinition) {
        let name_str = def.name.as_str().to_string();

        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }

        self.hpm
            .last_mut()
            .unwrap()
            .insert(name_str.into(), Definition::Material(def));
    }

    /// Register an imported profile definition (in HPM layer)
    /// v0.2.0: Stores ALL definitions (exported and private) for proper scoped resolution
    pub fn register_import_profile(&mut self, def: ProfileDefinition) {
        let name_str = def.name.as_str().to_string();
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        self.hpm
            .last_mut()
            .unwrap()
            .insert(name_str.into(), Definition::Profile(def));
    }

    /// Register an imported component definition (in HPM layer)
    ///
    /// v0.1.6 SEMANTIC BAKING: Also bakes imported components for performance.
    /// v0.2.0: Stores ALL definitions (exported and private) for proper scoped resolution
    pub fn register_import_component(&mut self, def: ComponentDefinition) {
        let name_str = def.name.as_str().to_string();
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        self.hpm
            .last_mut()
            .unwrap()
            .insert(name_str.clone().into(), Definition::Component(def));
    }

    /// Register an imported module definition (in HPM layer)
    /// v0.2.0: Stores ALL definitions (exported and private) for proper scoped resolution
    pub fn register_import_module(&mut self, def: ModuleDefinition) {
        let name_str = def.name.as_str().to_string();
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        self.hpm
            .last_mut()
            .unwrap()
            .insert(name_str.into(), Definition::Module(def));
    }

    /// Register an imported mechanical definition (in HPM layer)
    /// v0.2.0: Stores ALL definitions (exported and private) for proper scoped resolution
    pub fn register_import_mechanical(&mut self, def: MechanicalDefinition) {
        let name_str = def.name.as_str().to_string();
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        self.hpm
            .last_mut()
            .unwrap()
            .insert(name_str.into(), Definition::Mechanical(def));
    }

    /// Register an imported interface definition (in HPM layer)
    /// v0.2.0: Stores ALL definitions (exported and private) for proper scoped resolution
    pub fn register_import_interface(&mut self, def: InterfaceDefinition) {
        let name_str = def.name.as_str().to_string();
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        self.hpm
            .last_mut()
            .unwrap()
            .insert(name_str.into(), Definition::Interface(def));
    }

    /// Register an imported signal group definition (in HPM layer)
    /// v0.2.0: Stores ALL definitions (exported and private) for proper scoped resolution
    pub fn register_import_signal_group(&mut self, def: SignalGroupDefinition) {
        let name_str = def.name.as_str().to_string();
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        self.hpm
            .last_mut()
            .unwrap()
            .insert(name_str.into(), Definition::SignalGroup(def));
    }

    /// Register an imported pattern definition (in HPM layer)
    /// v0.2.0: Stores ALL definitions (exported and private) for proper scoped resolution
    pub fn register_import_pattern(&mut self, def: PatternDefinition) {
        let name_str = def.name.as_str().to_string();
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        self.hpm
            .last_mut()
            .unwrap()
            .insert(name_str.into(), Definition::Pattern(def));
    }

    /// Register an imported strategy definition (in HPM layer)
    /// v0.2.0: Stores ALL definitions (exported and private) for proper scoped resolution
    pub fn register_import_strategy(&mut self, def: StrategyDefinition) {
        let name_str = def.name.as_str().to_string();
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        self.hpm
            .last_mut()
            .unwrap()
            .insert(name_str.into(), Definition::Strategy(def));
    }

    /// Register an imported logic block definition (in HPM layer)
    /// v0.2.0: Stores ALL definitions (exported and private) for proper scoped resolution
    pub fn register_import_logic(&mut self, def: LogicDefinition) {
        let name_str = def.name.as_str().to_string();
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        self.hpm
            .last_mut()
            .unwrap()
            .insert(name_str.into(), Definition::Logic(def));
    }

    /// Register an imported enum definition (in HPM layer)
    /// v0.2.0: Stores ALL definitions (exported and private) for proper scoped resolution
    pub fn register_import_enum(&mut self, def: EnumDefinition) {
        let name_str = def.name.as_str().to_string();
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        self.hpm
            .last_mut()
            .unwrap()
            .insert(name_str.into(), Definition::Enum(def));
    }

    /// Register an imported struct definition (in HPM layer)
    /// v0.2.0: Stores ALL definitions (exported and private) for proper scoped resolution
    pub fn register_import_struct(&mut self, def: StructDefinition) {
        let name_str = def.name.as_str().to_string();
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        self.hpm
            .last_mut()
            .unwrap()
            .insert(name_str.into(), Definition::Struct(def));
    }

    /// Register an imported test definition (in HPM layer)
    /// v0.2.0: Stores ALL definitions (exported and private) for proper scoped resolution
    pub fn register_import_test(&mut self, def: TestDefinition) {
        let name_str = def.name.as_str().to_string();
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        self.hpm
            .last_mut()
            .unwrap()
            .insert(name_str.into(), Definition::Test(def));
    }

    /// Register an imported SPICE model definition (in HPM layer) (v0.2.1)
    ///
    /// v0.2.1: SPICE Model Card support for PDK physics
    /// Stores ALL definitions (exported and private) for proper scoped resolution
    pub fn register_import_spice_model(&mut self, def: SpiceModelDefinition) {
        let name_str = def.name.as_str().to_string();
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        self.hpm
            .last_mut()
            .unwrap()
            .insert(name_str.into(), Definition::SpiceModel(def));
    }

    /// Register an imported SPICE subcircuit definition (in HPM layer) (v0.2.2)
    ///
    /// v0.3.0: Native typed subcircuit support for PDK compact models
    /// Stores ALL definitions (exported and private) for proper scoped resolution
    pub fn register_import_subcircuit(&mut self, def: SubcircuitDefinition) {
        let name_str = def.name.as_str().to_string();
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        self.hpm
            .last_mut()
            .unwrap()
            .insert(name_str.into(), Definition::Subcircuit(def));
    }
}
