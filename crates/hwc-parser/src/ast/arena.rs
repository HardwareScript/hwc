//! Type-safe arena allocation using u32 indices (zero dependencies)
//!
//! This module provides a custom IndexVec implementation that enables:
//! - Compile-time type safety (can't mix ComponentId with RouteId)
//! - 4-byte indices (vs 8-byte pointers on 64-bit systems)
//! - Zero lifetimes (no 'ast pollution)
//! - Native thread safety (Copy + Send + Sync)
//! - Salsa compatibility ('static types)
//!
//! # Architecture
//!
//! All AST nodes are stored in contiguous Vec<T> arrays within AstArena.
//! References use lightweight u32 indices instead of pointers or lifetimes.
//!
//! # Example
//!
//! ```rust
//! use hwc_parser::ast::arena::*;
//!
//! let mut arena = AstArena::new();
//!
//! // Allocate component
//! let comp_id = arena.alloc_component(ComponentPlacement { /* ... */ });
//!
//! // Type-safe access
//! let component = &arena.components[comp_id];
//!
//! // This won't compile (type error):
//! // let route_id: RouteId = ...;
//! // let wrong = &arena.components[route_id];  // ERROR: expected ComponentId
//! ```

use serde::{Deserialize, Serialize};
use std::marker::PhantomData;
use std::ops::{Index, IndexMut};

// =============================================================================
// Core Idx Trait
// =============================================================================

/// Trait for types that can be used as indices in IndexVec
///
/// This trait is implemented by ID newtypes (ComponentId, RouteId, etc.)
/// to enable type-safe indexing.
pub trait Idx: Copy + Clone + PartialEq + Eq + std::hash::Hash {
    /// Create a new index from a usize
    fn new(idx: usize) -> Self;
    
    /// Convert this index to a usize for array access
    fn index(self) -> usize;
}

// =============================================================================
// Type-Safe IndexVec
// =============================================================================

/// Type-safe vector indexed by custom index types
///
/// This prevents accidentally using RouteId to index into components Vec.
/// Compiles to identical assembly as raw Vec<T> in release mode.
///
/// # Type Parameters
///
/// - `I`: The index type (must implement Idx trait)
/// - `T`: The element type
///
/// # Performance
///
/// - `push()`: O(1) amortized
/// - `get()`: O(1)
/// - `[]` indexing: O(1)
/// - Memory overhead: 0 bytes (just a Vec wrapper)
#[derive(Debug, Clone, PartialEq)]
pub struct IndexVec<I: Idx, T> {
    raw: Vec<T>,
    _marker: PhantomData<fn(&I)>,
}

impl<I: Idx, T> IndexVec<I, T> {
    /// Create a new empty IndexVec
    pub fn new() -> Self {
        Self {
            raw: Vec::new(),
            _marker: PhantomData,
        }
    }

    /// Create a new IndexVec with the specified capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            raw: Vec::with_capacity(capacity),
            _marker: PhantomData,
        }
    }

    /// Push a value and return its index
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut vec = IndexVec::<ComponentId, Component>::new();
    /// let id = vec.push(component);
    /// ```
    #[inline]
    pub fn push(&mut self, value: T) -> I {
        let idx = I::new(self.raw.len());
        self.raw.push(value);
        idx
    }

    /// Get a reference to an element by index
    #[inline]
    pub fn get(&self, index: I) -> Option<&T> {
        self.raw.get(index.index())
    }

    /// Get a mutable reference to an element by index
    #[inline]
    pub fn get_mut(&mut self, index: I) -> Option<&mut T> {
        self.raw.get_mut(index.index())
    }

    /// Returns the number of elements in the vector
    #[inline]
    pub fn len(&self) -> usize {
        self.raw.len()
    }

    /// Returns true if the vector contains no elements
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }

    /// Returns an iterator over the vector
    #[inline]
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.raw.iter()
    }

    /// Returns a mutable iterator over the vector
    #[inline]
    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, T> {
        self.raw.iter_mut()
    }

    /// Returns an iterator over (index, element) pairs
    pub fn iter_enumerated(&self) -> impl Iterator<Item = (I, &T)> {
        self.raw
            .iter()
            .enumerate()
            .map(|(idx, elem)| (I::new(idx), elem))
    }

    /// Clear all elements from the vector
    #[inline]
    pub fn clear(&mut self) {
        self.raw.clear();
    }

    /// Reserve capacity for at least `additional` more elements
    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        self.raw.reserve(additional);
    }
}

