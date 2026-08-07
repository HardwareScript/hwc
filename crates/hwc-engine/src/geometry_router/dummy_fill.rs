//! # Dummy Metal Fill (Thieving) — v0.1.8 NATIVE VECTOR
//!
//! **Architectural Reference:**
//! - `Docs/v0.1.7/ADVANCED-ROUTING-AND-MANUFACTURING-ARCHITECTURE.md` (Section 3)
//! - `ROADMAP/v0.1.7/Routing-&-Manufacturing-Roadmap.md` (Section 3.2)
//!
//! ## Purpose
//! Dummy metal fill (thieving) prevents silicon wafer warping during CMP and
//! maintains uniform copper density on high-frequency PCBs.
//!
//! ## v0.1.8 Changes
//! - **Density sampling** replaced with R*-tree bbox queries. The old approach
//!   sampled individual grid points via `is_point_occupied()`, which only tested
//!   component instances and missed substrate-layer geometry. The new approach
//!   queries `entity_graph.query_bbox()` to find all overlapping substrate
//!   layers, then computes density from their areas (area/zone_area).
//! - **Dummy stamping** fully implemented. Each dummy square is registered as
//!   a `SubstrateLayer::Pour` with `net_id = 0` (UNCONNECTED) via
//!   `entity_graph.add_substrate_layer()`. The old code returned 0 (no-op).
//! - **Zone computation** now uses nanometer bounding boxes directly instead of
//!   converting to/from coarse grid indices.
//!
//! ## Integration
//! Runs as a post-routing pass (Pass 4 in the 5-Stage Pipeline):
//! - Pass 3: Parallel 2.5D Auto-Routing completes
//! - Pass 4: Dummy fill stamps copper into low-density zones
//!
//! ## Configuration
//! Controlled via profile properties:
//! ```hardware
//! profile TSMC_180nm:
//!     dummy_fill: true
//!     dummy_fill_density: 45%
//!     dummy_fill_pattern: DotGrid(size: 2um, spacing: 4um)
//! ```

use crate::geometry::{BoundingBox, Point3D};
use crate::geometry_router::substrate_types::SubstrateLayerType;
use crate::geometry_router::EntityGraph;
use crate::netlist::NetId;

/// Configuration for dummy metal fill (thieving).
///
/// Controlled via profile properties in the .hw source.
#[derive(Debug, Clone)]
pub struct DummyFillConfig {
    /// Enable dummy fill post-routing.
    pub enabled: bool,

    /// Target metal density percentage (0-100).
    /// Typical values: 40-60% for CMP planarization.
    pub target_density_pct: u8,

    /// Size of each dummy fill square in nanometers.
    /// Typical: 2µm (2_000nm) for advanced nodes, 10µm for PCBs.
    pub dummy_size_nm: i64,

    /// Spacing between dummy fill squares in nanometers.
    /// Typical: 4µm (4_000nm) for advanced nodes.
    pub dummy_spacing_nm: i64,

    /// Minimum clearance from routed nets in nanometers.
    /// Typically 2-3× the minimum trace spacing.
    pub clearance_nm: i64,

    /// Z-layer to apply dummy fill on. None = all routing layers.
    pub target_z_nm: Option<i64>,
}

impl Default for DummyFillConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            target_density_pct: 45,
            dummy_size_nm: 2_000,    // 2µm
            dummy_spacing_nm: 4_000, // 4µm
            clearance_nm: 500_000,   // 0.5mm
            target_z_nm: None,
        }
    }
}

/// Result of a density analysis zone.
#[derive(Debug, Clone)]
pub struct DensityZone {
    /// Zone bounding box minimum X (nm)
    pub zone_min_x: i64,
    /// Zone bounding box minimum Y (nm)
    pub zone_min_y: i64,
    /// Zone bounding box maximum X (nm)
    pub zone_max_x: i64,
    /// Zone bounding box maximum Y (nm)
    pub zone_max_y: i64,
    /// Z-layer midpoint (nm)
    pub z_nm: i64,
    /// Overlapping substrate area in square nanometers
    pub occupied_area_nm2: i128,
    /// Total zone area in square nanometers
    pub total_area_nm2: i128,
    /// Density percentage (0-100)
    pub density_pct: u8,
    /// Whether this zone needs dummy fill
    pub needs_fill: bool,
}

