//! Arena-based Definition References for Symbol Table

use hwc_parser::ast::arena::*;
use hwc_parser::ast::AstArena;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Definition {
    Function(FunctionDefId),
    Struct(StructDefId),
    Enum(EnumDefId),
    Space(SpaceDefId),
    Module(ModuleDefId),
    Material(MaterialDefId),
    Profile(ProfileDefId),
    Device(DeviceDefId),
    Test(TestDefId),
}

pub trait DefinitionExt {
    fn kind_str(&self) -> &'static str;
    fn is_exported(&self, arena: &AstArena) -> bool;
}

impl DefinitionExt for Definition {
    fn kind_str(&self) -> &'static str {
        match self {
            Definition::Function(_) => "function",
            Definition::Struct(_) => "struct",
            Definition::Enum(_) => "enum",
            Definition::Space(_) => "space",
            Definition::Module(_) => "module",
            Definition::Material(_) => "material",
            Definition::Profile(_) => "profile",
            Definition::Device(_) => "device",
            Definition::Test(_) => "test",
        }
    }

    fn is_exported(&self, arena: &AstArena) -> bool {
        match self {
            Definition::Function(id) => arena.function_defs[*id].is_exported,
            Definition::Struct(id) => arena.struct_defs[*id].is_exported,
            Definition::Enum(id) => arena.enum_defs[*id].is_exported,
            Definition::Space(_) => true,
            Definition::Module(_) => true,
            Definition::Material(id) => arena.material_defs[*id].is_exported,
            Definition::Profile(id) => arena.profile_defs[*id].is_exported,
            Definition::Device(id) => arena.device_defs[*id].is_exported,
            Definition::Test(_) => true,
        }
    }
}
