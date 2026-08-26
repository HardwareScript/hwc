//! HardwareScript v0.3.0 Top-Level Declarations AST nodes

use super::common::Identifier;
use super::expression::Expression;
use super::statement::{Block, Statement, TypeExpr};
use super::Span;
use compact_str::CompactString;
use serde::{Deserialize, Serialize};

/// Top-level function declaration: `(export)? fn name(params) -> ReturnType { ... }`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionDecl {
    pub is_exported: bool,
    pub name: Identifier,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<TypeExpr>,
    pub body: Block,
    pub span: Span,
}

/// Function parameter: `name: Type (= default)?`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parameter {
    pub name: CompactString,
    pub type_annotation: TypeExpr,
    pub default_value: Option<Expression>,
    pub span: Span,
}

/// Struct declaration: `(export)? struct Name { field: Type, ... }`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructDecl {
    pub is_exported: bool,
    pub name: Identifier,
    pub fields: Vec<StructFieldDecl>,
    pub span: Span,
}

/// Struct field: `name: Type`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructFieldDecl {
    pub name: CompactString,
    pub type_annotation: TypeExpr,
    pub span: Span,
}

/// Enum declaration: `(export)? enum Name { Variant1, Variant2(Type), ... }`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnumDecl {
    pub is_exported: bool,
    pub name: Identifier,
    pub variants: Vec<EnumVariantDecl>,
    pub span: Span,
}

/// Enum variant declaration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnumVariantDecl {
    pub name: CompactString,
    pub payload: Option<EnumVariantPayload>,
    pub span: Span,
}

/// Enum variant payload: tuple `Variant(Type1, Type2)` or struct `Variant { field: Type }`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EnumVariantPayload {
    Tuple(Vec<TypeExpr>),
    Struct(Vec<StructFieldDecl>),
}

/// Space declaration: `space Name (implements Interface)? { ... }`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpaceDecl {
    pub name: Identifier,
    pub implements: Option<Identifier>,
    pub dimensions: Option<(Expression, Expression)>,
    pub profile: Option<Identifier>,
    pub nets: Vec<NetDecl>,
    pub statements: Vec<Statement>,
    pub span: Span,
}

/// Net definition item in `nets { Name: { classification: power, ... } }`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetDecl {
    pub name: CompactString,
    pub properties: Vec<(CompactString, Expression)>,
    pub span: Span,
}

/// Type alias for NetDecl
pub type NetDeclaration = NetDecl;

impl NetDecl {
    pub fn get_property(&self, key: &str) -> Option<&Expression> {
        self.properties.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn potential(&self) -> Option<&Expression> {
        self.get_property("potential").or_else(|| self.get_property("voltage"))
    }

    pub fn classification(&self) -> Option<CompactString> {
        if let Some(Expression::Variable { name, .. }) = self.get_property("classification") {
            Some(name.clone())
        } else if let Some(Expression::StringLiteral { value, .. }) = self.get_property("classification") {
            Some(value.as_str().into())
        } else {
            None
        }
    }
}

/// Module declaration: `module Name { pins: [...], ... }`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleDecl {
    pub name: Identifier,
    pub pins: Vec<PinDecl>,
    pub routes: Vec<Statement>,
    pub span: Span,
}

/// Pin declaration: `(input|output|inout|power|ground)? Identifier`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PinDecl {
    pub direction: Option<CompactString>,
    pub name: CompactString,
    pub span: Span,
}

/// Material declaration: `(export)? material Name { prop: value, ... }`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialDecl {
    pub is_exported: bool,
    pub name: Identifier,
    pub properties: Vec<(CompactString, Expression)>,
    pub span: Span,
}

/// Type alias for MaterialDecl
pub type MaterialDefinition = MaterialDecl;

/// Type alias for ProfileDecl
pub type ProfileDefinition = ProfileDecl;

/// Type alias for SpaceDecl
pub type SpaceDefinition = SpaceDecl;


/// Profile declaration: `(export)? profile Name { section { field: value } }`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileDecl {
    pub is_exported: bool,
    pub name: Identifier,
    pub sections: Vec<ProfileSection>,
    pub span: Span,
}

