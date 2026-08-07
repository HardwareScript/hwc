use super::elevation::RoutingConfig;
use super::layout::ModuleLayoutBlock;
use super::nets::NetDeclaration;
use super::placements::{ContactPlacement, PlanePlacement, PolygonPlacement, PourPlacement};
use super::region::RegionDefinition;
use super::routes::{Expose, Route};
use super::substrate::SubstratePlacement;
use crate::ast::component::ComponentPlacement;
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SpaceTopLevelStatement {
    Substrate(SubstratePlacement),
    Component(Box<ComponentPlacement>),
    Pour(Box<PourPlacement>),
    Plane(Box<PlanePlacement>),
    Polygon(PolygonPlacement),
    Contact(ContactPlacement),
    SpaceInstance(Box<super::placements::SpaceInstancePlacement>), // v0.2.1: Hierarchical space composition
    ForLoop(SpaceForLoop),
    Route(Route),
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
    Component(Box<ComponentPlacement>),
    Pour(Box<PourPlacement>),
    Plane(Box<PlanePlacement>),
    Contact(ContactPlacement),
    SpaceInstance(Box<super::placements::SpaceInstancePlacement>), // v0.2.1: Hierarchical space composition
    Route(Route),
    ForLoop(Box<SpaceForLoop>),
    If(SpaceIfConditional), // v0.2.1: Compile-time conditional branching
    Let(LetBinding),        // v0.2.1: Loop-scoped let bindings
}

impl SpaceDefinition {
    pub fn components(&self) -> Vec<ComponentPlacement> {
        self.statements
            .iter()
            .filter_map(|s| {
                if let SpaceTopLevelStatement::Component(c) = s {
                    Some((**c).clone())
                } else {
                    None
                }
            })
            .collect()
    }
    pub fn pours(&self) -> Vec<PourPlacement> {
        self.statements
            .iter()
            .filter_map(|s| {
                if let SpaceTopLevelStatement::Pour(p) = s {
                    Some((**p).clone())
                } else {
                    None
                }
            })
            .collect()
    }
    pub fn planes(&self) -> Vec<PlanePlacement> {
        self.statements
            .iter()
            .filter_map(|s| {
                if let SpaceTopLevelStatement::Plane(p) = s {
                    Some((**p).clone())
                } else {
                    None
                }
            })
            .collect()
    }
    pub fn polygons(&self) -> Vec<PolygonPlacement> {
        self.statements
            .iter()
            .filter_map(|s| {
                if let SpaceTopLevelStatement::Polygon(p) = s {
                    Some(p.clone())
                } else {
                    None
                }
            })
            .collect()
    }
    pub fn contacts(&self) -> Vec<ContactPlacement> {
        self.statements
            .iter()
            .filter_map(|s| {
                if let SpaceTopLevelStatement::Contact(c) = s {
                    Some(c.clone())
                } else {
                    None
                }
            })
            .collect()
    }
    pub fn for_loops(&self) -> Vec<SpaceForLoop> {
        self.statements
            .iter()
            .filter_map(|s| {
                if let SpaceTopLevelStatement::ForLoop(f) = s {
                    Some(f.clone())
                } else {
                    None
                }
            })
            .collect()
    }
    pub fn route_net_policies(&self) -> Vec<RouteNetPolicy> {
        self.statements
            .iter()
            .filter_map(|s| {
                if let SpaceTopLevelStatement::RouteNetPolicy(p) = s {
                    Some(p.clone())
                } else {
                    None
                }
            })
            .collect()
    }
    pub fn regions_from_statements(&self) -> Vec<RegionDefinition> {
        self.statements
            .iter()
            .filter_map(|s| {
                if let SpaceTopLevelStatement::Region(r) = s {
                    Some(r.clone())
                } else {
                    None
                }
            })
            .collect()
    }
}