// Indexing support
impl<I: Idx, T> Index<I> for IndexVec<I, T> {
    type Output = T;

    #[inline]
    fn index(&self, idx: I) -> &T {
        &self.raw[idx.index()]
    }
}

impl<I: Idx, T> IndexMut<I> for IndexVec<I, T> {
    #[inline]
    fn index_mut(&mut self, idx: I) -> &mut T {
        &mut self.raw[idx.index()]
    }
}

impl<I: Idx, T> Default for IndexVec<I, T> {
    fn default() -> Self {
        Self::new()
    }
}

// Serde support (if T is serializable)
impl<I: Idx, T: Serialize> Serialize for IndexVec<I, T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.raw.serialize(serializer)
    }
}

impl<'de, I: Idx, T: Deserialize<'de>> Deserialize<'de> for IndexVec<I, T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Vec::deserialize(deserializer).map(|raw| IndexVec {
            raw,
            _marker: PhantomData,
        })
    }
}

// =============================================================================
// Macro for Defining ID Types
// =============================================================================

/// Macro to define type-safe u32 index types
///
/// # Example
///
/// ```rust
/// define_id_type!(ComponentId);
/// define_id_type!(RouteId);
///
/// // Now ComponentId and RouteId are distinct types
/// // that can't be accidentally mixed
/// ```
#[macro_export]
macro_rules! define_id_type {
    ($name:ident) => {
        #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
        #[derive(serde::Serialize, serde::Deserialize)]
        pub struct $name(pub u32);

        impl $crate::ast::arena::Idx for $name {
            #[inline]
            fn new(idx: usize) -> Self {
                Self(idx as u32)
            }

            #[inline]
            fn index(self) -> usize {
                self.0 as usize
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0)
            }
        }
    };
}

// =============================================================================
// ID Type Definitions
// =============================================================================

define_id_type!(StmtId);
define_id_type!(ExprId);
define_id_type!(ComponentId);
define_id_type!(PourId);
define_id_type!(PlaneId);
define_id_type!(PolygonId);
define_id_type!(ContactId);
define_id_type!(RouteId);
define_id_type!(SpaceInstanceId);
define_id_type!(ForLoopId);

// =============================================================================
// AST Arena
// =============================================================================

use crate::ast::component::ComponentPlacement;
use crate::ast::expression::Expression;
use crate::ast::space::placements::{
    ContactPlacement, PlanePlacement, PolygonPlacement, PourPlacement, SpaceInstancePlacement,
};
use crate::ast::space::routes::Route;
use crate::ast::space::space_def::{SpaceForLoop, SpaceTopLevelStatement};

/// Centralized arena for all AST nodes
///
/// All AST nodes are stored in contiguous vectors and referenced
/// via type-safe u32 indices.
///
/// # Memory Layout
///
/// ```text
/// components: [Component₀, Component₁, Component₂, ...]  ← Contiguous
/// routes:     [Route₀, Route₁, Route₂, ...]              ← Contiguous
/// pours:      [Pour₀, Pour₁, Pour₂, ...]                 ← Contiguous
/// ```
///
/// # Thread Safety
///
/// AstArena can be shared across threads (via Arc or &) because:
/// - All ID types are Copy + Send + Sync
/// - Immutable access to arena is safe from multiple threads
///
/// # Performance
///
/// - Allocation: O(1) amortized (Vec::push)
/// - Lookup: O(1) (array indexing)
/// - Traversal: Sequential scan (optimal cache usage)
/// - Drop: O(n) but fast (contiguous deallocation)
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AstArena {
    pub statements: IndexVec<StmtId, SpaceTopLevelStatement>,
    pub expressions: IndexVec<ExprId, Expression>,
    pub components: IndexVec<ComponentId, ComponentPlacement>,
    pub pours: IndexVec<PourId, PourPlacement>,
    pub planes: IndexVec<PlaneId, PlanePlacement>,
    pub polygons: IndexVec<PolygonId, PolygonPlacement>,
    pub contacts: IndexVec<ContactId, ContactPlacement>,
    pub routes: IndexVec<RouteId, Route>,
    pub space_instances: IndexVec<SpaceInstanceId, SpaceInstancePlacement>,
    pub for_loops: IndexVec<ForLoopId, SpaceForLoop>,
}

