//! HardwareScript v0.3.0 DOPHR 3-Stage Guided Routing Integration Test Suite

use hwc_engine::routing::*;
use hwc_types::NetId;
use std::collections::HashMap;

#[test]
fn test_volumetric_tensor_porosity_and_congestion() {
    let mut tensor = VolumetricTensor3D::new(16, 16, 4, 10_000_000, 8, 8);

    // Initial capacity at (4, 4, z)
    for z in 0..4 {
        let idx = tensor.cell_index(4, 4, z);
        assert_eq!(tensor.cap_x[idx], 8);
        assert_eq!(tensor.cap_y[idx], 8);
    }

    // Puncture via from Layer 0 to Layer 2 at G-cell (4, 4)
    // Decrements 3 tracks of capacity on layers 0, 1, 2
    tensor.apply_via_porosity(4, 4, 0, 2, 3);

    for z in 0..=2 {
        let idx = tensor.cell_index(4, 4, z);
        assert_eq!(tensor.cap_x[idx], 5);
        assert_eq!(tensor.cap_y[idx], 5);
    }
    // Layer 3 should be unaffected
    let idx3 = tensor.cell_index(4, 4, 3);
    assert_eq!(tensor.cap_x[idx3], 8);
}

#[test]
fn test_panel_track_assignment_interval_coloring() {
    let assigner = PanelTrackAssigner::new(8, 10_000_000, 4);

    let intervals = vec![
        NetInterval {
            net_id: NetId::new(10),
            start_pos: 0,
            end_pos: 2,
        },
        NetInterval {
            net_id: NetId::new(20),
            start_pos: 1,
            end_pos: 4,
        },
        NetInterval {
            net_id: NetId::new(30),
            start_pos: 3,
            end_pos: 6,
        },
    ];

    let colored = assigner.color_intervals(&intervals, 4);
    assert_eq!(colored.len(), 3);
    assert_eq!(colored[0].0, NetId::new(10));
    assert_eq!(colored[0].1, 0); // Track 0
    assert_eq!(colored[1].0, NetId::new(20));
    assert_eq!(colored[1].1, 1); // Track 1 (overlapping Net 10)
    assert_eq!(colored[2].0, NetId::new(30));
    assert_eq!(colored[2].1, 0); // Track 0 (reused after Net 10 ended at 2)
}

#[test]
fn test_spatial_4_coloring_disjointness() {
    let scheduler = ColorScheduler::new(8, 8, 2);
    let batches = scheduler.partition_cells(0);

    for batch in &batches {
        // Assert no two cells in the same batch share a vertex or boundary
        for i in 0..batch.len() {
            for j in (i + 1)..batch.len() {
                let c1 = batch[i];
                let c2 = batch[j];
                assert!(
                    ColorScheduler::are_cells_independent(c1, c2),
                    "Cells {:?} and {:?} are in the same batch but not spatially independent!",
                    c1,
                    c2
                );
            }
        }
    }
}

#[test]
fn test_dophr_multi_net_synthesis() {
    let config = DophrConfig {
        dim_x: 16,
        dim_y: 16,
        dim_z: 3,
        gcell_size_pm: 10_000_000,
        default_trace_width_pm: 150_000,
        drc_clearance_pm: 150_000,
        tracks_per_cell: 6,
        panel_size: 4,
        global_iterations: 10,
    };

    let engine = DophrEngine::new(config);
    let mut net_terminals = HashMap::new();

    let net_clk = NetId::new(1);
    net_terminals.insert(
        net_clk,
        vec![
            DetailedTerminal {
                net_id: net_clk,
                layer: 0,
                x_pm: 2_000_000,
                y_pm: 2_000_000,
            },
            DetailedTerminal {
                net_id: net_clk,
                layer: 0,
                x_pm: 50_000_000,
                y_pm: 50_000_000,
            },
        ],
    );

    let net_data = NetId::new(2);
    net_terminals.insert(
        net_data,
        vec![
            DetailedTerminal {
                net_id: net_data,
                layer: 1,
                x_pm: 5_000_000,
                y_pm: 30_000_000,
            },
            DetailedTerminal {
                net_id: net_data,
                layer: 1,
                x_pm: 80_000_000,
                y_pm: 30_000_000,
            },
        ],
    );

    let result = engine.route_all(&net_terminals).expect("DOPHR route_all must succeed");

    assert!(result.routed_segments.contains_key(&net_clk));
    assert!(result.routed_segments.contains_key(&net_data));
    assert!(result.total_wirelength_pm > 0);
    assert!(!result.guides.is_empty());
}
