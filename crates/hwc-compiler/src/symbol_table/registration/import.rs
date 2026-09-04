use super::super::layer::SymbolTable;
use super::super::Definition;
use crate::module_resolver::ExportedItem;
use compact_str::CompactString;
use hwc_parser::{
    ConstDecl, DeviceDecl, EnumDecl, FunctionDecl, MaterialDecl, ModuleDecl, ProfileDecl,
    SpaceDecl, StructDecl, TestDecl,
};

impl SymbolTable {
    /// Register any strongly-typed `ExportedItem` directly into the symbol table
    pub fn register_exported_item(&mut self, item: &ExportedItem) {
        match item {
            ExportedItem::Function(f) => self.register_import_function(f.clone()),
            ExportedItem::Struct(s) => self.register_import_struct(s.clone()),
            ExportedItem::Enum(e) => self.register_import_enum(e.clone()),
            ExportedItem::Const(c) => self.register_import_const(c.clone()),
            ExportedItem::Space(sp) => self.register_import_space(sp.clone()),
            ExportedItem::Module(m) => self.register_import_module(m.clone()),
            ExportedItem::Material(m) => self.register_import_material(m.clone()),
            ExportedItem::Profile(p) => self.register_import_profile(p.clone()),
            ExportedItem::Device(d) => self.register_import_device(d.clone()),
            ExportedItem::Test(t) => self.register_import_test(t.clone()),
            ExportedItem::ReExport(def) => self.register_import_definition(*def),
        }
    }

    /// Register an imported definition (already present in arena) into the HPM layer
    pub fn register_import_definition(&mut self, def: Definition) {
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        let name: CompactString = match def {
            Definition::Function(id) => self.arena.function_defs[id].name.name.clone(),
            Definition::Struct(id) => self.arena.struct_defs[id].name.name.clone(),
            Definition::Enum(id) => self.arena.enum_defs[id].name.name.clone(),
            Definition::Const(id) => self.arena.const_defs[id].name.name.clone(),
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

    pub fn register_import_const(&mut self, def: ConstDecl) {
        let name = def.name.name.clone();
        if self.hpm.is_empty() {
            self.hpm.push(super::super::layer::SymbolLayer::new());
        }
        let id = self.arena.const_defs.push(def);
        self.hpm.last_mut().unwrap().insert(name, Definition::Const(id));
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
