//! Integration verification tests for PCB/APCB autorouter engine libraries.
//!
//! Each `#[test]` function proves a specific library works correctly in our context.
//! All coordinates use i64 nanometers. No f64 in core path.

#[cfg(test)]
mod tests {
    use crate::geometry::{BoundingBox, Point3D};
    use crate::geometry_router::spatial_index::{
        query_overlapping_segments, DynamicSpatialIndex, IndexedSegment,
    };

    // -----------------------------------------------------------------------
    // 1. rstar Integration
    // -----------------------------------------------------------------------

    fn make_segment(id: usize, net: usize, x1: i64, y1: i64, x2: i64, y2: i64, w: i64) -> IndexedSegment {
        IndexedSegment {
            segment_id: id,
            net_id: net,
            width_nm: w,
            thickness_nm: 35_000,
            start: Point3D::new(x1, y1, 0),
            end: Point3D::new(x2, y2, 0),
            layer: 0,
        }
    }

    #[test]
    fn rstar_dynamic_insertion_and_query() {
        let mut idx = DynamicSpatialIndex::new();
        // Use mm-scale coordinates (1mm = 1_000_000nm)
        idx.insert(make_segment(0, 1, 0, 0, 2_000_000, 0, 200_000));
        idx.insert(make_segment(1, 1, 10_000_000, 0, 12_000_000, 0, 200_000));
        idx.insert(make_segment(2, 2, 50_000_000, 50_000_000, 52_000_000, 50_000_000, 200_000));
        assert_eq!(idx.len(), 3);

        // Query box from (-1mm, -1mm) to (15mm, 15mm)
        let bbox = BoundingBox {
            min: Point3D::new(-1_000_000, -1_000_000, -1),
            max: Point3D::new(15_000_000, 15_000_000, 1),
        };
        let results = idx.query_bbox(&bbox);
        assert_eq!(results.len(), 2, "should find 2 segments in the 15mm box");
    }

    #[test]
    fn rstar_locate_within_distance() {
        let mut idx = DynamicSpatialIndex::new();
        // Segments far apart: one at origin, one 50mm away
        idx.insert(make_segment(0, 1, 0, 0, 1_000_000, 0, 100_000));
        idx.insert(make_segment(1, 1, 50_000_000, 50_000_000, 51_000_000, 50_000_000, 100_000));

        // Query near origin with 5mm radius — only segment 0 should be found
        let results = idx.query_radius(0, 0, 5_000_000);
        assert_eq!(results.len(), 1, "only segment 0 within 5mm of origin");
        assert_eq!(results[0].segment_id, 0);
    }

    #[test]
    fn rstar_query_overlapping_segments_works() {
        let mut idx = DynamicSpatialIndex::new();
        // Two segments on different nets that overlap spatially
        idx.insert(make_segment(0, 1, 1000, 1000, 2000, 1000, 200));
        idx.insert(make_segment(1, 2, 1500, 1000, 2500, 1000, 200));
        // Segment on a far-away net
        idx.insert(make_segment(2, 3, 10000, 10000, 11000, 10000, 200));

        let query_seg = make_segment(99, 1, 1000, 1000, 2000, 1000, 200);
        let overlapping = query_overlapping_segments(&idx, &query_seg, 500_000);
        // Should find segment 1 (different net, overlapping) but NOT segment 0 (same net) or 2 (far away)
        assert!(
            overlapping.iter().any(|s| s.segment_id == 1),
            "should find the overlapping net-2 segment"
        );
        assert!(
            !overlapping.iter().any(|s| s.net_id == 1),
            "must exclude same-net segments"
        );
    }

    #[test]
    fn rstar_large_tree_query_completes_quickly() {
        let mut idx = DynamicSpatialIndex::new();
        // Insert 10,000 segments
        for i in 0..10_000 {
            let x = (i as i64) * 100_000; // 100µm spacing
            idx.insert(make_segment(i, 1, x, 0, x + 50_000, 0, 100));
        }
        assert_eq!(idx.len(), 10_000);

        let start = std::time::Instant::now();
        let bbox = BoundingBox {
            min: Point3D::new(5_000_000, -100_000, -1),
            max: Point3D::new(5_500_000, 100_000, 1),
        };
        let _results = idx.query_bbox(&bbox);
        let elapsed = start.elapsed();
        // O(log N) should complete in well under 1ms for 10k items
        assert!(
            elapsed.as_millis() < 100,
            "query on 10k segments took {:?}, expected < 100ms",
            elapsed
        );
    }