use crate::ast::material::{ManufacturingProcess, MaterialCategory};

impl MaterialDecl {
    pub fn get_property(&self, key: &str) -> Option<&Expression> {
        self.properties.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn category(&self) -> MaterialCategory {
        let cat_str = if let Some(Expression::Variable { name, .. }) = self.get_property("category") {
            Some(name.as_str())
        } else if let Some(Expression::StringLiteral { value, .. }) = self.get_property("category") {
            Some(value.as_str())
        } else {
            None
        };

        if let Some(cat) = cat_str {
            match cat.to_lowercase().as_str() {
                "conductor" => MaterialCategory::Conductor,
                "insulator" | "dielectric" => MaterialCategory::Insulator,
                "semiconductor" => MaterialCategory::Semiconductor,
                "ohmic_contact" | "ohmiccontact" => MaterialCategory::OhmicContact,
                "die_interconnect" | "dieinterconnect" => MaterialCategory::DieInterconnect,
                "pcb_solder" | "pcbsolder" => MaterialCategory::PcbSolder,
                "barrier_layer" | "barrierlayer" => MaterialCategory::BarrierLayer,
                "adhesive" => MaterialCategory::Adhesive,
                "mask" => MaterialCategory::Mask,
                _ => MaterialCategory::Conductor,
            }
        } else {
            MaterialCategory::Conductor
        }
    }

    pub fn get_process(&self) -> ManufacturingProcess {
        let proc_str = if let Some(Expression::Variable { name, .. }) = self.get_property("process") {
            Some(name.as_str())
        } else if let Some(Expression::StringLiteral { value, .. }) = self.get_property("process") {
            Some(value.as_str())
        } else {
            None
        };

        if let Some(proc) = proc_str {
            match proc.to_lowercase().as_str() {
                "drilled_plated" | "drilledplated" => ManufacturingProcess::DrilledPlated,
                "etched" => ManufacturingProcess::Etched,
                _ => ManufacturingProcess::Deposited,
            }
        } else {
            ManufacturingProcess::Deposited
        }
    }

    pub fn symbol(&self) -> Option<CompactString> {
        if let Some(Expression::StringLiteral { value, .. }) = self.get_property("symbol") {
            Some(value.as_str().into())
        } else if let Some(Expression::Variable { name, .. }) = self.get_property("symbol") {
            Some(name.clone())
        } else {
            None
        }
    }

    pub fn description(&self) -> Option<CompactString> {
        if let Some(Expression::StringLiteral { value, .. }) = self.get_property("description") {
            Some(value.as_str().into())
        } else {
            None
        }
    }

    pub fn get_color(&self) -> Option<CompactString> {
        if let Some(Expression::StringLiteral { value, .. }) = self.get_property("color") {
            Some(value.as_str().into())
        } else if let Some(Expression::Variable { name, .. }) = self.get_property("color") {
            Some(name.clone())
        } else {
            None
        }
    }

    pub fn get_opacity(&self) -> f64 {
        if let Some(Expression::Measurement { value, .. }) = self.get_property("opacity") {
            *value
        } else if let Some(Expression::Literal { value, .. }) = self.get_property("opacity") {
            *value as f64
        } else {
            match self.category() {
                MaterialCategory::Insulator | MaterialCategory::Mask => 0.8,
                _ => 1.0,
            }
        }
    }

    pub fn get_outline_opacity(&self) -> f64 {
        if let Some(Expression::Measurement { value, .. }) = self.get_property("outline_opacity") {
            *value
        } else {
            1.0
        }
    }

    pub fn get_roughness(&self) -> f64 {
        if let Some(Expression::Measurement { value, .. }) = self.get_property("roughness") {
            *value
        } else {
            0.5
        }
    }

    pub fn get_metallic(&self) -> f64 {
        if let Some(Expression::Measurement { value, .. }) = self.get_property("metallic") {
            *value
        } else {
            match self.category() {
                MaterialCategory::Conductor => 0.9,
                _ => 0.1,
            }
        }
    }

    pub fn get_ior(&self) -> f64 {
        if let Some(Expression::Measurement { value, .. }) = self.get_property("ior") {
            *value
        } else {
            1.5
        }
    }

    pub fn get_clearcoat(&self) -> f64 {
        if let Some(Expression::Measurement { value, .. }) = self.get_property("clearcoat") {
            *value
        } else {
            0.0
        }
    }

    pub fn get_clearcoat_roughness(&self) -> f64 {
        if let Some(Expression::Measurement { value, .. }) = self.get_property("clearcoat_roughness") {
            *value
        } else {
            0.1
        }
    }

    pub fn get_subsurface(&self) -> f64 {
        if let Some(Expression::Measurement { value, .. }) = self.get_property("subsurface") {
            *value
        } else {
            0.0
        }
    }

    pub fn get_anisotropy(&self) -> f64 {
        if let Some(Expression::Measurement { value, .. }) = self.get_property("anisotropy") {
            *value
        } else {
            0.0
        }
    }

    pub fn get_anisotropy_rotation(&self) -> f64 {
        if let Some(Expression::Measurement { value, .. }) = self.get_property("anisotropy_rotation") {
            *value
        } else {
            0.0
        }
    }

    pub fn get_texture(&self) -> Option<CompactString> {
        if let Some(Expression::StringLiteral { value, .. }) = self.get_property("texture") {
            Some(value.as_str().into())
        } else {
            None
        }
    }
}

impl ProfileDecl {
    pub fn get_section(&self, section_type: &str) -> Option<&ProfileSection> {
        self.sections.iter().find(|s| s.section_type == section_type)
    }

