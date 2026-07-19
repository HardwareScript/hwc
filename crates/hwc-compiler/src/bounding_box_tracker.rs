//! Bounding Box Tracker (Sprint 3, Task 3.1)
//!
//! Tracks bounding boxes for all placed components and pours to enable
//! relative positioning: `at M1.right + 1mm`
//!
//! This is a simple name-to-bbox lookup table that gets populated during
//! component and pour placement.

use compact_str::CompactString;
use hwc_engine::geometry::{BoundingBox, Point3D};
use rustc_hash::FxHashMap;

/// Component placement metadata for relative positioning
///
/// v0.1.7: Stores both bounding box AND original placement point
/// This is critical for coordinate inheritance with the `last` keyword.
#[derive(Debug, Clone)]
struct PlacementMetadata {
    /// Bounding box of the placed component
    bbox: BoundingBox,
    /// Original placement point (before size was added)
    /// This is the point the user specified in `at [x: ..., y: ..., z: ...]`
    origin: Point3D,
}

/// Tracks bounding boxes for components and pours
///
/// Used by ConstraintSolver to resolve relative positions like:
/// - `at M1.right + 1mm`
/// - `at Resistor1.top + [0.5mm, 1mm, 0mm]`
/// - `at last.right + 1mm` (O(1) lookup of most recently placed component)
#[derive(Debug, Clone, Default)]
pub struct BoundingBoxTracker {
    /// Map from component/pour name to placement metadata
    metadata: FxHashMap<CompactString, PlacementMetadata>,

    /// O(1) tracking of most recently registered component (for 'last' keyword)
    /// Updated on every register() call - no need to iterate through all names
    last_registered_name: Option<CompactString>,
}

impl BoundingBoxTracker {
    /// Create a new empty tracker
    pub fn new() -> Self {
        Self {
            metadata: FxHashMap::default(),
            last_registered_name: None,
        }
    }

    pub fn register(&mut self, name: CompactString, bbox: BoundingBox, origin: Point3D) {
        self.last_registered_name = Some(name.clone());
        self.metadata
            .insert(name, PlacementMetadata { bbox, origin });
    }

    /// Get the name of the most recently registered component (for 'last' keyword)
    ///
    /// **Performance:** O(1) - no iteration needed
    ///
    /// # Returns
    /// The name of the last registered component, or None if no components have been placed
    pub fn last_registered(&self) -> Option<&CompactString> {
        self.last_registered_name.as_ref()
    }

    /// Get the bounding box for a named entity
    ///
    /// Returns `None` if the entity hasn't been placed yet.
    ///
    /// # Arguments
    /// * `name` - Name of the component or pour
    ///
    /// # Returns
    /// The bounding box if found, or None
    pub fn get(&self, name: &str) -> Option<&BoundingBox> {
        self.metadata.get(name).map(|m| &m.bbox)
    }

    /// Get the original placement point for a named entity (v0.1.7)
    ///
    /// This is the point the user specified in `at [x: ..., y: ..., z: ...]`
    /// CRITICAL for coordinate inheritance with the `last` keyword.
    ///
    /// # Arguments
    /// * `name` - Name of the component or pour
    ///
    /// # Returns
    /// The origin point if found, or None
    pub fn get_origin(&self, name: &str) -> Option<Point3D> {
        self.metadata.get(name).map(|m| m.origin)
    }

    /// Check if an entity has been registered
    pub fn contains(&self, name: &str) -> bool {
        self.metadata.contains_key(name)
    }

    /// Get all registered entity names
    pub fn all_names(&self) -> Vec<&CompactString> {
        self.metadata.keys().collect()
    }

    /// Clear all registered bounding boxes
    pub fn clear(&mut self) {
        self.metadata.clear();
        self.last_registered_name = None;
    }

    /// Get the number of registered entities
    pub fn len(&self) -> usize {
        self.metadata.len()
    }

    /// Check if the tracker is empty
    pub fn is_empty(&self) -> bool {
        self.metadata.is_empty()
    }
}