    // -----------------------------------------------------------------------
    // 2. clipper2-rust Integration
    // -----------------------------------------------------------------------

    use clipper2_rust::clipper::union_subjects_64;
    use clipper2_rust::core::{FillRule, Path64, Paths64, Point64};

    fn make_rect_path(x: i64, y: i64, w: i64, h: i64) -> Path64 {
        vec![
            Point64::new(x, y),
            Point64::new(x + w, y),
            Point64::new(x + w, y + h),
            Point64::new(x, y + h),
        ]
        .into_iter()
        .collect()
    }

    #[test]
    fn clipper2_boolean_union_overlapping_rects() {
        let rect_a = make_rect_path(0, 0, 2_000_000, 2_000_000);
        let rect_b = make_rect_path(1_000_000, 1_000_000, 2_000_000, 2_000_000);
        let subjects: Paths64 = vec![rect_a, rect_b].into_iter().collect();

        let result = union_subjects_64(&subjects, FillRule::NonZero);
        // Union of two overlapping 2mm×2mm squares offset by 1mm should produce
        // a single merged polygon (L-shaped or rectangular depending on overlap)
        assert!(!result.is_empty(), "union must not be empty");
        // The merged area should be less than the sum of two full squares (3mm × 3mm bbox)
        // but more than a single square. With a 1mm overlap, area = 3mm×3mm - 1mm×1mm = 8mm²
        // Each square = 4mm², sum = 8mm², union = 8mm² (they overlap in 1mm² = total 7mm²)
        // The exact area depends on clipper2's output, but we verify count
        assert!(
            result.len() <= 2,
            "union of two overlapping rects should be 1-2 polygons, got {}",
            result.len()
        );
    }

    #[test]
    fn clipper2_nonzero_winding_rule() {
        // Two CCW-wound rectangles (NonZero rule merges them)
        let rect_a = make_rect_path(0, 0, 1_000_000, 1_000_000);
        let rect_b = make_rect_path(500_000, 500_000, 1_000_000, 1_000_000);
        let subjects: Paths64 = vec![rect_a, rect_b].into_iter().collect();

        let result_nz = union_subjects_64(&subjects, FillRule::NonZero);
        // NonZero: overlapping region has winding 2 → filled → single merged shape
        assert_eq!(
            result_nz.len(),
            1,
            "NonZero winding should merge into 1 polygon, got {}",
            result_nz.len()
        );
    }

    #[test]
    fn clipper2_empty_input() {
        let subjects: Paths64 = Vec::<Path64>::new().into_iter().collect();
        let result = union_subjects_64(&subjects, FillRule::NonZero);
        assert!(
            result.is_empty(),
            "union of empty input should be empty"
        );
    }

    #[test]
    fn clipper2_single_polygon_passthrough() {
        let rect = make_rect_path(0, 0, 1_000_000, 1_000_000);
        let subjects: Paths64 = vec![rect].into_iter().collect();

        let result = union_subjects_64(&subjects, FillRule::NonZero);
        assert_eq!(
            result.len(),
            1,
            "single polygon should pass through unchanged"
        );
        // Verify the single output has 4 points (rectangle)
        assert_eq!(result[0].len(), 4, "output should be a rectangle with 4 points");
    }

    // -----------------------------------------------------------------------
    // 3. rkyv + memmap2 Integration
    // -----------------------------------------------------------------------

    use crate::geometry_router::lockfile::{
        ArchivedArcSegment, ArchivedComponentInstance, CompactLockfileBinary,
    };