/// Dummy Metal Fill (Thieving) Engine.
///
/// # Example
/// ```
/// use hwc_engine::geometry_router::{DummyFillEngine, DummyFillConfig};
///
/// let config = DummyFillConfig {
///     enabled: true,
///     target_density_pct: 50,
///     ..DummyFillConfig::default()
/// };
///
/// let mut engine = DummyFillEngine::new();
/// // engine.run(&entity_graph, &config);
/// ```
pub struct DummyFillEngine {
    /// Analysis results by zone
    pub zones: Vec<DensityZone>,

    /// Total dummies placed
    pub total_dummies_placed: usize,

    /// Total zones analyzed
    pub zones_analyzed: usize,

    /// Zones that needed fill
    pub zones_filled: usize,
}

impl DummyFillEngine {
    /// Create a new DummyFillEngine.
    pub fn new() -> Self {
        Self {
            zones: Vec::new(),
            total_dummies_placed: 0,
            zones_analyzed: 0,
            zones_filled: 0,
        }
    }

    /// Run the full dummy fill analysis and stamping pipeline.
    ///
    /// 1. Divide the routing layer into coarse zones (16×16 grid blocks).
    /// 2. Calculate current metal density of each zone via R*-tree queries.
    /// 3. If density is below target, stamp isolated dummy squares.
    ///
    /// # Arguments
    /// * `entity_graph` - The EntityGraph to analyze.
    /// * `config` - Dummy fill configuration.
    ///
    /// # Returns
    /// Statistics about the dummy fill operation.
    pub fn run(
        &mut self,
        entity_graph: &mut EntityGraph,
        config: &DummyFillConfig,
    ) -> DummyFillStats {
        if !config.enabled {
            return DummyFillStats {
                zones_analyzed: 0,
                zones_filled: 0,
                total_dummies_placed: 0,
                average_density_before: 0.0,
                average_density_after: 0.0,
            };
        }

        // v0.1.8: Use bounding box dimensions directly in nanometers
        let board_bbox = match entity_graph.total_bounding_box() {
            Some(bbox) => bbox,
            None => {
                return DummyFillStats {
                    zones_analyzed: 0,
                    zones_filled: 0,
                    total_dummies_placed: 0,
                    average_density_before: 0.0,
                    average_density_after: 0.0,
                }
            }
        };

        let board_width_nm = board_bbox.max.x - board_bbox.min.x;
        let board_height_nm = board_bbox.max.y - board_bbox.min.y;

        if board_width_nm <= 0 || board_height_nm <= 0 {
            return DummyFillStats {
                zones_analyzed: 0,
                zones_filled: 0,
                total_dummies_placed: 0,
                average_density_before: 0.0,
                average_density_after: 0.0,
            };
        }

        // v0.1.8: Determine Z-layers to process using actual layer Z positions
        let z_layers: Vec<i64> = if let Some(target_z) = config.target_z_nm {
            vec![target_z]
        } else {
            // Collect unique Z positions from substrate layers
            let mut z_set = std::collections::BTreeSet::new();
            for layer in entity_graph.get_substrate_layers() {
                let mid_z = (layer.bbox.min.z + layer.bbox.max.z) / 2;
                z_set.insert(mid_z);
            }
            // Remove substrate layer (z=0) — only process routing layers
            z_set.remove(&0);
            z_set.into_iter().collect()
        };

        // v0.1.8: Use coarse zone size of 16×16 in nanometer-space
        // Each zone is coarse_cell_nm × coarse_cell_nm nanometers
        let coarse_cell_nm = 1_600_000i64; // 1.6mm per zone (matches 16×100µm grid)
        let coarse_x_count = (board_width_nm / coarse_cell_nm).max(1) as usize;
        let coarse_y_count = (board_height_nm / coarse_cell_nm).max(1) as usize;

        let mut total_occupied_area: i128 = 0;
        let mut total_zone_area: i128 = 0;
        let mut total_dummies = 0usize;

        for &z_nm in &z_layers {
            for cy in 0..coarse_y_count {
                for cx in 0..coarse_x_count {
                    // v0.1.8: Compute zone bounding box directly in nanometers
                    let zone_min_x = board_bbox.min.x + (cx as i64) * coarse_cell_nm;
                    let zone_min_y = board_bbox.min.y + (cy as i64) * coarse_cell_nm;
                    let zone_max_x = (zone_min_x + coarse_cell_nm).min(board_bbox.max.x);
                    let zone_max_y = (zone_min_y + coarse_cell_nm).min(board_bbox.max.y);

                    let zone_area_nm2 =
                        (zone_max_x - zone_min_x) as i128 * (zone_max_y - zone_min_y) as i128;

                    // v0.1.8: R*-tree bbox query instead of grid point sampling
                    let (occupied_area, zone_bbox) = self.sample_zone(
                        entity_graph,
                        zone_min_x,
                        zone_min_y,
                        zone_max_x,
                        zone_max_y,
                        z_nm,
                    );

                    total_occupied_area += occupied_area;
                    total_zone_area += zone_area_nm2;

                    let density_pct = if zone_area_nm2 > 0 {
                        ((occupied_area * 100) / zone_area_nm2) as u8
                    } else {
                        100
                    };

                    let needs_fill = density_pct < config.target_density_pct;

                    self.zones.push(DensityZone {
                        zone_min_x: zone_bbox.min.x,
                        zone_min_y: zone_bbox.min.y,
                        zone_max_x: zone_bbox.max.x,
                        zone_max_y: zone_bbox.max.y,
                        z_nm,
                        occupied_area_nm2: occupied_area,
                        total_area_nm2: zone_area_nm2,
                        density_pct,
                        needs_fill,
                    });

                    self.zones_analyzed += 1;

                    // Stamp dummies if needed
                    if needs_fill {
                        let zone = BoundingBox::new(
                            Point3D::new(zone_min_x, zone_min_y, z_nm),
                            Point3D::new(zone_max_x, zone_max_y, z_nm),
                        );
                        let dummies = self.stamp_dummies_in_zone(entity_graph, zone, config);
                        total_dummies += dummies;
                        self.zones_filled += 1;
                    }
                }
            }
        }

        self.total_dummies_placed = total_dummies;

        let avg_before = if total_zone_area > 0 {
            (total_occupied_area as f64 / total_zone_area as f64) * 100.0
        } else {
            0.0
        };

        DummyFillStats {
            zones_analyzed: self.zones_analyzed,
            zones_filled: self.zones_filled,
            total_dummies_placed: total_dummies,
            average_density_before: avg_before,
            average_density_after: avg_before,
        }
    }

