//! Lightweight placement-item handles for the compilation pipeline (v0.2.x)
//!
//! Every variant is a 4-byte type-safe arena index. No AST node is cloned or
//! boxed: the data stays in `AstArena` and is referenced via
//! `&arena.components[id]` during execution.

use hwc_parser::ast::arena::{
    ComponentId, ContactId, PlaneId, PourId, RegionId, RouteId, SpaceInstanceId, SubstrateId,
};

/// Reference handle to a placement item.
///
/// All variants carry a type-safe arena ID — including substrates and regions,
/// which are arena-allocated at parse time just like every other node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlacementItem {
    Substrate(SubstrateId),
    Component(ComponentId),
    Pour(PourId),
    Plane(PlaneId),
    Contact(ContactId),
    SpaceInstance(SpaceInstanceId),
    Route(RouteId),
    Region(RegionId),
}

/// A placement item plus its dense position in the collected placement list.
///
/// `item_index` is the item's own `0..N` slot, used directly as the node ID in
/// the dependency graph so topological sorting is pure integer work — no
/// string keys, no hash lookups in the placement hot path.
///
/// This type is `Copy` and 8 bytes wide: collecting 10,000 placement items
/// allocates nothing beyond the backing `Vec`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextualPlacementItem {
    pub item: PlacementItem,
    pub item_index: usize,
}
