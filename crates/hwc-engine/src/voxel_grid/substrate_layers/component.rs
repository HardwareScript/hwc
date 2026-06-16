use super::super::chunk::{MaterialId, NetId};
use super::types::Terminal;
use crate::geometry::BoundingBox;
use compact_str::CompactString;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

/// Component pin for physical continuity validation (v0.1.6 Sprint 3).
///
/// Represents an external connection point on a component. Pins are used by
/// the P43 validator to detect floating conductors - conductive geometry that
/// has no component pins touching it.
///
/// Pins are registered during component placement and stored in absolute
/// coordinates (nanometers). The physics validator checks if each conductive
/// island has at least one pin touching it.
///
/// Total size: ~40 bytes (position + name pointer + net pointer)
///
/// # Example
/// ```
/// # use hwc_engine::voxel_grid::ComponentPin;
/// let pin = ComponentPin::new(
///     1_000_000,  // x: 1mm
///     2_000_000,  // y: 2mm
///     0,          // z: 0mm (bottom layer)
///     "M1".into(),
///     "gate".into(),
///     Some("VIN".into())
/// );
/// assert_eq!(pin.x_nm, 1_000_000);
/// assert_eq!(pin.component_name, "M1");
/// assert_eq!(pin.pin_name, "gate");
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentPin {
    /// X coordinate in nanometers (absolute position)
    pub x_nm: i64,

    /// Y coordinate in nanometers (absolute position)
    pub y_nm: i64,

    /// Z coordinate in nanometers (absolute position)
    pub z_nm: i64,

    /// Component instance name (e.g., "M1", "R1")
    pub component_name: CompactString,

    /// Pin name within the component (e.g., "gate", "drain", "source", "A", "K")
    pub pin_name: CompactString,

    /// Net assignment (e.g., "VIN", "GND", "VDD")
    /// None if the pin is not connected to any net
    pub net: Option<CompactString>,
}

impl ComponentPin {
    /// Create a new component pin.
    ///
    /// # Arguments
    /// * `x_nm` - X coordinate in nanometers (absolute)
    /// * `y_nm` - Y coordinate in nanometers (absolute)
    /// * `z_nm` - Z coordinate in nanometers (absolute)
    /// * `component_name` - Component instance name (e.g., "M1")
    /// * `pin_name` - Pin name within the component (e.g., "gate")
    /// * `net` - Optional net assignment (e.g., Some("VIN"))
    pub fn new(
        x_nm: i64,
        y_nm: i64,
        z_nm: i64,
        component_name: CompactString,
        pin_name: CompactString,
        net: Option<CompactString>,
    ) -> Self {
        Self {
            x_nm,
            y_nm,
            z_nm,
            component_name,
            pin_name,
            net,
        }
    }

    /// Get the position as a tuple (x, y, z) in nanometers.
    pub fn position(&self) -> (i64, i64, i64) {
        (self.x_nm, self.y_nm, self.z_nm)
    }

    /// Get a display name for this pin (e.g., "M1.gate").
    pub fn display_name(&self) -> CompactString {
        format!("{}.{}", self.component_name, self.pin_name).into()
    }
}