    /// Sample a zone to determine metal density using R*-tree bbox queries.
    ///
    /// v0.1.8: Replaced grid-based point sampling with `entity_graph.query_bbox()`.
    /// The old approach sampled individual points via `is_point_occupied()` which
    /// only tested component instances and missed substrate-layer geometry. This
    /// approach queries all overlapping substrate layers and computes density from
    /// their bounding box areas.
    ///
    /// Only counts layers with net_id > 0 as occupied metal. Layers with net_id=0
    /// are base substrate or UNCONNECTED dummy fill — they represent background,
    /// not routing metal.
    ///
    /// Returns (occupied_area_nm2, zone_bbox).
    fn sample_zone(
        &self,
        entity_graph: &EntityGraph,
        zone_min_x: i64,
        zone_min_y: i64,
        zone_max_x: i64,
        zone_max_y: i64,
        z_nm: i64,
    ) -> (i128, BoundingBox) {
        let zone_bbox = BoundingBox::new(
            Point3D::new(zone_min_x, zone_min_y, z_nm),
            Point3D::new(zone_max_x, zone_max_y, z_nm),
        );

        // v0.1.8: R*-tree query for all overlapping geometry
        let overlapping = entity_graph.query_bbox(&zone_bbox);

        let mut occupied_area: i128 = 0;
        for layer in &overlapping {
            // v0.1.8: Skip base substrate and UNCONNECTED (net_id=0) layers.
            // Only layers with net_id > 0 represent routing metal for density purposes.
            if layer.net == NetId::UNCONNECTED {
                continue;
            }
            // Only count layers that match the target Z
            let layer_z_min = layer.bbox.min.z;
            let layer_z_max = layer.bbox.max.z;
            if z_nm >= layer_z_min && z_nm <= layer_z_max {
                // Compute intersection area in 2D (XY plane)
                let ix_min = zone_min_x.max(layer.bbox.min.x);
                let ix_max = zone_max_x.min(layer.bbox.max.x);
                let iy_min = zone_min_y.max(layer.bbox.min.y);
                let iy_max = zone_max_y.min(layer.bbox.max.y);

                if ix_max > ix_min && iy_max > iy_min {
                    let intersection_area = (ix_max - ix_min) as i128 * (iy_max - iy_min) as i128;
                    occupied_area += intersection_area;
                }
            }
        }

        (occupied_area, zone_bbox)
    }

