//! Symbol resolution methods (Pass 2) - Arena-Based Architecture

use super::{definition::DefinitionExt, error::SymbolError, layer::SymbolTable, Definition};
use hwc_parser::{
    DeviceDecl, EnumDecl, FunctionDecl, MaterialDecl, ModuleDecl, ProfileDecl, SpaceDecl,
    StructDecl, TestDecl,
};

impl SymbolTable {
    /// Get a function declaration by name
    pub fn get_function(&self, name: &str) -> Result<&FunctionDecl, SymbolError> {
        match self.get_symbol(name) {
            Some(Definition::Function(id)) => Ok(&self.arena.function_defs[id]),
            Some(other) => Err(SymbolError::type_mismatch(name, "function", other.kind_str())),
            None => Err(SymbolError::undefined(name.into(), "function", None)),
        }
    }

    /// Get a struct declaration by name
    pub fn get_struct(&self, name: &str) -> Result<&StructDecl, SymbolError> {
        match self.get_symbol(name) {
            Some(Definition::Struct(id)) => Ok(&self.arena.struct_defs[id]),
            Some(other) => Err(SymbolError::type_mismatch(name, "struct", other.kind_str())),
            None => Err(SymbolError::undefined(name.into(), "struct", None)),
        }
    }

    /// Get an enum declaration by name
    pub fn get_enum(&self, name: &str) -> Result<&EnumDecl, SymbolError> {
        match self.get_symbol(name) {
            Some(Definition::Enum(id)) => Ok(&self.arena.enum_defs[id]),
            Some(other) => Err(SymbolError::type_mismatch(name, "enum", other.kind_str())),
            None => Err(SymbolError::undefined(name.into(), "enum", None)),
        }
    }

    /// Get a space declaration by name
    pub fn get_space(&self, name: &str) -> Result<&SpaceDecl, SymbolError> {
        match self.get_symbol(name) {
            Some(Definition::Space(id)) => Ok(&self.arena.space_defs[id]),
            Some(other) => Err(SymbolError::type_mismatch(name, "space", other.kind_str())),
            None => Err(SymbolError::undefined(name.into(), "space", None)),
        }
    }

    /// Get a module declaration by name
    pub fn get_module(&self, name: &str) -> Result<&ModuleDecl, SymbolError> {
        match self.get_symbol(name) {
            Some(Definition::Module(id)) => Ok(&self.arena.module_defs[id]),
            Some(other) => Err(SymbolError::type_mismatch(name, "module", other.kind_str())),
            None => Err(SymbolError::undefined(name.into(), "module", None)),
        }
    }

    /// Get a material declaration by name
    pub fn get_material(&self, name: &str) -> Result<&MaterialDecl, SymbolError> {
        match self.get_symbol(name) {
            Some(Definition::Material(id)) => Ok(&self.arena.material_defs[id]),
            Some(other) => Err(SymbolError::type_mismatch(
                name,
                "material",
                other.kind_str(),
            )),
            None => Err(SymbolError::undefined(name.into(), "material", None)),
        }
    }

    /// Get a profile declaration by name
    pub fn get_profile(&self, name: &str) -> Result<&ProfileDecl, SymbolError> {
        match self.get_symbol(name) {
            Some(Definition::Profile(id)) => Ok(&self.arena.profile_defs[id]),
            Some(other) => Err(SymbolError::type_mismatch(
                name,
                "profile",
                other.kind_str(),
            )),
            None => Err(SymbolError::undefined(name.into(), "profile", None)),
        }
    }

    /// Get a device declaration by name
    pub fn get_device(&self, name: &str) -> Result<&DeviceDecl, SymbolError> {
        match self.get_symbol(name) {
            Some(Definition::Device(id)) => Ok(&self.arena.device_defs[id]),
            Some(other) => Err(SymbolError::type_mismatch(name, "device", other.kind_str())),
            None => Err(SymbolError::undefined(name.into(), "device", None)),
        }
    }

    /// Get a test declaration by name
    pub fn get_test(&self, name: &str) -> Result<&TestDecl, SymbolError> {
        match self.get_symbol(name) {
            Some(Definition::Test(id)) => Ok(&self.arena.test_defs[id]),
            Some(other) => Err(SymbolError::type_mismatch(name, "test", other.kind_str())),
            None => Err(SymbolError::undefined(name.into(), "test", None)),
        }
    }

    /// Find a test declaration for a given space by target name
    pub fn find_test_for_space(&self, space_name: &str) -> Option<&TestDecl> {
        self.arena.test_defs.iter().find(|t| t.target.as_str() == space_name)
    }

    /// Iterate over all materials
    pub fn materials(&self) -> impl Iterator<Item = (&compact_str::CompactString, &MaterialDecl)> {
        self.arena.material_defs.iter().map(|m| (&m.name.name, m))
    }

    /// Iterate over all profiles
    pub fn profiles(&self) -> impl Iterator<Item = (&compact_str::CompactString, &ProfileDecl)> {
        self.arena.profile_defs.iter().map(|p| (&p.name.name, p))
    }

    /// Iterate over all devices
    pub fn iter_all_devices(&self) -> impl Iterator<Item = (&compact_str::CompactString, &DeviceDecl)> {
        self.arena.device_defs.iter().map(|d| (&d.name.name, d))
    }

    /// Iterate over all modules
    pub fn modules(&self) -> impl Iterator<Item = (&compact_str::CompactString, &ModuleDecl)> {
        self.arena.module_defs.iter().map(|m| (&m.name.name, m))
    }

    /// Legacy / compatibility: get all constants (empty in v0.3.0 since constants are evaluated in Comptime Engine)
    pub fn get_all_constants(&self) -> rustc_hash::FxHashMap<compact_str::CompactString, f64> {
        rustc_hash::FxHashMap::default()
    }

    /// Legacy / compatibility: get all bridges
    pub fn get_all_bridges(&self) -> Vec<&DeviceDecl> {
        self.arena.device_defs.iter().collect()
    }

    /// Legacy / compatibility: get component (now devices / modules)
    pub fn get_component(&self, name: &str) -> Result<&DeviceDecl, SymbolError> {
        self.get_device(name)
    }

    /// Legacy / compatibility: get shape
    pub fn get_shape(&self, name: &str) -> Option<&SpaceDecl> {
        self.get_space(name).ok()
    }

    /// Legacy / compatibility: get pattern
    pub fn get_pattern(&self, name: &str) -> Result<&ModuleDecl, SymbolError> {
        self.get_module(name)
    }

    /// Legacy / compatibility: measurement_to_nm
    pub fn measurement_to_nm(&self, measurement: &hwc_parser::Measurement) -> Result<i64, SymbolError> {
        measurement.to_nanometers_i64().ok_or_else(|| {
            SymbolError::undefined("unit".into(), "length unit", None)
        })
    }

    /// Legacy / compatibility: resolve_unit_symbol
    pub fn resolve_unit_symbol(&self, _symbol: &str) -> Option<hwc_parser::Unit> {
        None
    }
}
