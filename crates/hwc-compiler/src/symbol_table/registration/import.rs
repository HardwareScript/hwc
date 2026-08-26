use super::super::layer::SymbolTable;
use super::super::Definition;
use compact_str::CompactString;
use hwc_parser::{
    DeviceDecl, EnumDecl, FunctionDecl, MaterialDecl, ModuleDecl, ProfileDecl, SpaceDecl,
    StructDecl, TestDecl,
};

impl SymbolTable {
    /// Register an imported definition (already present in arena) into the HPM layer
    pub fn register_import_definition(&mut self, def: Definition) {
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        let name: CompactString = match def {
            Definition::Function(id) => self.arena.function_defs[id].name.name.clone(),
            Definition::Struct(id) => self.arena.struct_defs[id].name.name.clone(),
            Definition::Enum(id) => self.arena.enum_defs[id].name.name.clone(),
            Definition::Space(id) => self.arena.space_defs[id].name.name.clone(),
            Definition::Module(id) => self.arena.module_defs[id].name.name.clone(),
            Definition::Material(id) => self.arena.material_defs[id].name.name.clone(),
            Definition::Profile(id) => self.arena.profile_defs[id].name.name.clone(),
            Definition::Device(id) => self.arena.device_defs[id].name.name.clone(),
            Definition::Test(id) => self.arena.test_defs[id].name.name.clone(),
        };
        self.hpm.last_mut().unwrap().insert(name, def);
    }

    pub fn register_import_function(&mut self, def: FunctionDecl) {
        let name = def.name.name.clone();
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        let id = self.arena.function_defs.push(def);
        self.hpm.last_mut().unwrap().insert(name, Definition::Function(id));
    }

    pub fn register_import_struct(&mut self, def: StructDecl) {
        let name = def.name.name.clone();
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        let id = self.arena.struct_defs.push(def);
        self.hpm.last_mut().unwrap().insert(name, Definition::Struct(id));
    }

    pub fn register_import_enum(&mut self, def: EnumDecl) {
        let name = def.name.name.clone();
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        let id = self.arena.enum_defs.push(def);
        self.hpm.last_mut().unwrap().insert(name, Definition::Enum(id));
    }

    pub fn register_import_space(&mut self, def: SpaceDecl) {
        let name = def.name.name.clone();
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        let id = self.arena.space_defs.push(def);
        self.hpm.last_mut().unwrap().insert(name, Definition::Space(id));
    }

    pub fn register_import_module(&mut self, def: ModuleDecl) {
        let name = def.name.name.clone();
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        let id = self.arena.module_defs.push(def);
        self.hpm.last_mut().unwrap().insert(name, Definition::Module(id));
    }

    pub fn register_import_material(&mut self, def: MaterialDecl) {
        let name = def.name.name.clone();
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        let id = self.arena.material_defs.push(def);
        self.hpm.last_mut().unwrap().insert(name, Definition::Material(id));
    }

    pub fn register_import_profile(&mut self, def: ProfileDecl) {
        let name = def.name.name.clone();
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        let id = self.arena.profile_defs.push(def);
        self.hpm.last_mut().unwrap().insert(name, Definition::Profile(id));
    }

    pub fn register_import_device(&mut self, def: DeviceDecl) {
        let name = def.name.name.clone();
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        let id = self.arena.device_defs.push(def);
        self.hpm.last_mut().unwrap().insert(name, Definition::Device(id));
    }

    pub fn register_import_test(&mut self, def: TestDecl) {
        let name = def.name.name.clone();
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        let id = self.arena.test_defs.push(def);
        self.hpm.last_mut().unwrap().insert(name, Definition::Test(id));
    }
}