impl AstArena {
    /// Create a new empty arena
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new arena with pre-allocated capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            statements: IndexVec::with_capacity(capacity),
            expressions: IndexVec::with_capacity(capacity),
            components: IndexVec::with_capacity(capacity / 2),
            pours: IndexVec::with_capacity(capacity / 10),
            planes: IndexVec::with_capacity(capacity / 20),
            polygons: IndexVec::with_capacity(capacity / 20),
            contacts: IndexVec::with_capacity(capacity / 10),
            routes: IndexVec::with_capacity(capacity / 5),
            space_instances: IndexVec::with_capacity(capacity / 20),
            for_loops: IndexVec::with_capacity(capacity / 50),
        }
    }

    // Allocation methods
    
    #[inline]
    pub fn alloc_statement(&mut self, stmt: SpaceTopLevelStatement) -> StmtId {
        self.statements.push(stmt)
    }

    #[inline]
    pub fn alloc_expression(&mut self, expr: Expression) -> ExprId {
        self.expressions.push(expr)
    }

    #[inline]
    pub fn alloc_component(&mut self, comp: ComponentPlacement) -> ComponentId {
        self.components.push(comp)
    }

    #[inline]
    pub fn alloc_pour(&mut self, pour: PourPlacement) -> PourId {
        self.pours.push(pour)
    }

    #[inline]
    pub fn alloc_plane(&mut self, plane: PlanePlacement) -> PlaneId {
        self.planes.push(plane)
    }

    #[inline]
    pub fn alloc_polygon(&mut self, polygon: PolygonPlacement) -> PolygonId {
        self.polygons.push(polygon)
    }

    #[inline]
    pub fn alloc_contact(&mut self, contact: ContactPlacement) -> ContactId {
        self.contacts.push(contact)
    }

    #[inline]
    pub fn alloc_route(&mut self, route: Route) -> RouteId {
        self.routes.push(route)
    }

    #[inline]
    pub fn alloc_space_instance(&mut self, inst: SpaceInstancePlacement) -> SpaceInstanceId {
        self.space_instances.push(inst)
    }

    #[inline]
    pub fn alloc_for_loop(&mut self, loop_stmt: SpaceForLoop) -> ForLoopId {
        self.for_loops.push(loop_stmt)
    }

    /// Clear all arena contents (useful for reusing arena between parses)
    pub fn clear(&mut self) {
        self.statements.clear();
        self.expressions.clear();
        self.components.clear();
        self.pours.clear();
        self.planes.clear();
        self.polygons.clear();
        self.contacts.clear();
        self.routes.clear();
        self.space_instances.clear();
        self.for_loops.clear();
    }

    /// Get total memory usage estimate in bytes
    pub fn memory_usage(&self) -> usize {
        self.statements.len() * std::mem::size_of::<SpaceTopLevelStatement>()
            + self.expressions.len() * std::mem::size_of::<Expression>()
            + self.components.len() * std::mem::size_of::<ComponentPlacement>()
            + self.pours.len() * std::mem::size_of::<PourPlacement>()
            + self.planes.len() * std::mem::size_of::<PlanePlacement>()
            + self.polygons.len() * std::mem::size_of::<PolygonPlacement>()
            + self.contacts.len() * std::mem::size_of::<ContactPlacement>()
            + self.routes.len() * std::mem::size_of::<Route>()
            + self.space_instances.len() * std::mem::size_of::<SpaceInstancePlacement>()
            + self.for_loops.len() * std::mem::size_of::<SpaceForLoop>()
    }
}