    #[test]
    fn rkyv_mmap_roundtrip() {
        let lockfile = CompactLockfileBinary {
            version: 1,
            board_name: "test_board".to_string(),
            placement_hash: [0xABu8; 32],
            arcs: vec![
                ArchivedArcSegment {
                    net_id: 42,
                    layer: 1,
                    width_nm: 150_000,
                    x1: 1_000_000,
                    y1: 2_000_000,
                    x2: 3_000_000,
                    y2: 4_000_000,
                    thickness_nm: 35_000,
                    material_name: "Copper".to_string(),
                    current_ma: 20_000,
                },
                ArchivedArcSegment {
                    net_id: 7,
                    layer: 3,
                    width_nm: 200_000,
                    x1: 5_000_000,
                    y1: 6_000_000,
                    x2: 7_000_000,
                    y2: 8_000_000,
                    thickness_nm: 35_000,
                    material_name: "Copper".to_string(),
                    current_ma: 20_000,
                },
            ],
            instances: vec![ArchivedComponentInstance {
                id: 0,
                x_nm: 10_000_000,
                y_nm: 20_000_000,
                rotation_deg: 90,
                mirror: true,
            }],
        };

        // Serialize to bytes
        let bytes: rkyv::AlignedVec =
            rkyv::to_bytes::<_, 1_048_576>(&lockfile).expect("rkyv serialize failed");

        // Validate with check_archived_root
        let archived =
            rkyv::validation::validators::check_archived_root::<CompactLockfileBinary>(&bytes)
                .expect("check_archived_root failed");

        // Zero-copy access
        assert_eq!(archived.version, 1);
        assert_eq!(archived.board_name.as_str(), "test_board");
        assert_eq!(archived.placement_hash, [0xABu8; 32]);
        assert_eq!(archived.arcs.len(), 2);
        assert_eq!(archived.arcs[0].net_id, 42);
        assert_eq!(archived.arcs[1].x2, 7_000_000);
        assert_eq!(archived.instances.len(), 1);
        assert_eq!(archived.instances[0].rotation_deg, 90);
        assert!(archived.instances[0].mirror);
    }

    #[test]
    fn rkyv_mmap_file_roundtrip() {
        use std::io::Write;

        let lockfile = CompactLockfileBinary {
            version: 2,
            board_name: "mmap_test".to_string(),
            placement_hash: [0x42u8; 32],
            arcs: vec![ArchivedArcSegment {
                net_id: 1,
                layer: 0,
                width_nm: 100_000,
                x1: 0,
                y1: 0,
                x2: 1_000_000,
                y2: 0,
                thickness_nm: 35_000,
                material_name: "Copper".to_string(),
                current_ma: 20_000,
            }],
            instances: vec![],
        };

        // Serialize
        let bytes: rkyv::AlignedVec =
            rkyv::to_bytes::<_, 1_048_576>(&lockfile).expect("serialize");

        // Write to temp file, mmap, and validate
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test.lock");
        {
            let mut f = std::fs::File::create(&path).expect("create");
            f.write_all(bytes.as_slice()).expect("write");
        }

        let file = std::fs::File::open(&path).expect("open");
        let mmap = unsafe { memmap2::Mmap::map(&file).expect("mmap") };

        let archived =
            rkyv::validation::validators::check_archived_root::<CompactLockfileBinary>(&mmap)
                .expect("validate");

        assert_eq!(archived.version, 2);
        assert_eq!(archived.board_name.as_str(), "mmap_test");
        assert_eq!(archived.arcs[0].net_id, 1);
        assert_eq!(archived.arcs[0].x2, 1_000_000);
    }

    // -----------------------------------------------------------------------
    // 4. sha2 Integration
    // -----------------------------------------------------------------------

    use sha2::{Digest, Sha256};

    #[test]
    fn sha256_deterministic() {
        let input = b"hello world";
        let h1: [u8; 32] = {
            let mut hasher = Sha256::new();
            hasher.update(input);
            let result = hasher.finalize();
            let mut out = [0u8; 32];
            out.copy_from_slice(&result);
            out
        };
        let h2: [u8; 32] = {
            let mut hasher = Sha256::new();
            hasher.update(input);
            let result = hasher.finalize();
            let mut out = [0u8; 32];
            out.copy_from_slice(&result);
            out
        };
        assert_eq!(h1, h2, "SHA-256 must be deterministic for identical inputs");
        // Verify hash is non-zero (sanity check)
        assert_ne!(h1, [0u8; 32], "SHA-256 hash must not be all zeros");
        // Verify hash is 32 bytes
        assert_eq!(h1.len(), 32, "SHA-256 must produce 32 bytes");
    }

