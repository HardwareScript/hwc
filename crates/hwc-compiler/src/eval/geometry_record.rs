//! Salsa-Compliant Pure Geometry Record and Buffering Subsystem (Phase 2).
//!
//! Replaces legacy in-place side-effecting database mutation with pure, immutable
//! `GeometryBuffer` streams bearing mandatory span-independent `EntityId`s.

use compact_str::CompactString;
use hwc_engine::entity_graph::identity::EntityId;
use serde::{Deserialize, Serialize};
use super::value::SpaceId;

/// Pure, Merkle-bearing physical geometry record.
/// Every variant carries a mandatory `id: EntityId` derived from the active `HierarchicalPath`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GeometryRecord {
    Polygon {
        id: EntityId,
        space_id: SpaceId,
        layer: CompactString,
        net_id: Option<u32>,
        points_pm: Vec<(i64, i64)>,
    },
    Contact {
        id: EntityId,
        space_id: SpaceId,
        from_layer: CompactString,
        to_layer: CompactString,
        center_pm: (i64, i64),
        diameter_pm: i64,
        net_id: Option<u32>,
    },
    Device {
        id: EntityId,
        space_id: SpaceId,
        device_type: CompactString,
        instance_name: CompactString,
        terminals: Vec<(CompactString, u32)>,
        params: Vec<(CompactString, f64)>,
    },
    RouteIntent {
        id: EntityId,
        space_id: SpaceId,
        from_port: (i64, i64, u8),
        to_port: (i64, i64, u8),
        intent: CompactString,
    },
}

impl GeometryRecord {
    /// Returns the mandatory Merkle EntityId of this record.
    #[inline(always)]
    pub fn entity_id(&self) -> EntityId {
        match self {
            GeometryRecord::Polygon { id, .. } => *id,
            GeometryRecord::Contact { id, .. } => *id,
            GeometryRecord::Device { id, .. } => *id,
            GeometryRecord::RouteIntent { id, .. } => *id,
        }
    }

    /// Returns the space id of this record.
    #[inline(always)]
    pub fn space_id(&self) -> SpaceId {
        match self {
            GeometryRecord::Polygon { space_id, .. } => *space_id,
            GeometryRecord::Contact { space_id, .. } => *space_id,
            GeometryRecord::Device { space_id, .. } => *space_id,
            GeometryRecord::RouteIntent { space_id, .. } => *space_id,
        }
    }

    /// Estimates memory usage in bytes for quota tracking.
    pub fn memory_bytes(&self) -> usize {
        std::mem::size_of::<GeometryRecord>()
            + match self {
                GeometryRecord::Polygon { points_pm, .. } => points_pm.len() * 16,
                GeometryRecord::Device { terminals, params, .. } => {
                    terminals.len() * 32 + params.len() * 32
                }
                _ => 0,
            }
    }
}

/// Standard pure geometry buffer for evaluation and Salsa query caching.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeometryBuffer {
    pub records: Vec<GeometryRecord>,
}

impl GeometryBuffer {
    pub fn new() -> Self {
        Self {
            records: Vec::with_capacity(1024),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            records: Vec::with_capacity(capacity),
        }
    }