    pub fn get_property(&self, section_type: &str, key: &str) -> Option<&Expression> {
        self.get_section(section_type)
            .and_then(|s| s.fields.iter().find(|(k, _)| k == key).map(|(_, v)| v))
    }

    pub fn is_asic(&self) -> bool {
        if let Some(Expression::Variable { name, .. }) = self.get_property("target", "type") {
            name.eq_ignore_ascii_case("asic")
        } else if let Some(Expression::StringLiteral { value, .. }) = self.get_property("target", "type") {
            value.eq_ignore_ascii_case("asic")
        } else {
            true
        }
    }

    pub fn substrate_net(&self) -> Option<CompactString> {
        if let Some(Expression::Variable { name, .. }) = self.get_property("substrate", "net") {
            Some(name.clone())
        } else if let Some(Expression::StringLiteral { value, .. }) = self.get_property("substrate", "net") {
            Some(value.as_str().into())
        } else if let Some(Expression::Variable { name, .. }) = self.get_property("target", "substrate_net") {
            Some(name.clone())
        } else if let Some(Expression::StringLiteral { value, .. }) = self.get_property("target", "substrate_net") {
            Some(value.as_str().into())
        } else {
            Some("BULK".into())
        }
    }
}

/// Profile section
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileSection {
    pub section_type: CompactString,
    pub name: Option<CompactString>,
    pub fields: Vec<(CompactString, Expression)>,
    pub span: Span,
}

/// Device declaration: `(export)? device Name { ... }`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceDecl {
    pub is_exported: bool,
    pub name: Identifier,
    pub sections: Vec<DeviceSection>,
    pub span: Span,
}

/// Device section or property
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceSection {
    pub name: CompactString,
    pub fields: Vec<(CompactString, Expression)>,
    pub span: Span,
}

/// Test declaration: `test Name for Target { dc: { ... }, tran: { ... } }`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestDecl {
    pub name: Identifier,
    pub target: Identifier,
    pub configs: Vec<TestConfig>,
    pub span: Span,
}

/// Test configuration section
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestConfig {
    pub name: CompactString,
    pub params: Vec<(CompactString, Expression)>,
    pub span: Span,
}

/// Import declaration: `import { a, b } from "module"` or `import * from @std/primitives/units`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportDecl {
    pub symbols: ImportSymbols,
    pub from: String,
    pub span: Span,
}

/// Imported symbols
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ImportSymbols {
    All,                          // *
    Named(Vec<CompactString>),    // { a, b, c }
    Single(CompactString),        // a
}