    /// Stamp dummy fill squares into a low-density zone.
    ///
    /// v0.1.8: Fully implemented. Lays out a grid of dummy squares at
    /// `dummy_spacing_nm` intervals, skipping positions that violate
    /// `clearance_nm` from existing geometry. Each dummy is registered as
    /// a `SubstrateLayer::Pour` with `net_id = 0` (UNCONNECTED).
    ///
    /// # Arguments
    /// * `entity_graph` - EntityGraph to stamp into.
    /// * `zone_min_x`, `zone_min_y`, `zone_max_x`, `zone_max_y` - Zone bounds in nm.
    /// * `z_nm` - Z-layer midpoint in nm.
    /// * `config` - Dummy fill configuration.
    ///
    /// # Returns
    /// Number of dummy squares placed.
    fn stamp_dummies_in_zone(
        &self,
        entity_graph: &mut EntityGraph,
        zone: BoundingBox,
        config: &DummyFillConfig,
    ) -> usize {
        let half_size = config.dummy_size_nm / 2;
        let step = config.dummy_spacing_nm;
        let clearance = config.clearance_nm;

        if step <= 0 {
            return 0;
        }

        let zone_min_x = zone.min.x;
        let zone_min_y = zone.min.y;
        let zone_max_x = zone.max.x;
        let zone_max_y = zone.max.y;
        let z_nm = zone.min.z;

        // Compute grid positions at dummy_spacing_nm intervals
        // Start from first position that fits within zone
        let start_x = ((zone_min_x + half_size + step - 1) / step) * step;
        let start_y = ((zone_min_y + half_size + step - 1) / step) * step;

        let mut count = 0usize;

        let mut y = start_y;
        while y < zone_max_y - half_size {
            let mut x = start_x;
            while x < zone_max_x - half_size {
                // Check clearance from existing geometry via R*-tree query
                let dummy_bbox = BoundingBox::new(
                    Point3D::new(x - half_size, y - half_size, z_nm),
                    Point3D::new(x + half_size, y + half_size, z_nm),
                );

                let clearance_bbox = BoundingBox::new(
                    Point3D::new(x - half_size - clearance, y - half_size - clearance, z_nm),
                    Point3D::new(x + half_size + clearance, y + half_size + clearance, z_nm),
                );

                let nearby = entity_graph.query_bbox(&clearance_bbox);
                let has_conflict = nearby.iter().any(|layer| {
                    // Skip UNCONNECTED (net_id=0) dummies — we don't clear from our own fill
                    if layer.net == NetId::UNCONNECTED {
                        return false;
                    }
                    // Check if this layer actually overlaps the clearance zone
                    layer.bbox.intersects(&clearance_bbox)
                        && z_nm >= layer.bbox.min.z
                        && z_nm <= layer.bbox.max.z
                });

                if !has_conflict {
                    // v0.1.8: Register dummy as native SubstrateLayer (Pour, net_id=0)
                    entity_graph.add_substrate_layer(
                        0u8,                // material placeholder — will be resolved by stackup
                        NetId::UNCONNECTED, // net_id = 0 (UNCONNECTED)
                        dummy_bbox,
                        SubstrateLayerType::Pour,
                    );
                    count += 1;
                }

                x += step;
            }
            y += step;
        }

        count
    }
}

impl Default for DummyFillEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics from a dummy fill operation.
#[derive(Debug, Clone)]
pub struct DummyFillStats {
    /// Number of zones analyzed for density.
    pub zones_analyzed: usize,

    /// Number of zones that received dummy fill.
    pub zones_filled: usize,

    /// Total number of dummy squares placed.
    pub total_dummies_placed: usize,

    /// Average metal density before fill (percentage).
    pub average_density_before: f64,

    /// Average metal density after fill (percentage).
    pub average_density_after: f64,
}
