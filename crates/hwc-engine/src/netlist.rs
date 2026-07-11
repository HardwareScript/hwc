//! ECS-style netlist storage with strongly-typed IDs.
//!
//! This module implements a custom arena allocator for components, pins, and nets.
//! Unlike generic graph libraries (petgraph), this uses strongly-typed IDs (u32 indices)
//! for zero-cost abstractions and O(1) queries without runtime borrow checking.

mod handle;

pub use handle::{NetHandle, NetLookupTable};

use compact_str::CompactString;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

/// Strongly-typed component ID (newtype wrapper around u32).
///
/// Zero memory overhead - compiles to a raw u32.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ComponentId(pub u32);

impl ComponentId {
    /// Create a new component ID.
    #[inline]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Get the raw ID value.
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// Strongly-typed pin ID (newtype wrapper around u32).
///
/// Zero memory overhead - compiles to a raw u32.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct PinId(pub u32);

impl PinId {
    /// Create a new pin ID.
    #[inline]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Get the raw ID value.
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// Strongly-typed net ID (newtype wrapper around u32).
///
/// Zero memory overhead - compiles to a raw u32.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct NetId(pub u32);

impl NetId {
    /// Create a new net ID.
    #[inline]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Get the raw ID value.
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// Component data stored in the arena.
#[derive(Clone, Debug)]
pub struct ComponentData {
    /// Component name (e.g., "Power", "LED1")
    pub name: CompactString,

    /// Component type (e.g., "Battery", "LED")
    pub component_type: CompactString,

    /// Position in nanometers (X, Y, Z order)
    pub position_nm: (i64, i64, i64),

    /// First pin in the pin array
    pub first_pin: PinId,

    /// Number of pins this component has
    pub pin_count: u32,
}

/// Pin data stored in the arena.
#[derive(Clone, Debug)]
pub struct PinData {
    /// Pin name (e.g., "Plus", "Anode")
    pub name: CompactString,

    /// Parent component
    pub parent_component: ComponentId,

    /// Connected net (if any)
    pub connected_net: Option<NetId>,

    /// Local offset from component position in nanometers (X, Y, Z order)
    pub local_offset_nm: (i64, i64, i64),

    /// Pad shape for solder mask and paste layer generation
    pub pad_shape: Option<crate::placement::PadShape>,
}

/// Net data stored in the arena.
#[derive(Clone, Debug)]
pub struct NetData {
    /// Net name (e.g., "VCC", "GND")
    pub name: CompactString,

    /// Trace width in nanometers
    pub width_nm: i64,

    /// Material ID (e.g., Copper = 2)
    pub material: u8,

    /// Pins connected to this net
    pub pins: SmallVec<[PinId; 8]>,

    /// v0.1.7: Signal frequency in Hz (e.g., 5_000_000_000.0 for 5 GHz).
    /// None if frequency is unspecified. Used to classify high-speed nets
    /// that must avoid reference-plane voids.
    pub frequency_hz: Option<f64>,

    /// v0.1.8: Target current in milliamps (e.g., 500mA).
    /// None if current is unspecified. Used for thermal and EM validation.
    pub current_ma: Option<f64>,
}

/// ECS-style arena for netlist storage.
///
/// Uses Struct of Arrays (SoA) design for cache-friendly access.
/// All queries are O(1) array lookups with no runtime borrow checking.
#[derive(Debug)]
pub struct NetlistArena {
    /// All components (indexed by ComponentId)
    components: Vec<ComponentData>,

    /// All pins (indexed by PinId)
    pins: Vec<PinData>,

    /// All nets (indexed by NetId)
    nets: Vec<NetData>,

    /// Component name → ID lookup
    component_names: FxHashMap<CompactString, ComponentId>,

    /// Net name → ID lookup
    net_names: FxHashMap<CompactString, NetId>,
}

impl NetlistArena {
    /// Create a new empty netlist arena.
    pub fn new() -> Self {
        Self {
            components: Vec::new(),
            pins: Vec::new(),
            nets: Vec::new(),
            component_names: FxHashMap::default(),
            net_names: FxHashMap::default(),
        }
    }

    /// Add a component to the arena.
    ///
    /// Returns the component ID.
    pub fn add_component(
        &mut self,
        name: CompactString,
        component_type: CompactString,
        position_nm: (i64, i64, i64),
    ) -> ComponentId {
        let id = ComponentId::new(self.components.len() as u32);

        let component = ComponentData {
            name: name.clone(),
            component_type,
            position_nm,
            first_pin: PinId::new(self.pins.len() as u32),
            pin_count: 0,
        };

        self.components.push(component);
        self.component_names.insert(name, id);

        id
    }