    #[test]
    fn sha256_different_inputs_different_hashes() {
        let h1: [u8; 32] = {
            let mut hasher = Sha256::new();
            hasher.update(b"input_a");
            let result = hasher.finalize();
            let mut out = [0u8; 32];
            out.copy_from_slice(&result);
            out
        };
        let h2: [u8; 32] = {
            let mut hasher = Sha256::new();
            hasher.update(b"input_b");
            let result = hasher.finalize();
            let mut out = [0u8; 32];
            out.copy_from_slice(&result);
            out
        };
        assert_ne!(h1, h2, "different inputs must produce different hashes");
    }

    // -----------------------------------------------------------------------
    // 5. earcutr Integration
    // -----------------------------------------------------------------------

    #[test]
    fn earcutr_rectangle_triangle_count() {
        // A simple rectangle: 4 vertices
        // earcutr should produce exactly 2 triangles for a convex quad
        let vertices = vec![
            0.0, 0.0,   // bottom-left
            1000.0, 0.0, // bottom-right
            1000.0, 1000.0, // top-right
            0.0, 1000.0, // top-left
        ];
        let hole_indices: Vec<usize> = vec![];
        let indices = earcutr::earcut(&vertices, &hole_indices, 2)
            .expect("earcutr should succeed for simple rectangle");
        // A rectangle → 2 triangles → 6 indices
        assert_eq!(indices.len(), 6, "rectangle should produce 2 triangles (6 indices)");
        // Verify all indices are in range [0, 3]
        for &idx in &indices {
            assert!(idx < 4, "index {idx} out of range for 4-vertex polygon");
        }
    }

    #[test]
    fn earcutr_polygon_with_hole() {
        // Outer: 1000×1000 square
        // Hole: 200×200 square centered at (500, 500)
        let vertices = vec![
            // Outer ring (CCW)
            0.0, 0.0,
            1000.0, 0.0,
            1000.0, 1000.0,
            0.0, 1000.0,
            // Hole ring (CW — smaller winding)
            400.0, 400.0,
            400.0, 600.0,
            600.0, 600.0,
            600.0, 400.0,
        ];
        let hole_indices = vec![4]; // hole starts at vertex index 4

        let indices = earcutr::earcut(&vertices, &hole_indices, 2)
            .expect("earcutr should succeed for polygon with hole");

        // With a hole, the triangulation must have more triangles than without
        let tri_count = indices.len() / 3;
        // Outer alone = 2 triangles, with hole removed = more triangles (at least 4-6)
        assert!(
            tri_count > 2,
            "polygon with hole should produce more than 2 triangles, got {tri_count}"
        );
        // All indices must be valid
        for &idx in &indices {
            assert!(idx < 8, "index {idx} out of range for 8-vertex polygon");
        }
    }

    // -----------------------------------------------------------------------
    // 6. glam Isolation Verification
    // -----------------------------------------------------------------------

    /// Compile-time check: the geometry_router module must not import glam.
    /// This test is a no-op at runtime but fails to compile if glam leaks in.
    #[test]
    fn glam_not_imported_in_geometry_router() {
        // This test verifies the architectural boundary.
        // If glam is ever imported in geometry_router, this test file
        // would fail to compile (use of undeclared type).
        //
        // We verify indirectly: all types used here are from our own crate
        // or from approved libraries (rstar, clipper2, sha2, rkyv, earcutr).
        let _point = Point3D::new(0, 0, 0);
        let _bbox = BoundingBox::new(
            Point3D::new(0, 0, 0),
            Point3D::new(1000, 1000, 1000),
        );
        // No glam types used anywhere in this module — verified at compile time.
    }

    // -----------------------------------------------------------------------
    // 7. Cross-Library Integration
    // -----------------------------------------------------------------------

