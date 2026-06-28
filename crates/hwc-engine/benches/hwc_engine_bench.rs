use criterion::{criterion_group, criterion_main, Criterion};
use hwc_engine::geometry::{Point3D, TraceSegment};
use hwc_engine::geometry_router::spatial_index::{DynamicSpatialIndex, IndexedSegment};
use hwc_engine::geometry_router::gcell_sweep::{find_overlaps, sort_segments_by_morton};
use hwc_engine::geometry_router::connectivity_check::verify_connectivity;
use hwc_engine::geometry_router::parasitic_extraction::{
    extract_parasitics, ExtractionParams,
};
use hwc_engine::geometry_router::legalizer::Legalizer;
use hwc_engine::geometry_router::deterministic_sort::deterministic_toposort;
use hwc_engine::geometry_router::lockfile::{
    write_lockfile, load_lockfile, CompactLockfileBinary, ArchivedArcSegment,
    ArchivedComponentInstance,
};

// ---------------------------------------------------------------------------
// Deterministic pseudo-random generator (no external deps)
// ---------------------------------------------------------------------------

struct Rng {
    state: u64,
}

impl Rng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.state
    }

    fn gen_i64(&mut self, max: i64) -> i64 {
        (self.next_u64() as i64).abs() % max
    }

    fn gen_range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + self.gen_i64(hi - lo)
    }
}

// ---------------------------------------------------------------------------
// Synthetic data generators
// ---------------------------------------------------------------------------

fn make_horizontal_segment(
    rng: &mut Rng,
    segment_id: usize,
    net_id: usize,
    board_size: i64,
    max_width: i64,
) -> IndexedSegment {
    let x1 = rng.gen_range(0, board_size);
    let y1 = rng.gen_range(0, board_size);
    let len = rng.gen_range(1_000_000, 10_000_000);
    let width_nm = rng.gen_range(50_000, max_width);
    IndexedSegment {
        segment_id,
        net_id,
        width_nm,
        thickness_nm: 35_000,
        start: Point3D::new(x1, y1, 1),
        end: Point3D::new(x1 + len, y1, 1),
        layer: 1,
    }
}

fn make_segments(count: usize, board_size: i64, seed: u64) -> Vec<IndexedSegment> {
    let mut rng = Rng::new(seed);
    (0..count)
        .map(|i| make_horizontal_segment(&mut rng, i, i % 20, board_size, 200_000))
        .collect()
}

// ---------------------------------------------------------------------------
// 1. Spatial Index Benchmarks
// ---------------------------------------------------------------------------

fn bench_spatial_index_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("spatial_index");
    let segments: Vec<IndexedSegment> = {
        let mut rng = Rng::new(0xDEAD);
        (0..1000)
            .map(|i| make_horizontal_segment(&mut rng, i, i % 50, 100_000_000, 200_000))
            .collect()
    };

    group.bench_function("insert_1000", |b| {
        b.iter(|| {
            let mut index = DynamicSpatialIndex::new();
            for seg in &segments {
                index.insert(seg.clone());
            }
            index.len()
        });
    });

    group.finish();
}

fn bench_spatial_index_query(c: &mut Criterion) {
    let mut group = c.benchmark_group("spatial_index");
    let mut rng = Rng::new(0xCAFE);

    let mut index = DynamicSpatialIndex::new();
    for i in 0..1000 {
        let seg = make_horizontal_segment(&mut rng, i, i % 50, 100_000_000, 200_000);
        index.insert(seg);
    }

    let query_points: Vec<(i64, i64)> = (0..100)
        .map(|_| (rng.gen_range(0, 100_000_000), rng.gen_range(0, 100_000_000)))
        .collect();

    group.bench_function("query_100_segments", |b| {
        b.iter(|| {
            let mut count = 0usize;
            for &(x, y) in &query_points {
                let results = index.query_radius(x, y, 5_000_000);
                count += results.len();
            }
            count
        });
    });

    group.finish();
}