    #[inline(always)]
    pub fn push(&mut self, record: GeometryRecord) {
        self.records.push(record);
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn clear(&mut self) {
        self.records.clear();
    }

    /// Computes the total allocated memory in bytes for this buffer.
    pub fn total_memory_bytes(&self) -> usize {
        self.records.len() * std::mem::size_of::<GeometryRecord>()
            + self
                .records
                .iter()
                .map(|r| match r {
                    GeometryRecord::Polygon { points_pm, .. } => points_pm.len() * 16,
                    GeometryRecord::Device { terminals, params, .. } => {
                        terminals.len() * 32 + params.len() * 32
                    }
                    _ => 0,
                })
                .sum::<usize>()
    }

    /// Converts this buffer into a high-density FlatGeometryBuffer.
    pub fn to_flat_buffer(&self, layer_map: impl Fn(&str) -> u16) -> FlatGeometryBuffer {
        let mut flat = FlatGeometryBuffer::new();
        for record in &self.records {
            match record {
                GeometryRecord::Polygon {
                    id,
                    space_id,
                    layer,
                    net_id,
                    points_pm,
                } => {
                    let coord_start_idx = flat.coordinate_pool.len() as u32;
                    for (x, y) in points_pm {
                        flat.coordinate_pool.push(*x);
                        flat.coordinate_pool.push(*y);
                    }
                    let coord_count = points_pm.len() as u32;
                    flat.records.push(CompactGeometryRecordHeader {
                        id: *id,
                        space_id: *space_id,
                        net_id: net_id.unwrap_or(0),
                        layer_idx: layer_map(layer.as_str()),
                        record_type: 1, // Polygon
                        coord_start_idx,
                        coord_count,
                    });
                }
                GeometryRecord::Contact {
                    id,
                    space_id,
                    from_layer,
                    center_pm,
                    diameter_pm,
                    net_id,
                    ..
                } => {
                    let coord_start_idx = flat.coordinate_pool.len() as u32;
                    flat.coordinate_pool.push(center_pm.0);
                    flat.coordinate_pool.push(center_pm.1);
                    flat.coordinate_pool.push(*diameter_pm);
                    flat.records.push(CompactGeometryRecordHeader {
                        id: *id,
                        space_id: *space_id,
                        net_id: net_id.unwrap_or(0),
                        layer_idx: layer_map(from_layer.as_str()),
                        record_type: 2, // Contact
                        coord_start_idx,
                        coord_count: 1,
                    });
                }
                GeometryRecord::Device {
                    id,
                    space_id,
                    ..
                } => {
                    flat.records.push(CompactGeometryRecordHeader {
                        id: *id,
                        space_id: *space_id,
                        net_id: 0,
                        layer_idx: 0,
                        record_type: 3, // Device
                        coord_start_idx: 0,
                        coord_count: 0,
                    });
                }
                GeometryRecord::RouteIntent {
                    id,
                    space_id,
                    from_port,
                    to_port,
                    ..
                } => {
                    let coord_start_idx = flat.coordinate_pool.len() as u32;
                    flat.coordinate_pool.push(from_port.0);
                    flat.coordinate_pool.push(from_port.1);
                    flat.coordinate_pool.push(to_port.0);
                    flat.coordinate_pool.push(to_port.1);
                    flat.records.push(CompactGeometryRecordHeader {
                        id: *id,
                        space_id: *space_id,
                        net_id: 0,
                        layer_idx: 0,
                        record_type: 4, // RouteIntent
                        coord_start_idx,
                        coord_count: 2,
                    });
                }
            }
        }
        flat
    }
}

/// Compact 32-byte header indexing into a contiguous coordinate pool.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CompactGeometryRecordHeader {
    pub id: EntityId,
    pub space_id: SpaceId,
    pub net_id: u32,
    pub layer_idx: u16,
    pub record_type: u8, // Polygon = 1, Contact = 2, Device = 3, RouteIntent = 4
    pub coord_start_idx: u32,
    pub coord_count: u32,
}

/// High-Density Flat-Packed Picometer Coordinate Arena (>100k records).
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlatGeometryBuffer {
    /// Contiguous coordinate pool: [x0, y0, x1, y1, ...]
    pub coordinate_pool: Vec<i64>,
    /// Compact 32-byte header records.
    pub records: Vec<CompactGeometryRecordHeader>,
}

impl FlatGeometryBuffer {
    pub fn new() -> Self {
        Self {
            coordinate_pool: Vec::with_capacity(4096),
            records: Vec::with_capacity(1024),
        }
    }

    pub fn with_capacity(coord_cap: usize, record_cap: usize) -> Self {
        Self {
            coordinate_pool: Vec::with_capacity(coord_cap),
            records: Vec::with_capacity(record_cap),
        }
    }

    #[inline(always)]
    pub fn total_memory_bytes(&self) -> usize {
        self.coordinate_pool.len() * std::mem::size_of::<i64>()
            + self.records.len() * std::mem::size_of::<CompactGeometryRecordHeader>()
    }
}