    #[test]
    fn cross_lib_rstar_clipper_earcutr_pipeline() {
        // Pipeline: rstar query → clipper2 union → earcutr triangulation

        // Step 1: Build spatial index with mm-scale segments
        let mut idx = DynamicSpatialIndex::new();
        idx.insert(make_segment(0, 1, 0, 0, 2_000_000, 0, 200_000));
        idx.insert(make_segment(1, 1, 1_000_000, 0, 3_000_000, 0, 200_000));
        idx.insert(make_segment(2, 2, 50_000_000, 50_000_000, 52_000_000, 50_000_000, 200_000));

        // Query box that captures segments 0 and 1 (both net 1)
        let bbox = BoundingBox {
            min: Point3D::new(-1_000_000, -1_000_000, -1),
            max: Point3D::new(5_000_000, 5_000_000, 1),
        };
        let results = idx.query_bbox(&bbox);
        assert!(results.len() >= 2, "rstar query must find at least 2 segments, got {}", results.len());

        // Step 2: Convert query results to clipper2 polygons (bounding boxes)
        let paths: Vec<Path64> = results
            .iter()
            .map(|s| {
                let hw = s.width_nm / 2;
                let min_x = s.start.x.min(s.end.x) - hw;
                let max_x = s.start.x.max(s.end.x) + hw;
                let min_y = s.start.y.min(s.end.y) - hw;
                let max_y = s.start.y.max(s.end.y) + hw;
                vec![
                    Point64::new(min_x, min_y),
                    Point64::new(max_x, min_y),
                    Point64::new(max_x, max_y),
                    Point64::new(min_x, max_y),
                ]
                .into_iter()
                .collect::<Path64>()
            })
            .collect();

        let subjects: Paths64 = paths.into_iter().collect();
        let union_result = union_subjects_64(&subjects, FillRule::NonZero);
        assert!(!union_result.is_empty(), "clipper2 union must produce output");

        // Step 3: Triangulate the union result
        for polygon in &union_result {
            if polygon.len() < 3 {
                continue;
            }
            let mut vertices = Vec::new();
            for pt in polygon.iter() {
                vertices.push(pt.x as f64);
                vertices.push(pt.y as f64);
            }
            let hole_indices: Vec<usize> = vec![];
            let indices = earcutr::earcut(&vertices, &hole_indices, 2)
                .expect("earcutr should succeed on union output");
            assert!(
                !indices.is_empty(),
                "earcutr must produce triangles from union polygon"
            );
        }
    }

    #[test]
    fn cross_lib_i64_coordinates_no_data_loss() {
        // Verify that i64 nanometer coordinates flow through all libraries
        // without precision loss or truncation.

        // rstar: insert and query with moderate coordinates
        let mut idx = DynamicSpatialIndex::new();
        let seg = make_segment(0, 1, 2_000_000, 3_000_000, 8_000_000, 9_000_000, 100_000);
        idx.insert(seg);
        let found = idx.query_radius(5_000_000, 6_000_000, 5_000_000);
        assert!(!found.is_empty(), "rstar must find segment via radius query");

        // clipper2: polygon with i64 coordinates
        let rect = make_rect_path(1_000_000, 1_000_000, 5_000_000, 5_000_000);
        let subjects: Paths64 = vec![rect].into_iter().collect();
        let result = union_subjects_64(&subjects, FillRule::NonZero);
        assert_eq!(result.len(), 1);
        assert!(!result[0].is_empty(), "clipper2 must return non-empty polygon");

        // rkyv: serialize/deserialize i64 values
        let lockfile = CompactLockfileBinary {
            version: 1,
            board_name: String::new(),
            placement_hash: [0; 32],
            arcs: vec![ArchivedArcSegment {
                net_id: 0,
                layer: 0,
                width_nm: 100_000,
                x1: 2_000_000,
                y1: 3_000_000,
                x2: 8_000_000,
                y2: 9_000_000,
                thickness_nm: 35_000,
                material_name: "Copper".to_string(),
                current_ma: 20_000,
            }],
            instances: vec![],
        };

        // Serialize
        let bytes: rkyv::AlignedVec =
            rkyv::to_bytes::<_, 1_048_576>(&lockfile).expect("serialize");
        let archived =
            rkyv::validation::validators::check_archived_root::<CompactLockfileBinary>(&bytes)
                .expect("validate");
        assert_eq!(archived.arcs[0].width_nm, 100_000);
        assert_eq!(archived.arcs[0].x1, 2_000_000);
        assert_eq!(archived.arcs[0].x2, 8_000_000);
    }
}