    /// Add a pin to a component.
    ///
    /// Returns the pin ID.
    pub fn add_pin(
        &mut self,
        component: ComponentId,
        name: CompactString,
        local_offset_nm: (i64, i64, i64),
        pad_shape: Option<crate::placement::PadShape>,
    ) -> PinId {
        let id = PinId::new(self.pins.len() as u32);

        let pin = PinData {
            name,
            parent_component: component,
            connected_net: None,
            local_offset_nm,
            pad_shape,
        };

        self.pins.push(pin);

        // Update component pin count
        if let Some(comp) = self.components.get_mut(component.0 as usize) {
            comp.pin_count += 1;
        }

        id
    }

    /// Add a net to the arena.
    ///
    /// Returns the net ID.
    ///
    /// NOTE: Net IDs start from 1 (not 0) so that 0 can represent "unassigned" in substrate layers.
    pub fn add_net(&mut self, name: CompactString, width_nm: i64, material: u8) -> NetId {
        let id = NetId::new((self.nets.len() + 1) as u32); // Start from 1, not 0

        let net = NetData {
            name: name.clone(),
            width_nm,
            material,
            pins: SmallVec::new(),
            frequency_hz: None,
            current_ma: None,
        };

        self.nets.push(net);
        self.net_names.insert(name, id);

        id
    }

    /// v0.1.7: Set the signal frequency (in Hz) for a net.
    pub fn set_net_frequency(&mut self, net: NetId, frequency_hz: f64) {
        if net.0 > 0 {
            if let Some(net_data) = self.nets.get_mut((net.0 - 1) as usize) {
                net_data.frequency_hz = Some(frequency_hz);
            }
        }
    }

    /// v0.1.8: Set the target current (in mA) for a net.
    pub fn set_net_current(&mut self, net: NetId, current_ma: f64) {
        if net.0 > 0 {
            if let Some(net_data) = self.nets.get_mut((net.0 - 1) as usize) {
                net_data.current_ma = Some(current_ma);
            }
        }
    }

    /// Returns the name of a net.
    pub fn get_net_name(&self, net: NetId) -> Option<CompactString> {
        self.nets.get((net.0 - 1) as usize).map(|n| n.name.clone())
    }

    /// Connect a pin to a net.
    pub fn connect_pin(&mut self, pin: PinId, net: NetId) {
        // Update pin's connected net
        if let Some(pin_data) = self.pins.get_mut(pin.0 as usize) {
            pin_data.connected_net = Some(net);
        }

        // Add pin to net's pin list
        if net.0 > 0 {
            if let Some(net_data) = self.nets.get_mut((net.0 - 1) as usize) {
                net_data.pins.push(pin);
            }
        }
    }

    /// Get component data by ID.
    #[inline]
    pub fn get_component(&self, id: ComponentId) -> Option<&ComponentData> {
        self.components.get(id.0 as usize)
    }

    /// Get mutable component data by ID.
    #[inline]
    pub fn get_component_mut(&mut self, id: ComponentId) -> Option<&mut ComponentData> {
        self.components.get_mut(id.0 as usize)
    }

    /// Get component by name.
    #[inline]
    pub fn get_component_by_name(&self, name: &str) -> Option<ComponentId> {
        self.component_names.get(name).copied()
    }

    /// Get a pin of a component by its name.
    pub fn get_pin_by_name(&self, component: ComponentId, pin_name: &str) -> Option<PinId> {
        let comp = self.get_component(component)?;
        let first = comp.first_pin.0 as usize;
        let count = comp.pin_count as usize;

        for i in first..first + count {
            if let Some(pin) = self.pins.get(i) {
                if pin.name == pin_name {
                    return Some(PinId::new(i as u32));
                }
            }
        }
        None
    }

    /// Get the number of components in the arena.
    #[inline]
    pub fn component_count(&self) -> usize {
        self.components.len()
    }

    /// Get pin data by ID.
    #[inline]
    pub fn get_pin(&self, id: PinId) -> Option<&PinData> {
        self.pins.get(id.0 as usize)
    }

    /// Get mutable pin data by ID.
    #[inline]
    pub fn get_pin_mut(&mut self, id: PinId) -> Option<&mut PinData> {
        self.pins.get_mut(id.0 as usize)
    }

