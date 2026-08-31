use compact_str::CompactString;
use hwc_types::NetId;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use super::cell_layout::CellLayout;
use super::ids::{DeviceId, FunctionId, SpaceId};
use super::measurement::MeasurementValue;
use super::placed::{PlacedCellInstance, PlacedPort};

/// Unified value produced and manipulated by the compile-time evaluation engine.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    // ── Primitive Literals ──
    Void,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(CompactString),

    // ── Physical & Geometric Primitives (Evaluated in Picometers) ──
    Measurement(MeasurementValue),
    Point2D {
        x: i64,
        y: i64,
    },
    Point3D {
        x: i64,
        y: i64,
        z: i64,
    },
    Vector2D {
        dx: i64,
        dy: i64,
    },
    BoundingBox {
        min_x: i64,
        min_y: i64,
        max_x: i64,
        max_y: i64,
    },

    // ── Pure Physical PCell & Placed Cell Handles (Pillar 1) ──
    CellLayout(Arc<CellLayout>),
    PlacedCell(Arc<PlacedCellInstance>),
    PlacedPort(PlacedPort),

    // ── Composites & References ──
    Array(Arc<Vec<Value>>),
    Tuple(Arc<Vec<Value>>),
    StructInstance {
        name: CompactString,
        fields: Arc<Vec<(CompactString, Value)>>,
    },
    EnumVariant {
        enum_name: CompactString,
        variant_name: CompactString,
        payload: Option<Arc<Vec<Value>>>,
    },
    EnumType {
        name: CompactString,
        variants: Arc<FxHashMap<CompactString, Value>>,
    },
    FunctionRef(FunctionId),

    // ── Hardware Domain Handles ──
    NetHandle(NetId),
    SpaceHandle(SpaceId),
    DeviceHandle(DeviceId),
}