// Serde support for AstArena
impl Serialize for AstArena {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("AstArena", 10)?;
        state.serialize_field("statements", &self.statements)?;
        state.serialize_field("expressions", &self.expressions)?;
        state.serialize_field("components", &self.components)?;
        state.serialize_field("pours", &self.pours)?;
        state.serialize_field("planes", &self.planes)?;
        state.serialize_field("polygons", &self.polygons)?;
        state.serialize_field("contacts", &self.contacts)?;
        state.serialize_field("routes", &self.routes)?;
        state.serialize_field("space_instances", &self.space_instances)?;
        state.serialize_field("for_loops", &self.for_loops)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for AstArena {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            Statements,
            Expressions,
            Components,
            Pours,
            Planes,
            Polygons,
            Contacts,
            Routes,
            SpaceInstances,
            ForLoops,
        }

        struct AstArenaVisitor;

        impl<'de> serde::de::Visitor<'de> for AstArenaVisitor {
            type Value = AstArena;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("struct AstArena")
            }

            fn visit_map<V>(self, mut map: V) -> Result<AstArena, V::Error>
            where
                V: serde::de::MapAccess<'de>,
            {
                let mut statements = None;
                let mut expressions = None;
                let mut components = None;
                let mut pours = None;
                let mut planes = None;
                let mut polygons = None;
                let mut contacts = None;
                let mut routes = None;
                let mut space_instances = None;
                let mut for_loops = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Statements => statements = Some(map.next_value()?),
                        Field::Expressions => expressions = Some(map.next_value()?),
                        Field::Components => components = Some(map.next_value()?),
                        Field::Pours => pours = Some(map.next_value()?),
                        Field::Planes => planes = Some(map.next_value()?),
                        Field::Polygons => polygons = Some(map.next_value()?),
                        Field::Contacts => contacts = Some(map.next_value()?),
                        Field::Routes => routes = Some(map.next_value()?),
                        Field::SpaceInstances => space_instances = Some(map.next_value()?),
                        Field::ForLoops => for_loops = Some(map.next_value()?),
                    }
                }

                Ok(AstArena {
                    statements: statements.unwrap_or_default(),
                    expressions: expressions.unwrap_or_default(),
                    components: components.unwrap_or_default(),
                    pours: pours.unwrap_or_default(),
                    planes: planes.unwrap_or_default(),
                    polygons: polygons.unwrap_or_default(),
                    contacts: contacts.unwrap_or_default(),
                    routes: routes.unwrap_or_default(),
                    space_instances: space_instances.unwrap_or_default(),
                    for_loops: for_loops.unwrap_or_default(),
                })
            }
        }

        const FIELDS: &[&str] = &[
            "statements",
            "expressions",
            "components",
            "pours",
            "planes",
            "polygons",
            "contacts",
            "routes",
            "space_instances",
            "for_loops",
        ];
        deserializer.deserialize_struct("AstArena", FIELDS, AstArenaVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_vec_type_safety() {
        let mut arena = AstArena::new();
        
        // This should compile
        let _comp_id = arena.components.push(ComponentPlacement::default());
        
        // Type safety is enforced at compile time
        // Uncommenting this would cause a compile error:
        // let route_id: RouteId = RouteId(0);
        // let _wrong = &arena.components[route_id];  // ERROR: mismatched types
    }

    #[test]
    fn test_arena_allocation() {
        let mut arena = AstArena::new();
        
        assert_eq!(arena.components.len(), 0);
        
        let id1 = arena.components.push(ComponentPlacement::default());
        assert_eq!(id1.0, 0);
        assert_eq!(arena.components.len(), 1);
        
        let id2 = arena.components.push(ComponentPlacement::default());
        assert_eq!(id2.0, 1);
        assert_eq!(arena.components.len(), 2);
    }

    #[test]
    fn test_arena_clear() {
        let mut arena = AstArena::new();
        
        arena.components.push(ComponentPlacement::default());
        arena.routes.push(Route::default());
        
        assert_eq!(arena.components.len(), 1);
        assert_eq!(arena.routes.len(), 1);
        
        arena.clear();
        
        assert_eq!(arena.components.len(), 0);
        assert_eq!(arena.routes.len(), 0);
    }
}
