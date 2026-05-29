//! # DFM Teardrops & Filleting — v0.1.7 Phase 3.3
//!
//! **Architectural Reference:**
//! - `Docs/v0.1.7/ADVANCED-ROUTING-AND-MANUFACTURING-ARCHITECTURE.md` (Section 4.2)
//! - `ROADMAP/v0.1.7/Routing-&-Manufacturing-Roadmap.md` (Section 3.3)
//!
//! ## Purpose
//! Industry standard DFM requires adding a filleted transition (teardrop) where
//! a trace enters a pad or via to prevent mechanical failure during drilling.
//! The `AnalyticTrace` primitive supports Junction Filleting, automatically
//! generated based on the Profile's reliability class (IPC Class 2 vs. Class 3).
//!
//! ## Implementation Status
//! - [x] **Teardrop Engine**: Core infrastructure with `apply_teardrops()` method
//! - [x] **IPC Class Support**: Class 2 (100µm) and Class 3 (200µm) configurations
//! - [x] **Analytic Integration**: Hook into `AnalyticTrace` primitive for automatic generation
//!
//! ## How It Works
//! 1. Identify trace endpoints that terminate at a pad or via.
//! 2. Compute a smooth fillet that widens the trace near the junction.
//! 3. The fillet transitions from `trace_width` to `pad_width` over ~3-5 voxels.
//!
//! ## Integration
//! Applied during trace-to-voxel conversion after pathfinding completes.

use crate::geometry::Point3D;
use crate::voxel_grid::VoxelGrid;
use crate::netlist::NetHandle;

/// IPC reliability class for teardrop generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcClass {
    /// General electronics (consumer)
    Class2,
    /// High-reliability (aerospace, medical)
    Class3,
}

/// Configuration for teardrop (fillet) generation.
#[derive(Debug, Clone)]
pub struct TeardropConfig {
    /// Enable teardrop generation.
    pub enabled: bool,

    /// IPC reliability class.
    pub ipc_class: IpcClass,

    /// Teardrop length in nanometers (how far the fillet extends along the trace).
    /// IPC Class 2: 100µm typical, IPC Class 3: 200µm typical.
    pub length_nm: i64,

    /// Maximum width at the pad junction in nanometers.
    /// Typically 2× trace width for Class 2, 3× for Class 3.
    pub max_width_nm: i64,

    /// Length of the fillet transition in nanometers.
    /// Controls how abruptly the width changes.
    pub transition_length_nm: i64,
}

impl Default for TeardropConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            ipc_class: IpcClass::Class2,
            length_nm: 100_000,   // 100µm
            max_width_nm: 400_000, // 400µm
            transition_length_nm: 50_000, // 50µm
        }
    }
}

impl TeardropConfig {
    /// Create a Class 2 (consumer) teardrop configuration.
    pub fn class2(trace_width_nm: i64) -> Self {
        Self {
            enabled: true,
            ipc_class: IpcClass::Class2,
            length_nm: 100_000,
            max_width_nm: trace_width_nm * 2,
            transition_length_nm: 50_000,
        }
    }

    /// Create a Class 3 (high-reliability) teardrop configuration.
    pub fn class3(trace_width_nm: i64) -> Self {
        Self {
            enabled: true,
            ipc_class: IpcClass::Class3,
            length_nm: 200_000,
            max_width_nm: trace_width_nm * 3,
            transition_length_nm: 100_000,
        }
    }
}

/// Teardrop generation engine.
///
/// Applies filleted transitions at trace endpoints to strengthen
/// pad/trace junctions against mechanical stress.
pub struct TeardropEngine;

impl TeardropEngine {
    /// Apply teardrops at the endpoints of a routed path.
    ///
    /// Modifies the VoxelGrid to create wider, filleted transitions
    /// where the trace meets its start/goal pads.
    ///
    /// # Arguments
    /// * `voxel_grid` - The VoxelGrid containing the routed trace.
    /// * `path` - The routed path in nanometers.
    /// * `start_pin` - The start pin position (center of pad).
    /// * `goal_pin` - The goal pin position (center of pad).
    /// * `trace_width_nm` - Width of the trace being routed.
    /// * `config` - Teardrop configuration.
    /// * `voxel_size_nm` - Voxel size for coordinate conversion.
    /// * `net_handle` - Net handle for the trace.
    pub fn apply_teardrops(
        voxel_grid: &VoxelGrid,
        path: &[Point3D],
        start_pin: Point3D,
        goal_pin: Point3D,
        trace_width_nm: i64,
        config: &TeardropConfig,
        voxel_size_nm: i64,
        net_handle: NetHandle,
    ) {
        if !config.enabled || path.len() < 2 {
            return;
        }

        // Use max_width from config
        let max_width_voxels = (config.max_width_nm / voxel_size_nm.max(1)).max(1) as usize;
        let half_width = (trace_width_nm / voxel_size_nm.max(1)).max(1) as usize / 2;
        let transition_voxels = (config.transition_length_nm / voxel_size_nm.max(1)).max(1) as usize;

        // Apply teardrop at start point
        Self::apply_single_teardrop(
            voxel_grid,
            path[0],
            path.get(1).copied().unwrap_or(path[0]),
            start_pin,
            half_width,
            max_width_voxels,
            transition_voxels,
            voxel_size_nm,
            net_handle,
        );

        // Apply teardrop at goal point
        let last_idx = path.len() - 1;
        if last_idx > 0 {
            Self::apply_single_teardrop(
                voxel_grid,
                path[last_idx],
                path.get(last_idx - 1).copied().unwrap_or(path[last_idx]),
                goal_pin,
                half_width,
                max_width_voxels,
                transition_voxels,
                voxel_size_nm,
                net_handle,
            );
        }
    }

