use super::super::{error::SymbolError, layer::SymbolTable, Definition};
use hwc_diagnostics::DiagnosticCollector;
use hwc_parser::{
    ConstDecl, DeviceDecl, EnumDecl, FunctionDecl, MaterialDecl, ModuleDecl, ProfileDecl, SpaceDecl,
    StructDecl, TestDecl,
};

impl SymbolTable {
    /// Register a function declaration
    pub fn register_function(&mut self, collector: &DiagnosticCollector, def: FunctionDecl) {
        let name_str = def.name.name.as_str().to_string();

        if let Some(Definition::Function(existing)) = self.local.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                name_str.clone().into(),
                "function",
                (def.span.start, def.span.end),
                Some((
                    self.arena.function_defs[*existing].span.start,
                    self.arena.function_defs[*existing].span.end,
                )),
            ));
            return;
        }

        let id = self.arena.function_defs.push(def);
        self.local.insert(name_str.into(), Definition::Function(id));
    }

    /// Register a const declaration
    pub fn register_const(&mut self, collector: &DiagnosticCollector, def: ConstDecl) {
        let name_str = def.name.name.as_str().to_string();

        if let Some(Definition::Const(existing)) = self.local.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                name_str.clone().into(),
                "const",
                (def.span.start, def.span.end),
                Some((
                    self.arena.const_defs[*existing].span.start,
                    self.arena.const_defs[*existing].span.end,
                )),
            ));
            return;
        }

        let id = self.arena.const_defs.push(def);
        self.local.insert(name_str.into(), Definition::Const(id));
    }

    /// Register a struct declaration
    pub fn register_struct(&mut self, collector: &DiagnosticCollector, def: StructDecl) {
        let name_str = def.name.name.as_str().to_string();

        if let Some(Definition::Struct(existing)) = self.local.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                name_str.clone().into(),
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

    /// Register an enum declaration
    pub fn register_enum(&mut self, collector: &DiagnosticCollector, def: EnumDecl) {
        let name_str = def.name.name.as_str().to_string();

        if let Some(Definition::Enum(existing)) = self.local.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                name_str.clone().into(),
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

    /// Register a space declaration
    pub fn register_space(&mut self, collector: &DiagnosticCollector, def: SpaceDecl) {
        let name_str = def.name.name.as_str().to_string();

        if let Some(Definition::Space(existing)) = self.local.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                name_str.clone().into(),
                "space",
                (def.span.start, def.span.end),
                Some((
                    self.arena.space_defs[*existing].span.start,
                    self.arena.space_defs[*existing].span.end,
                )),
            ));
            return;
        }

        let id = self.arena.space_defs.push(def);
        self.local.insert(name_str.into(), Definition::Space(id));
    }

    /// Register a module declaration
    pub fn register_module(&mut self, collector: &DiagnosticCollector, def: ModuleDecl) {
        let name_str = def.name.name.as_str().to_string();

        if let Some(Definition::Module(existing)) = self.local.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                name_str.clone().into(),
                "module",
                (def.span.start, def.span.end),
                Some((
                    self.arena.module_defs[*existing].span.start,
                    self.arena.module_defs[*existing].span.end,
                )),
            ));
            return;
        }

        let id = self.arena.module_defs.push(def);
        self.local.insert(name_str.into(), Definition::Module(id));
    }

    /// Register a material declaration
    pub fn register_material(&mut self, collector: &DiagnosticCollector, def: MaterialDecl) {
        let name_str = def.name.name.as_str().to_string();

        if let Some(Definition::Material(existing)) = self.local.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                name_str.clone().into(),
                "material",
                (def.span.start, def.span.end),
                Some((
                    self.arena.material_defs[*existing].span.start,
                    self.arena.material_defs[*existing].span.end,
                )),
            ));
            return;
        }

        let id = self.arena.material_defs.push(def);
        self.local.insert(name_str.into(), Definition::Material(id));
    }

    /// Register a profile declaration
    pub fn register_profile(&mut self, collector: &DiagnosticCollector, def: ProfileDecl) {
        let name_str = def.name.name.as_str().to_string();

        if let Some(Definition::Profile(existing)) = self.local.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                name_str.clone().into(),
                "profile",
                (def.span.start, def.span.end),
                Some((
                    self.arena.profile_defs[*existing].span.start,
                    self.arena.profile_defs[*existing].span.end,
                )),
            ));
            return;
        }

        let id = self.arena.profile_defs.push(def);
        self.local.insert(name_str.into(), Definition::Profile(id));
    }

    /// Register a device declaration
    pub fn register_device(&mut self, collector: &DiagnosticCollector, def: DeviceDecl) {
        let name_str = def.name.name.as_str().to_string();

        if let Some(Definition::Device(existing)) = self.local.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                name_str.clone().into(),
                "device",
                (def.span.start, def.span.end),
                Some((
                    self.arena.device_defs[*existing].span.start,
                    self.arena.device_defs[*existing].span.end,
                )),
            ));
            return;
        }

        let id = self.arena.device_defs.push(def);
        self.local.insert(name_str.into(), Definition::Device(id));
    }

    /// Register a test declaration
    pub fn register_test(&mut self, collector: &DiagnosticCollector, def: TestDecl) {
        let name_str = def.name.name.as_str().to_string();

        if let Some(Definition::Test(existing)) = self.local.get(name_str.as_str()) {
            collector.report(SymbolError::duplicate(
                name_str.clone().into(),
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
}
