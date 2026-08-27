//! HardwareScript v0.3.0 Native Physical Emitter Bridge
//!
//! The `SpaceEmitter` trait is the boundary between the compile-time evaluator
//! and the physical database. `MemoryEmitter` is the in-memory diagnostic
//! implementation that records emitted primitives for downstream export and verification.

use compact_str::CompactString;
use hwc_types::NetId;
use rustc_hash::FxHashMap;
use std::sync::Arc;

use super::context::EvalError;
use super::opcodes::Chunk;
use super::value::{FunctionId, MeasurementValue, Value};

/// Native Emitter Trait bridging compile-time `space.*` operations directly to the physical DB.
pub trait SpaceEmitter: std::fmt::Debug {
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;

    /// Resolve a compiled function by id into its bytecode `Chunk`.
    fn lookup_function(&self, id: FunctionId) -> Result<(Arc<Chunk>, usize), EvalError>;

    /// Allocate a net in the space database and return its `NetId`.
    fn allocate_net(
        &mut self,
        space_id: u32,
        name: &str,
        properties: FxHashMap<CompactString, Value>,
    ) -> Result<NetId, EvalError>;

    /// Add flat 2D polygon contour (points in integer picometers).
    fn add_polygon(
        &mut self,
        space_id: u32,
        layer: &str,
        net: Option<NetId>,
        points: Vec<(i64, i64)>,
        semantic_name: Option<CompactString>,
    ) -> Result<(), EvalError>;

    /// Add vertical contact/via pillar between layers.
    fn add_contact(
        &mut self,
        space_id: u32,
        from_layer: &str,
        to_layer: &str,
        at: (i64, i64),
        diameter_pm: i64,
        net: Option<NetId>,
        semantic_name: Option<CompactString>,
    ) -> Result<(), EvalError>;

    /// Bind semiconductor device contract for SPICE extraction.
    fn add_device(
        &mut self,
        space_id: u32,
        device_type: &str,
        name: &str,
        terminals: FxHashMap<CompactString, NetId>,
        params: FxHashMap<CompactString, MeasurementValue>,
    ) -> Result<(), EvalError>;

    /// Route interconnect between ports/points.
    fn add_route(
        &mut self,
        space_id: u32,
        from: Value,
        to: Value,
        intent: Option<CompactString>,
        properties: FxHashMap<CompactString, Value>,
    ) -> Result<(), EvalError>;
}

/// In-Memory Mock/Diagnostic Emitter for `hwc eval` and standalone unit testing.
#[derive(Debug, Default, Clone)]
pub struct MemoryEmitter {
    pub polygons: Vec<PolygonRecord>,
    pub contacts: Vec<ContactRecord>,
    pub devices: Vec<DeviceRecord>,
    pub routes: Vec<RouteRecord>,
    pub nets: FxHashMap<CompactString, NetId>,
    pub next_net_id: u32,
    pub functions: FxHashMap<FunctionId, Arc<Chunk>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolygonRecord {
    pub space_id: u32,
    pub layer: CompactString,
    pub net: Option<NetId>,
    pub points: Vec<(i64, i64)>,
    pub semantic_name: Option<CompactString>,  // v0.3.0: User-defined name for BOM/netlist
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContactRecord {
    pub space_id: u32,
    pub from_layer: CompactString,
    pub to_layer: CompactString,
    pub at: (i64, i64),
    pub diameter_pm: i64,
    pub net: Option<NetId>,
    pub semantic_name: Option<CompactString>,  // v0.3.0: User-defined name for BOM/netlist
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeviceRecord {
    pub space_id: u32,
    pub device_type: CompactString,
    pub name: CompactString,
    pub terminals: FxHashMap<CompactString, NetId>,
    pub params: FxHashMap<CompactString, MeasurementValue>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RouteRecord {
    pub space_id: u32,
    pub from: Value,
    pub to: Value,
    pub intent: Option<CompactString>,
    pub properties: FxHashMap<CompactString, Value>,
}

impl MemoryEmitter {
    pub fn new() -> Self {
        Self {
            polygons: Vec::new(),
            contacts: Vec::new(),
            devices: Vec::new(),
            routes: Vec::new(),
            nets: FxHashMap::default(),
            next_net_id: 1, // 0 is NetId::UNCONNECTED
            functions: FxHashMap::default(),
        }
    }

    /// Register compiled function chunks so the VM can resolve calls.
    pub fn register_functions(&mut self, functions: FxHashMap<FunctionId, Arc<Chunk>>) {
        self.functions = functions;
    }
}

impl SpaceEmitter for MemoryEmitter {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn lookup_function(&self, id: FunctionId) -> Result<(Arc<Chunk>, usize), EvalError> {
        self.functions
            .get(&id)
            .cloned()
            .map(|c| (c, 0))
            .ok_or_else(|| EvalError::General {
                message: format!("Unknown function id {}", id.0),
            })
    }

    fn allocate_net(
        &mut self,
        _space_id: u32,
        name: &str,
        _properties: FxHashMap<CompactString, Value>,
    ) -> Result<NetId, EvalError> {
        let compact_name = CompactString::new(name);
        if let Some(id) = self.nets.get(&compact_name) {
            Ok(*id)
        } else {
            let id = NetId::new(self.next_net_id);
            self.next_net_id += 1;
            self.nets.insert(compact_name, id);
            Ok(id)
        }
    }

    fn add_polygon(
        &mut self,
        space_id: u32,
        layer: &str,
        net: Option<NetId>,
        points: Vec<(i64, i64)>,
        semantic_name: Option<CompactString>,
    ) -> Result<(), EvalError> {
        self.polygons.push(PolygonRecord {
            space_id,
            layer: CompactString::new(layer),
            net,
            points,
            semantic_name,
        });
        Ok(())
    }

    fn add_contact(
        &mut self,
        space_id: u32,
        from_layer: &str,
        to_layer: &str,
        at: (i64, i64),
        diameter_pm: i64,
        net: Option<NetId>,
        semantic_name: Option<CompactString>,
    ) -> Result<(), EvalError> {
        self.contacts.push(ContactRecord {
            space_id,
            from_layer: CompactString::new(from_layer),
            to_layer: CompactString::new(to_layer),
            at,
            diameter_pm,
            net,
            semantic_name,
        });
        Ok(())
    }

    fn add_device(
        &mut self,
        space_id: u32,
        device_type: &str,
        name: &str,
        terminals: FxHashMap<CompactString, NetId>,
        params: FxHashMap<CompactString, MeasurementValue>,
    ) -> Result<(), EvalError> {
        self.devices.push(DeviceRecord {
            space_id,
            device_type: CompactString::new(device_type),
            name: CompactString::new(name),
            terminals,
            params,
        });
        Ok(())
    }

    fn add_route(
        &mut self,
        space_id: u32,
        from: Value,
        to: Value,
        intent: Option<CompactString>,
        properties: FxHashMap<CompactString, Value>,
    ) -> Result<(), EvalError> {
        self.routes.push(RouteRecord {
            space_id,
            from,
            to,
            intent,
            properties,
        });
        Ok(())
    }
}
