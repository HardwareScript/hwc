//! Core arena types: Idx trait, IndexVec, and macro definitions
//!
//! This module provides the foundational infrastructure for type-safe arena allocation:
//! - `Idx` trait for custom index types
//! - `IndexVec<I, T>` for type-safe indexing
//! - `define_id_type!` macro for creating ID types

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
    /// ```
    /// use hwc_parser::ast::arena::{IndexVec, Idx};
    ///
    /// // Define a simple ID type for the example
    /// #[derive(Copy, Clone, PartialEq, Eq, Hash)]
    /// struct ItemId(u32);
    ///
    /// impl Idx for ItemId {
    ///     fn new(idx: usize) -> Self { ItemId(idx as u32) }
    ///     fn index(self) -> usize { self.0 as usize }
    /// }
    ///
    /// let mut vec = IndexVec::<ItemId, String>::new();
    /// let id = vec.push("hello".to_string());
    /// assert_eq!(vec[id], "hello");
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

    /// Append all elements from another IndexVec into this one
    #[inline]
    pub fn extend_from(&mut self, mut other: IndexVec<I, T>) {
        self.raw.append(&mut other.raw);
    }
}

// =============================================================================
// Indexing Support
// =============================================================================

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

// =============================================================================
// Serde Support for IndexVec
// =============================================================================

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
/// ```
/// use hwc_parser::define_id_type;
/// use hwc_parser::ast::arena::{IndexVec, Idx};
///
/// // Define ID types for different entity kinds
/// define_id_type!(ComponentId);
/// define_id_type!(RouteId);
///
/// // Now you can create type-safe vectors
/// let mut components = IndexVec::<ComponentId, String>::new();
/// let mut routes = IndexVec::<RouteId, String>::new();
///
/// let c_id = components.push("resistor".to_string());
/// let r_id = routes.push("trace".to_string());
///
/// // This compiles: using ComponentId with components
/// assert_eq!(components[c_id], "resistor");
///
/// // This would NOT compile (type mismatch):
/// // let _ = components[r_id]; // ERROR: expected ComponentId, found RouteId
/// ```
#[macro_export]
macro_rules! define_id_type {
    ($name:ident) => {
        #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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