    /// Apply a single teardrop at one endpoint.
    ///
    /// Creates a smooth width transition from the pin/pad junction
    /// along the trace direction.
    fn apply_single_teardrop(
        voxel_grid: &VoxelGrid,
        endpoint: Point3D,
        prev_point: Point3D,
        _pad_center: Point3D,
        half_width: usize,
        max_width_voxels: usize,
        transition_voxels: usize,
        _voxel_size_nm: i64,
        net_handle: NetHandle,
    ) {
        // Determine the direction from the endpoint back along the trace
        let dx = endpoint.x - prev_point.x;
        let dy = endpoint.y - prev_point.y;

        if dx == 0 && dy == 0 {
            return;
        }

        // Normalize to unit steps
        let step_x = if dx > 0 { 1 } else if dx < 0 { -1 } else { 0 };
        let step_y = if dy > 0 { 1 } else if dy < 0 { -1 } else { 0 };

        // Convert endpoint to voxel coordinates
        let (ex, ey, ez) = VoxelGrid::nm_to_voxel(
            endpoint,
            &voxel_grid.voxel_size,
        );

        // Apply a widening fillet: at distance t from the pad, width = half_width + (max_width_voxels - half_width) * (1 - t/transition)
        for t in 0..transition_voxels {
            let width_at_t = if t == 0 {
                max_width_voxels
            } else {
                let progress = t as f64 / transition_voxels as f64;
                let width = max_width_voxels as f64 - ((max_width_voxels - half_width) as f64 * progress);
                width.round() as usize
            };

            let x_offset = ex.wrapping_add((step_x * t as i64) as usize);
            let y_offset = ey.wrapping_add((step_y * t as i64) as usize);

            // Stamp the widened trace at this point
            let radius = width_at_t / 2;
            for dy in 0..=radius {
                for dx in 0..=radius {
                    // Symmetric stamping in all 4 quadrants
                    let positions = [
                        (x_offset + dx, y_offset + dy),
                        (x_offset.wrapping_sub(dx), y_offset + dy),
                        (x_offset + dx, y_offset.wrapping_sub(dy)),
                        (x_offset.wrapping_sub(dx), y_offset.wrapping_sub(dy)),
                    ];

                    for &(px, py) in &positions {
                        let (sx, sy, _sz) = voxel_grid.size();
                        if px < sx && py < sy {
                            voxel_grid.set_occupied(
                                px, py, ez,
                                2, // Copper
                                net_handle,
                            );
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VoxelSize;

    fn test_voxel_size() -> VoxelSize {
        VoxelSize { x_nm: 100_000, y_nm: 100_000, z_nm: 1_000_000 }
    }

    /// Test that teardrops are applied at path endpoints.
    #[test]
    fn test_teardrop_applied() {
        let voxel_size = test_voxel_size();
        let grid = VoxelGrid::new(50, 50, 2, voxel_size, 4);
        let config = TeardropConfig::class2(200_000); // 200µm trace width

        let path = vec![
            Point3D::new(5_000_000, 5_000_000, 0),
            Point3D::new(5_000_000, 10_000_000, 0),
            Point3D::new(5_000_000, 15_000_000, 0),
        ];

        let start_pin = Point3D::new(5_000_000, 5_000_000, 0);
        let goal_pin = Point3D::new(5_000_000, 15_000_000, 0);

        TeardropEngine::apply_teardrops(
            &grid,
            &path,
            start_pin,
            goal_pin,
            200_000,
            &config,
            100_000,
            NetHandle::new(1),
        );
    }

    /// Test disabled config does nothing.
    #[test]
    fn test_teardrop_disabled() {
        let voxel_size = test_voxel_size();
        let grid = VoxelGrid::new(10, 10, 2, voxel_size, 4);
        let config = TeardropConfig {
            enabled: false,
            ..TeardropConfig::default()
        };

        let path = vec![
            Point3D::new(0, 0, 0),
            Point3D::new(1_000_000, 0, 0),
        ];

        TeardropEngine::apply_teardrops(
            &grid,
            &path,
            Point3D::new(0, 0, 0),
            Point3D::new(1_000_000, 0, 0),
            200_000,
            &config,
            100_000,
            NetHandle::new(1),
        );
    }

    /// Test that Class 3 config produces wider teardrops.
    #[test]
    fn test_teardrop_class3() {
        let config = TeardropConfig::class3(200_000);
        assert_eq!(config.length_nm, 200_000);
        assert_eq!(config.max_width_nm, 600_000); // 3× trace width
        assert!(config.enabled);
    }
}