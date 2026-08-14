use super::elevation::RoutingConfig;
use super::layout::ModuleLayoutBlock;
use super::nets::NetDeclaration;
use super::routes::{Expose, Route};
use crate::ast::arena::{
    ComponentId, ContactId, ForLoopId, PlaneId, PolygonId, PourId, RegionId, RouteId,
    SpaceInstanceId, SubstrateId,
};
use crate::ast::expression::Expression;
use crate::lexer::Span;
use compact_str::CompactString;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

/// Local variable binding in space block (v0.2.0)
/// Example: `let edge_pad_w = 150um`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LetBinding {
    pub name: CompactString,
    pub value: Expression,
    pub span: Span,
}

/// Immutable constant binding in space block (v0.2.1)
/// Example: `const PI: 3.14159`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstBinding {
    pub name: CompactString,
    pub value: Expression,
    pub span: Span,
}

/// Space definition: `space Name:` (v0.1.6)
/// v0.2.0: Supports optional `export` keyword for visibility control
/// v0.2.1: Supports `device_nets` for explicit virtual terminal binding
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpaceDefinition {
    pub name: crate::ast::common::Identifier,
    pub is_exported: bool, // v0.2.0: Access control
    pub implements_module: Option<CompactString>,
    pub dimensions: Option<crate::ast::common::Dimensions>,
    pub profile: Option<crate::ast::common::Identifier>,
    pub mechanical: Option<crate::ast::common::Identifier>,
    pub substrate: Option<SubstrateId>,
    pub render: Option<crate::ast::component::RenderBlock>,
    pub routing_config: Option<RoutingConfig>,
    pub statements: Vec<SpaceTopLevelStatement>,
    pub layouts: Vec<ModuleLayoutBlock>,
    pub routes: Vec<Route>,
    pub exposes: Vec<Expose>,
    pub nets: Vec<NetDeclaration>,
    pub regions: Vec<RegionId>, // v0.2.0: Region declarations (arena-allocated)
    
    /// v0.2.1: Explicit device terminal net bindings for virtual terminals
    /// Maps device_name -> (terminal_name -> net_name)
    /// Example: device_nets R1: { BULK: GND }
    pub device_nets: FxHashMap<CompactString, FxHashMap<CompactString, CompactString>>,
    
    pub span: Span,
}

/// Top-level statement in a space block (v0.1.7)
/// 100% Pure Arena IDs - Every variant is uniformly 8 bytes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SpaceTopLevelStatement {
    Substrate(SubstrateId),
    Component(ComponentId),
    Pour(PourId),
    Plane(PlaneId),
    Polygon(PolygonId), // ✅ Arena-allocated for zero-copy uniformity
    Contact(ContactId),
    SpaceInstance(SpaceInstanceId), // v0.2.1: Hierarchical space composition
    ForLoop(ForLoopId),
    Route(RouteId), // Arena-allocated for SoC-scale performance
    Expose(Expose),
    RouteNetPolicy(RouteNetPolicy),
    Region(RegionId),    // v0.2.0: Region declaration (arena-allocated)
    Let(LetBinding),     // v0.2.0: Local variable binding
    Const(ConstBinding), // v0.2.1: Immutable constant binding
}

/// v0.1.8: Prescriptive net-scoped route policy
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteNetPolicy {
    pub net_id: crate::ast::common::Identifier,
    pub target_layer: Option<crate::ast::common::Identifier>,
    pub pattern: Option<crate::ast::pattern::PatternInstantiation>,
    pub strategy: Option<crate::ast::common::Identifier>,
    pub span: Span,
}

/// For loop in space block (Sprint 3.4: Parametric Unrolling)
///
/// Range Semantics (Rust/Swift-style explicit):
/// - `0..3` (exclusive): Iterates 3 times [0, 1, 2] - count-driven
/// - `0..=3` (inclusive): Iterates 4 times [0, 1, 2, 3] - bound-driven
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpaceForLoop {
    pub variable: CompactString,
    pub start: usize,
    pub end: usize,
    pub inclusive: bool,            // true for ..= (inclusive), false for .. (exclusive)
    pub body: Vec<SpaceStatement>,
    pub span: Span,
}

/// Compile-time conditional in space block (v0.2.1: Generator Conditions)
/// This is NOT runtime control flow - it's compile-time code generation branching
/// Example: if (row + col) mod 2 == 0: add Aluminum else: add Tungsten
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpaceIfConditional {
    pub condition: Expression,
    pub then_body: Vec<SpaceStatement>,
    pub else_body: Vec<SpaceStatement>,
    pub span: Span,
}

/// Statement inside a space for loop
/// 100% Pure Arena IDs - Zero Box<T>!
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SpaceStatement {
    Component(ComponentId),
    Pour(PourId),
    Plane(PlaneId),
    Polygon(PolygonId), // ✅ Arena-allocated
    Contact(ContactId),
    SpaceInstance(SpaceInstanceId), // v0.2.1: Hierarchical space composition
    Route(RouteId),                 // Arena-allocated for SoC-scale performance
    ForLoop(ForLoopId),             // ✅ Pure Arena ID - Box<T> eliminated!
    If(SpaceIfConditional),         // v0.2.1: Compile-time conditional branching
    Let(LetBinding),                // v0.2.1: Loop-scoped let bindings
}

impl SpaceDefinition {
    pub fn component_ids(&self) -> impl Iterator<Item = ComponentId> + '_ {
        self.statements.iter().filter_map(|s| match s {
            SpaceTopLevelStatement::Component(id) => Some(*id),
            _ => None,
        })
    }
    pub fn pour_ids(&self) -> impl Iterator<Item = PourId> + '_ {
        self.statements.iter().filter_map(|s| match s {
            SpaceTopLevelStatement::Pour(id) => Some(*id),
            _ => None,
        })
    }
    pub fn plane_ids(&self) -> impl Iterator<Item = PlaneId> + '_ {
        self.statements.iter().filter_map(|s| match s {
            SpaceTopLevelStatement::Plane(id) => Some(*id),
            _ => None,
        })
    }
    pub fn polygon_ids(&self) -> impl Iterator<Item = PolygonId> + '_ {
        self.statements.iter().filter_map(|s| match s {
            SpaceTopLevelStatement::Polygon(id) => Some(*id),
            _ => None,
        })
    }
    pub fn contact_ids(&self) -> impl Iterator<Item = ContactId> + '_ {
        self.statements.iter().filter_map(|s| match s {
            SpaceTopLevelStatement::Contact(id) => Some(*id),
            _ => None,
        })
    }
    pub fn for_loop_ids(&self) -> impl Iterator<Item = ForLoopId> + '_ {
        self.statements.iter().filter_map(|s| match s {
            SpaceTopLevelStatement::ForLoop(id) => Some(*id),
            _ => None,
        })
    }
    pub fn route_net_policies(&self) -> impl Iterator<Item = &RouteNetPolicy> + '_ {
        self.statements.iter().filter_map(|s| match s {
            SpaceTopLevelStatement::RouteNetPolicy(p) => Some(p),
            _ => None,
        })
    }
    pub fn region_ids(&self) -> impl Iterator<Item = RegionId> + '_ {
        self.statements.iter().filter_map(|s| match s {
            SpaceTopLevelStatement::Region(id) => Some(*id),
            _ => None,
        })
    }
    pub fn substrate_ids(&self) -> impl Iterator<Item = SubstrateId> + '_ {
        self.statements.iter().filter_map(|s| match s {
            SpaceTopLevelStatement::Substrate(id) => Some(*id),
            _ => None,
        })
    }
}
