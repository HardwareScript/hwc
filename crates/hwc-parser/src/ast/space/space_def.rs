use super::elevation::RoutingConfig;
use super::layout::ModuleLayoutBlock;
use super::nets::NetDeclaration;
use super::placements::PolygonPlacement;
use super::region::RegionDefinition;
use super::routes::{Expose, Route};
use super::substrate::SubstratePlacement;
use crate::ast::arena::{ComponentId, ContactId, ForLoopId, PlaneId, PourId, RouteId, SpaceInstanceId};
use crate::ast::expression::Expression;
use crate::lexer::Span;
use compact_str::CompactString;
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
#[derive(Debug, Clone, PartialEq)]
pub struct SpaceDefinition {
    pub name: crate::ast::common::Identifier,
    pub is_exported: bool, // v0.2.0: Access control
    pub implements_module: Option<CompactString>,
    pub dimensions: Option<crate::ast::common::Dimensions>,
    pub resolution: Option<crate::ast::common::Measurement>,
    pub origin: Option<crate::ast::common::OriginPoint>,
    pub profile: Option<crate::ast::common::Identifier>,
    pub mechanical: Option<crate::ast::common::Identifier>,
    pub substrate: Option<SubstratePlacement>,
    pub render: Option<crate::ast::component::RenderBlock>,
    pub routing_config: Option<RoutingConfig>,
    pub statements: Vec<SpaceTopLevelStatement>,
    pub layouts: Vec<ModuleLayoutBlock>,
    pub routes: Vec<Route>,
    pub exposes: Vec<Expose>,
    pub nets: Vec<NetDeclaration>,
    pub regions: Vec<RegionDefinition>, // v0.2.0: Region declarations
    pub span: Span,
}

/// Top-level statement in a space block (v0.1.7)
#[derive(Debug, Clone, PartialEq)]
pub enum SpaceTopLevelStatement {
    Substrate(SubstratePlacement),
    Component(ComponentId),
    Pour(PourId),
    Plane(PlaneId),
    Polygon(PolygonPlacement),
    Contact(ContactId),
    SpaceInstance(SpaceInstanceId), // v0.2.1: Hierarchical space composition
    ForLoop(ForLoopId),
    Route(RouteId), // Arena-allocated for SoC-scale performance
    Expose(Expose),
    RouteNetPolicy(RouteNetPolicy),
    Region(RegionDefinition), // v0.2.0: Region declaration
    Let(LetBinding),          // v0.2.0: Local variable binding
    Const(ConstBinding),      // v0.2.1: Immutable constant binding
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpaceForLoop {
    pub variable: CompactString,
    pub start: usize,
    pub end: usize,
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SpaceStatement {
    Component(ComponentId),
    Pour(PourId),
    Plane(PlaneId),
    Contact(ContactId),
    SpaceInstance(SpaceInstanceId), // v0.2.1: Hierarchical space composition
    Route(RouteId), // Arena-allocated for SoC-scale performance
    ForLoop(Box<SpaceForLoop>),
    If(SpaceIfConditional), // v0.2.1: Compile-time conditional branching
    Let(LetBinding),              // v0.2.1: Loop-scoped let bindings
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
    pub fn regions_from_statements(&self) -> impl Iterator<Item = &RegionDefinition> + '_ {
        self.statements.iter().filter_map(|s| match s {
            SpaceTopLevelStatement::Region(r) => Some(r),
            _ => None,
        })
    }
}