    /// Get net data by ID.
    ///
    /// NOTE: Net IDs are 1-based, so we subtract 1 to get the array index.
    #[inline]
    pub fn get_net(&self, id: NetId) -> Option<&NetData> {
        if id.0 == 0 {
            return None; // 0 means unassigned
        }
        self.nets.get((id.0 - 1) as usize)
    }

    /// Get mutable net data by ID.
    ///
    /// NOTE: Net IDs are 1-based, so we subtract 1 to get the array index.
    #[inline]
    pub fn get_net_mut(&mut self, id: NetId) -> Option<&mut NetData> {
        if id.0 == 0 {
            return None; // 0 means unassigned
        }
        self.nets.get_mut((id.0 - 1) as usize)
    }

    /// Get net by name.
    #[inline]
    pub fn get_net_by_name(&self, name: &str) -> Option<NetId> {
        self.net_names.get(name).copied()
    }

    /// Get or create a net by name (v0.1.7).
    ///
    /// If the net doesn't exist, it is created with default parameters (0.2mm width, Copper).
    pub fn get_or_create_net(&mut self, name: &str) -> NetId {
        if let Some(id) = self.get_net_by_name(name) {
            id
        } else {
            self.add_net(name.into(), 200_000, 2)
        }
    }

    /// Get or create a net, automatically adjusting the default width based on technology
    pub fn get_or_create_net_with_technology(
        &mut self,
        name: &str,
        is_asic: bool,
        profile_min_width_nm: i64,
    ) -> NetId {
        if let Some(id) = self.get_net_by_name(name) {
            id
        } else {
            // If ASIC, use the microscopic trace width (e.g. 180nm)
            // If PCB, fall back to the standard board trace width (e.g. 200um)
            let default_width = if is_asic {
                profile_min_width_nm
            } else {
                200_000 // 0.2mm PCB default
            };

            self.add_net(name.into(), default_width, 2)
        }
    }

    /// Get the global position of a pin (component position + local offset).
    ///
    /// Pure integer math - no floating point.
    /// Returns (x, y, z) in nanometers.
    #[inline]
    /// Get absolute pin position in nanometers.
    ///
    /// COORDINATE SYSTEM: Top-Left Anchor
    /// - Component position is the top-left corner
    /// - Pin local_offset is relative to top-left corner
    /// - Absolute position = component_anchor + pin_offset (simple addition)
    pub fn get_pin_position(&self, pin: PinId) -> Option<(i64, i64, i64)> {
        let pin_data = self.get_pin(pin)?;
        let component = self.get_component(pin_data.parent_component)?;

        Some((
            component.position_nm.0 + pin_data.local_offset_nm.0,
            component.position_nm.1 + pin_data.local_offset_nm.1,
            component.position_nm.2 + pin_data.local_offset_nm.2,
        ))
    }

    /// Get the net connected to a pin.
    #[inline]
    pub fn get_connected_net(&self, pin: PinId) -> Option<NetId> {
        self.get_pin(pin)?.connected_net
    }

    /// Get all pins on a net.
    #[inline]
    pub fn get_net_pins(&self, net: NetId) -> Option<&[PinId]> {
        Some(&self.get_net(net)?.pins)
    }

    /// Get all pins of a component.
    pub fn get_component_pins(&self, component: ComponentId) -> Vec<PinId> {
        let comp = match self.get_component(component) {
            Some(c) => c,
            None => return Vec::new(),
        };

        let first = comp.first_pin.0 as usize;
        let count = comp.pin_count as usize;

        (first..first + count)
            .map(|i| PinId::new(i as u32))
            .collect()
    }

    /// Get statistics about the arena.
    pub fn stats(&self) -> ArenaStats {
        ArenaStats {
            component_count: self.components.len(),
            pin_count: self.pins.len(),
            net_count: self.nets.len(),
        }
    }

    /// Get the number of nets in the arena.
    pub fn num_nets(&self) -> usize {
        self.nets.len()
    }

    /// Get all net IDs in the arena.
    ///
    /// Returns an iterator over all valid NetIds.
    pub fn all_net_ids(&self) -> impl Iterator<Item = NetId> + '_ {
        (1..=self.nets.len()).map(|i| NetId::new(i as u32))
    }
}

impl Default for NetlistArena {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about the netlist arena.
#[derive(Debug, Clone, Copy)]
pub struct ArenaStats {
    pub component_count: usize,
    pub pin_count: usize,
    pub net_count: usize,
}