/// Component metadata for sparse component architecture.
///
/// GOD-TIER SPARSE ARCHITECTURE: Same pattern as SubstrateLayer.
/// Instead of filling millions of voxels per component (Density Bomb),
/// we store just the bounding box, material ID, and component name.
///
/// Router sees components via get_material() lookup (O(components) per query).
/// Placement is O(1): Just push to vector.
/// Memory is O(components), not O(voxels).
///
/// Total size: ~72 bytes (bbox + material + name pointer + blocked_z_ranges)
///
/// # Layer-Aware Keepout Zones (KOZ) — v0.1.7
///
/// `blocked_z_ranges` defines which Z-layers this component blocks for
/// pours and traces. A component sitting on M3 (top metal) should only
/// block the M3 Z-range, allowing pours on M1/M2 to pass underneath.
///
/// When `blocked_z_ranges` is empty (default), the component blocks ALL
/// Z-layers it occupies (legacy behavior for backward compatibility).
///
/// # Example
/// ```
/// # use hwc_engine::geometry::{BoundingBox, Point3D};
/// # use hwc_engine::voxel_grid::ComponentMetadata;
/// # use smallvec::SmallVec;
/// let bbox = BoundingBox::new(
///     Point3D::new(1_000_000, 1_000_000, 0),
///     Point3D::new(6_000_000, 3_000_000, 1_000_000)
/// );
/// let component = ComponentMetadata::new(5, bbox, "R1".into());
/// assert_eq!(component.material, 5);
/// assert_eq!(component.name, "R1");
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentMetadata {
    /// Material ID (e.g., 5 = Ceramic, 10 = Polysilicon)
    pub material: MaterialId,

    /// Bounding box in nanometers defining the component region
    pub bbox: BoundingBox,

    /// Component name for debugging and error messages
    pub name: CompactString,

    /// Component type (e.g., "Resistor", "Transistor", "MCU")
    pub component_type: CompactString,

    /// Terminal positions (absolute world coordinates)
    pub terminals: Vec<Terminal>,

    /// Net bindings
    pub net_bindings: FxHashMap<CompactString, NetId>,

    /// Layer-Aware Keepout Zones (KOZ) — v0.1.7
    ///
    /// A list of Z-ranges [start_nm, end_nm) that this component blocks.
    /// Pours and traces at a Z outside these ranges can pass through the
    /// component's XY footprint without collision.
    ///
    /// When empty (default), the component blocks ALL Z-layers it occupies
    /// (legacy behavior — full 3D keepout).
    ///
    /// Example: A surface-mount resistor on M3 (z=500µm to 600µm) would
    /// block z:[500_000, 600_000] but permit pours on M1/M2 underneath.
    pub blocked_z_ranges: SmallVec<[(i64, i64); 2]>,
}

impl ComponentMetadata {
    /// Create a new component metadata entry.
    ///
    /// # Arguments
    /// * `material` - Material ID (e.g., 5 for Ceramic)
    /// * `bbox` - Bounding box in nanometers
    /// * `name` - Component name (e.g., "R1", "Q1")
    /// * `component_type` - Component type (e.g., "Resistor")
    pub fn new(
        material: MaterialId,
        bbox: BoundingBox,
        name: CompactString,
        component_type: CompactString,
    ) -> Self {
        Self {
            material,
            bbox,
            name,
            component_type,
            terminals: Vec::new(),
            net_bindings: FxHashMap::default(),
            blocked_z_ranges: SmallVec::new(),
        }
    }

    /// Add a terminal to the component
    pub fn add_terminal(&mut self, terminal: Terminal) {
        self.terminals.push(terminal);
    }

    /// Bind a pin to a net
    pub fn bind_net(&mut self, pin_name: CompactString, net_id: NetId) {
        self.net_bindings.insert(pin_name, net_id);
    }

    /// Check if a point (in nanometers) is within this component.
    ///
    /// This is the O(1) lookup operation for component material queries.
    ///
    /// # Arguments
    /// * `x`, `y`, `z` - Coordinates in nanometers
    ///
    /// # Returns
    /// `true` if the point is within the component bounding box
    #[inline]
    pub fn contains_nm(&self, x: i64, y: i64, z: i64) -> bool {
        x >= self.bbox.min.x
            && x <= self.bbox.max.x
            && y >= self.bbox.min.y
            && y <= self.bbox.max.y
            && z >= self.bbox.min.z
            && z <= self.bbox.max.z
    }

    /// Check if a point (in nanometers) is inside this component's keepout zone.
    ///
    /// Layer-Aware KOZ (v0.1.7):
    /// - If `blocked_z_ranges` is empty: blocks all Z-layers (full 3D keepout)
    /// - If `blocked_z_ranges` is non-empty: only blocks the listed Z-ranges
    ///
    /// This enables pours and traces to flow under/over components on
    /// different Z-layers (e.g., M3 trace under an M1 component).
    ///
    /// # Arguments
    /// * `x`, `y`, `z` - Coordinates in nanometers
    ///
    /// # Returns
    /// `true` if the point is inside the keepout zone (pour/trace should block)
    #[inline]
    pub fn is_in_koz(&self, x: i64, y: i64, z: i64) -> bool {
        if !(x >= self.bbox.min.x
            && x <= self.bbox.max.x
            && y >= self.bbox.min.y
            && y <= self.bbox.max.y)
        {
            return false;
        }

        if self.blocked_z_ranges.is_empty() {
            return z >= self.bbox.min.z && z <= self.bbox.max.z;
        }

        for &(z_start, z_end) in &self.blocked_z_ranges {
            if z >= z_start && z <= z_end {
                return true;
            }
        }

        false
    }
}
