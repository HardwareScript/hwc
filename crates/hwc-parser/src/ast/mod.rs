//! Abstract Syntax Tree for HardwareScript v0.3.0

use serde::{Deserialize, Serialize};

pub mod arena;
mod common;
pub mod declarations;
pub mod device;
pub mod device_contract;
pub mod expression;
pub mod material;
pub mod profile;
pub mod statement;
pub mod subcircuit;
pub mod test;
mod unit;

// Re-export public AST types
pub use arena::AstArena;
pub use common::*;
pub use declarations::*;
pub use expression::*;
pub use material::{ManufacturingProcess, MaterialCategory};
pub use profile::{
    BridgeRule, ClearanceConstraints, CostWeights, ExportConstraints, LayerConstraints,
    LayerStackup, ManufacturingConstraints, ProfileIntent, RoutingConstraints,
    RoutingDirection, RoutableMode, StackupLayer as ProfileStackupLayer, ThermalConstraints,
    TraceConstraints, ViaConstraints, ViaDefinition,
};
pub use statement::*;
pub use unit::*;

// Re-export Span from lexer for use in AST
pub use crate::lexer::Span;

// Re-export device types
pub use device::{
    DeviceDefinition, DeviceInstanceDeclaration, ManifoldExpr, MetricExpression,
    SpiceExportInfo, SpiceParameterStyle,
};

// Re-export device contract
pub use device_contract::{
    DeviceContract, MaterialConstraint, GeometricConstraint, ConnectivityConstraint,
    ExtractionRule,
};

// Re-export subcircuit types
pub use subcircuit::{SubcircuitDefinition, SubcircuitElement, SubcircuitParameter, Node};

// Re-export test/simulation types
pub use test::{
    TestDefinition, SimulationDirective, DcAnalysis, DcSweep, AcAnalysis, TranAnalysis,
    NoiseAnalysis, OpAnalysis, GenericAnalysis, SweepScale, SweepTarget, DirectiveValue,
    TestAction, TestActionType, TestAssertion, TestCondition,
};

// NetClassification - derived from string classification in NetDecl
/// Net classification enum for SPICE stimulus generation
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NetClassification {
    Power,
    Ground,
    Signal,
    HighVoltage,
    Unclassified,
}

impl NetClassification {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "power" | "vdd" | "vcc" | "supply" => Self::Power,
            "ground" | "gnd" | "vss" => Self::Ground,
            "signal" => Self::Signal,
            "high_voltage" | "highvoltage" | "hv" => Self::HighVoltage,
            _ => Self::Unclassified,
        }
    }
}

/// Root AST node representing a complete HardwareScript v0.3.0 file
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Program {
    pub imports: Vec<ImportDecl>,
    pub items: Vec<TopLevelItem>,
    pub span: Span,
}

/// Top-level item in a HardwareScript program
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TopLevelItem {
    Function(FunctionDecl),
    Struct(StructDecl),
    Enum(EnumDecl),
    Const(ConstDecl),
    Export(ExportDecl),
    Space(SpaceDecl),
    Module(ModuleDecl),
    Material(MaterialDecl),
    Profile(ProfileDecl),
    Device(DeviceDecl),
    Test(TestDecl),
    Statement(Statement),
}