fn bench_spatial_index_batch_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("spatial_index");
    let segments: Vec<IndexedSegment> = {
        let mut rng = Rng::new(0xBEEF);
        (0..10_000)
            .map(|i| make_horizontal_segment(&mut rng, i, i % 100, 100_000_000, 200_000))
            .collect()
    };

    group.bench_function("insert_10000", |b| {
        b.iter(|| {
            let mut index = DynamicSpatialIndex::new();
            for seg in &segments {
                index.insert(seg.clone());
            }
            index.len()
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 2. DRC Sweep Benchmarks
// ---------------------------------------------------------------------------

fn bench_gcell_sweep_small(c: &mut Criterion) {
    let mut group = c.benchmark_group("gcell_sweep");
    let segments = make_segments(50, 10_000_000, 0x1234);

    group.bench_function("50_segments_single_gcell", |b| {
        b.iter(|| {
            let mut sorted = segments.clone();
            sort_segments_by_morton(&mut sorted);
            find_overlaps(&sorted)
        });
    });

    group.finish();
}

fn bench_gcell_sweep_medium(c: &mut Criterion) {
    let mut group = c.benchmark_group("gcell_sweep");
    let segments = make_segments(500, 100_000_000, 0x5678);

    group.bench_function("500_segments_multi_gcell", |b| {
        b.iter(|| {
            let mut sorted = segments.clone();
            sort_segments_by_morton(&mut sorted);
            find_overlaps(&sorted)
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 3. Connectivity Check Benchmarks
// ---------------------------------------------------------------------------

fn make_connectivity_segments(
    net_count: usize,
    segs_per_net: usize,
    seed: u64,
) -> Vec<IndexedSegment> {
    let mut rng = Rng::new(seed);
    let mut segments = Vec::with_capacity(net_count * segs_per_net);
    let board_size = 50_000_000;

    for net in 0..net_count {
        let mut x = rng.gen_range(0, board_size);
        let mut y = rng.gen_range(0, board_size);
        for s in 0..segs_per_net {
            let len = rng.gen_range(500_000, 3_000_000);
            let horizontal = rng.next_u64() % 2 == 0;
            let (end_x, end_y) = if horizontal {
                (x + len, y)
            } else {
                (x, y + len)
            };
            segments.push(IndexedSegment {
                segment_id: net * segs_per_net + s,
                net_id: net,
                width_nm: 200_000,
                thickness_nm: 35_000,
                start: Point3D::new(x, y, 1),
                end: Point3D::new(end_x, end_y, 1),
                layer: 1,
            });
            x = end_x;
            y = end_y;
        }
    }
    segments
}

fn bench_connectivity_small(c: &mut Criterion) {
    let mut group = c.benchmark_group("connectivity");
    let segments = make_connectivity_segments(10, 5, 0xAAAA);

    group.bench_function("10_nets_50_segments", |b| {
        b.iter(|| {
            verify_connectivity(&segments, &[])
        });
    });

    group.finish();
}

fn bench_connectivity_medium(c: &mut Criterion) {
    let mut group = c.benchmark_group("connectivity");
    let segments = make_connectivity_segments(100, 5, 0xBBBB);

    group.bench_function("100_nets_500_segments", |b| {
        b.iter(|| {
            verify_connectivity(&segments, &[])
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 4. Parasitic Extraction Benchmarks
// ---------------------------------------------------------------------------

fn bench_parasitic_extraction(c: &mut Criterion) {
    let mut group = c.benchmark_group("parasitic_extraction");
    let mut rng = Rng::new(0xCCCC);
    let board_size = 50_000_000;

    let segments: Vec<IndexedSegment> = (0..100)
        .map(|i| make_horizontal_segment(&mut rng, i, i % 10, board_size, 200_000))
        .collect();

    let params = ExtractionParams {
        freq_hz: 1.0e9,
        substrate_er: 4.5,
        substrate_height_m: 35.0e-6,
        trace_thickness_m: 35.0e-6,
        loss_tangent: 0.02,
    };

    group.bench_function("100_traces", |b| {
        b.iter(|| {
            extract_parasitics(&segments, &[], &params)
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 5. Legalization Benchmarks
// ---------------------------------------------------------------------------

fn bench_legalization_small(c: &mut Criterion) {
    let mut group = c.benchmark_group("legalization");
    let mut rng = Rng::new(0xDDDD);
    let legalizer = Legalizer::new(200_000);

    let mut segments: Vec<TraceSegment> = Vec::with_capacity(40);
    let mut x = 0i64;
    for _i in 0..20 {
        let width_nm = 200_000;
        let seg = TraceSegment {
            start: Point3D::new(x, 0, 1),
            end: Point3D::new(x + 500_000, 0, 1),
            width_nm,
        };
        segments.push(seg);
        // Overlapping: shift by less than width to create violations
        x += rng.gen_range(100_000, 300_000);
    }
    // Add vertical overlapping segments
    let mut y = 0i64;
    for _i in 0..20 {
        let seg = TraceSegment {
            start: Point3D::new(y, 0, 1),
            end: Point3D::new(y, 500_000, 1),
            width_nm: 200_000,
        };
        segments.push(seg);
        y += rng.gen_range(50_000, 150_000);
    }

    group.bench_function("20_violations", |b| {
        b.iter(|| {
            let mut index = DynamicSpatialIndex::new();
            for (idx, seg) in segments.iter().enumerate() {
                index.insert(IndexedSegment::new(
                    idx,
                    idx,
                    seg,
                    seg.start.z,
                ));
            }
            legalizer.legalize(&segments, &index, 3)
        });
    });

    group.finish();
}

fn bench_legalization_medium(c: &mut Criterion) {
    let mut group = c.benchmark_group("legalization");
    let mut rng = Rng::new(0xEEEE);
    let legalizer = Legalizer::new(200_000);

    let mut segments: Vec<TraceSegment> = Vec::with_capacity(400);
    let mut x = 0i64;
    for _i in 0..200 {
        let width_nm = 200_000;
        let seg = TraceSegment {
            start: Point3D::new(x, 0, 1),
            end: Point3D::new(x + 500_000, 0, 1),
            width_nm,
        };
        segments.push(seg);
        x += rng.gen_range(100_000, 300_000);
    }
    let mut y = 0i64;
    for _i in 0..200 {
        let seg = TraceSegment {
            start: Point3D::new(y, 0, 1),
            end: Point3D::new(y, 500_000, 1),
            width_nm: 200_000,
        };
        segments.push(seg);
        y += rng.gen_range(50_000, 150_000);
    }

    group.bench_function("200_violations", |b| {
        b.iter(|| {
            let mut index = DynamicSpatialIndex::new();
            for (idx, seg) in segments.iter().enumerate() {
                index.insert(IndexedSegment::new(
                    idx,
                    idx,
                    seg,
                    seg.start.z,
                ));
            }
            legalizer.legalize(&segments, &index, 3)
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 6. Deterministic Sort Benchmarks
// ---------------------------------------------------------------------------

fn make_dag(node_count: usize, seed: u64) -> Vec<(u64, Vec<u64>)> {
    let mut rng = Rng::new(seed);
    let mut nodes: Vec<(u64, Vec<u64>)> = Vec::with_capacity(node_count);

    for i in 0..node_count as u64 {
        let dep_count = (rng.next_u64() % 4) as usize;
        let mut deps = Vec::with_capacity(dep_count);
        for _ in 0..dep_count {
            let dep = rng.gen_range(0, node_count as i64) as u64;
            if dep < i {
                deps.push(dep);
            }
        }
        deps.sort_unstable();
        deps.dedup();
        nodes.push((i, deps));
    }
    nodes
}

fn bench_toposort_small(c: &mut Criterion) {
    let mut group = c.benchmark_group("toposort");
    let nodes = make_dag(50, 0xF001);

    group.bench_function("50_nodes", |b| {
        b.iter(|| {
            deterministic_toposort(&nodes)
        });
    });

    group.finish();
}

fn bench_toposort_medium(c: &mut Criterion) {
    let mut group = c.benchmark_group("toposort");
    let nodes = make_dag(500, 0xF002);

    group.bench_function("500_nodes", |b| {
        b.iter(|| {
            deterministic_toposort(&nodes)
        });
    });

    group.finish();
}

fn bench_toposort_large(c: &mut Criterion) {
    let mut group = c.benchmark_group("toposort");
    let nodes = make_dag(5000, 0xF003);

    group.bench_function("5000_nodes", |b| {
        b.iter(|| {
            deterministic_toposort(&nodes)
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 7. Lockfile Benchmarks
// ---------------------------------------------------------------------------

fn make_lockfile(arcs_count: usize) -> CompactLockfileBinary {
    let mut rng = Rng::new(0xABCD);
    let board_size = 100_000_000;

    let arcs: Vec<ArchivedArcSegment> = (0..arcs_count as u32)
        .map(|i| {
            let x1 = rng.gen_range(0, board_size);
            let y1 = rng.gen_range(0, board_size);
            let x2 = rng.gen_range(0, board_size);
            let y2 = rng.gen_range(0, board_size);
            ArchivedArcSegment {
                net_id: i,
                layer: (rng.next_u64() % 6) as u16,
                width_nm: rng.gen_range(50_000, 300_000),
                x1,
                y1,
                x2,
                y2,
                thickness_nm: 35_000,
                material_name: "Copper".to_string(),
                current_ma: 20_000,
            }
        })
        .collect();

    let instances: Vec<ArchivedComponentInstance> = (0..100)
        .map(|i| {
            ArchivedComponentInstance {
                id: i,
                x_nm: rng.gen_range(0, board_size),
                y_nm: rng.gen_range(0, board_size),
                rotation_deg: (rng.next_u64() % 4) as i64 * 90,
                mirror: rng.next_u64() % 2 == 0,
            }
        })
        .collect();

    CompactLockfileBinary {
        version: 1,
        board_name: "bench_board".into(),
        placement_hash: [0x42u8; 32],
        arcs,
        instances,
    }
}

fn bench_lockfile_write(c: &mut Criterion) {
    let mut group = c.benchmark_group("lockfile");
    let lockfile = make_lockfile(1000);

    let dir = tempfile::tempdir().ok();
    let path = dir
        .as_ref()
        .map(|d| d.path().join("bench_write.lock"))
        .unwrap_or_else(|| std::path::PathBuf::from("bench_write.lock"));

    group.bench_function("write_1000_arcs", |b| {
        b.iter(|| {
            let _ = write_lockfile(&lockfile, &path);
        });
    });

    group.finish();
}

fn bench_lockfile_read(c: &mut Criterion) {
    let mut group = c.benchmark_group("lockfile");
    let lockfile = make_lockfile(1000);

    let dir = tempfile::tempdir().ok();
    let path = dir
        .as_ref()
        .map(|d| d.path().join("bench_read.lock"))
        .unwrap_or_else(|| std::path::PathBuf::from("bench_read.lock"));

    let _ = write_lockfile(&lockfile, &path);

    group.bench_function("read_1000_arcs_mmap", |b| {
        b.iter(|| {
            let loaded = load_lockfile(&path);
            let data = loaded.ok().map(|d| {
                let arcs = d.data().arcs.len();
                let instances = d.data().instances.len();
                arcs + instances
            });
            data
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Criterion group and main
// ---------------------------------------------------------------------------

criterion_group!(
    benches,
    bench_spatial_index_insert,
    bench_spatial_index_query,
    bench_spatial_index_batch_insert,
    bench_gcell_sweep_small,
    bench_gcell_sweep_medium,
    bench_connectivity_small,
    bench_connectivity_medium,
    bench_parasitic_extraction,
    bench_legalization_small,
    bench_legalization_medium,
    bench_toposort_small,
    bench_toposort_medium,
    bench_toposort_large,
    bench_lockfile_write,
    bench_lockfile_read,
);

criterion_main!(benches);
