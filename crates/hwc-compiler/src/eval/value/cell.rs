use compact_str::CompactString;
use hwc_types::NetId;
use super::Value;

/// Relative geometric polygon in a pure CellLayout
#[derive(Debug, Clone, PartialEq)]
pub struct CellPolygon {
    pub layer: CompactString,
    pub points: Vec<(i64, i64)>, // local coordinates in picometers
    pub net: Option<NetId>,
    pub port: Option<CompactString>,
}

/// Relative contact cut / via pillar in a pure CellLayout
#[derive(Debug, Clone, PartialEq)]
pub struct CellContact {
    pub name: CompactString,
    pub from_layer: CompactString,
    pub to_layer: CompactString,
    pub at: (i64, i64), // local coordinates in picometers
    pub diameter: i64,  // diameter in picometers
    pub net: Option<NetId>,
}

/// Named connection port exposed by a CellLayout
#[derive(Debug, Clone, PartialEq)]
pub struct CellPort {
    pub name: CompactString,
    pub at: (i64, i64), // local coordinates in picometers
    pub layer: CompactString,
    pub net: Option<NetId>,
}

/// Local sub-device declaration inside a CellLayout
#[derive(Debug, Clone, PartialEq)]
pub struct CellDevice {
    pub device_type: CompactString,
    pub instance_name: CompactString,
    pub terminals: Vec<(CompactString, CompactString)>,
    pub params: Vec<(CompactString, Value)>,
}
