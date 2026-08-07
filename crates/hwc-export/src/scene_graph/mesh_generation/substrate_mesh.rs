//! Substrate Mesh Generation Engine with Via Cutouts
//!
//! **v0.2.2**: Production-grade, zero-panic substrate mesh generation.
//! Replaces legacy grid subdivision with Clipper2 2D CSG and Earcut triangulation.

use crate::scene_graph::types::{Face, MeshNode, Vertex};
use clipper2_rust::{boolean_op_64, ClipType, FillRule, Path64, Paths64, Point64};
use earcut::Earcut;
use hwc_engine::geometry::BoundingBox;
use hwc_engine::SpaceView;
use std::f64::consts::PI;

/// Validated via cutout (can be circular or polygonal for IC design)
#[derive(Debug, Clone)]
pub enum ViaCutout {
    Circular {
        center_x_nm: i64,
        center_y_nm: i64,
        diameter_nm: i64,
        z_min_nm: i64,
        z_max_nm: i64,
    },
    Polygonal {
        /// Polygon contour in world space (nanometer coordinates)
        contour: clipper2_rust::Path64,
        z_min_nm: i64,
        z_max_nm: i64,
    },
}

impl ViaCutout {
    /// Create a circular via cutout with validation
    pub fn new_circular(
        center_x_nm: i64,
        center_y_nm: i64,
        diameter_nm: i64,
        z_min_nm: i64,
        z_max_nm: i64,
    ) -> Result<Self, SubstrateMeshError> {
        if diameter_nm <= 0 {
            return Err(SubstrateMeshError::InvalidDiameter(diameter_nm));
        }
        if z_max_nm <= z_min_nm {
            return Err(SubstrateMeshError::InvalidZRange {
                z_min: z_min_nm,
                z_max: z_max_nm,
            });
        }

        Ok(Self::Circular {
            center_x_nm,
            center_y_nm,
            diameter_nm,
            z_min_nm,
            z_max_nm,
        })
    }

    /// Create a polygonal via cutout (for square/rectangular IC vias)
    pub fn new_polygonal(
        contour: clipper2_rust::Path64,
        z_min_nm: i64,
        z_max_nm: i64,
    ) -> Result<Self, SubstrateMeshError> {
        if contour.len() < 3 {
            return Err(SubstrateMeshError::DegeneratePolygon);
        }
        if z_max_nm <= z_min_nm {
            return Err(SubstrateMeshError::InvalidZRange {
                z_min: z_min_nm,
                z_max: z_max_nm,
            });
        }

        Ok(Self::Polygonal {
            contour,
            z_min_nm,
            z_max_nm,
        })
    }

    /// Check if this via intersects a Z-range
    #[inline]
    pub fn intersects_z(&self, z_min: i64, z_max: i64) -> bool {
        let (via_z_min, via_z_max) = match self {
            ViaCutout::Circular {
                z_min_nm, z_max_nm, ..
            } => (*z_min_nm, *z_max_nm),
            ViaCutout::Polygonal {
                z_min_nm, z_max_nm, ..
            } => (*z_min_nm, *z_max_nm),
        };
        !(via_z_max <= z_min || via_z_min >= z_max)
    }

    /// Check if this via intersects an XY bounding box
    #[inline]
    pub fn intersects_xy(&self, bbox: &BoundingBox) -> bool {
        match self {
            ViaCutout::Circular {
                center_x_nm,
                center_y_nm,
                diameter_nm,
                ..
            } => {
                let r = diameter_nm / 2;
                !(center_x_nm + r <= bbox.min.x
                    || center_x_nm - r >= bbox.max.x
                    || center_y_nm + r <= bbox.min.y
                    || center_y_nm - r >= bbox.max.y)
            }
            ViaCutout::Polygonal { contour, .. } => {
                // Check if any polygon point is inside or near the bbox
                contour.iter().any(|pt| {
                    pt.x >= bbox.min.x - 1000
                        && pt.x <= bbox.max.x + 1000
                        && pt.y >= bbox.min.y - 1000
                        && pt.y <= bbox.max.y + 1000
                })
            }
        }
    }
}

/// Errors during substrate mesh generation
#[derive(Debug, thiserror::Error)]
pub enum SubstrateMeshError {
    #[error("Invalid via diameter: {0}nm (must be > 0)")]
    InvalidDiameter(i64),
    #[error("Invalid Z range: z_min={z_min}nm, z_max={z_max}nm")]
    InvalidZRange { z_min: i64, z_max: i64 },
    #[error("Degenerate substrate bounds: {0:?}")]
    DegenerateBounds(BoundingBox),
    #[error("Degenerate polygon (less than 3 vertices)")]
    DegeneratePolygon,
}

/// Builder for constructing validated substrate mesh with vias
pub struct SubstrateMeshBuilder {
    bbox: BoundingBox,
    material_name: String,
    view: SpaceView,
    vias: Vec<ViaCutout>,
    segments: usize,
}

impl SubstrateMeshBuilder {
    pub fn new(bbox: BoundingBox, material_name: impl Into<String>, view: SpaceView) -> Self {
        Self {
            bbox,
            material_name: material_name.into(),
            view,
            vias: Vec::new(),
            segments: 32,
        }
    }

    pub fn with_via(mut self, via: ViaCutout) -> Self {
        self.vias.push(via);
        self
    }

    pub fn with_vias(mut self, vias: Vec<ViaCutout>) -> Self {
        self.vias.extend(vias);
        self
    }

    pub fn set_circle_segments(mut self, segments: usize) -> Self {
        self.segments = segments.max(8);
        self
    }

    pub fn build(self, name: &str) -> Result<MeshNode, SubstrateMeshError> {
        let width = self.bbox.max.x - self.bbox.min.x;
        let height = self.bbox.max.y - self.bbox.min.y;
        let depth = self.bbox.max.z - self.bbox.min.z;

        if width <= 0 || height <= 0 || depth <= 0 {
            return Err(SubstrateMeshError::DegenerateBounds(self.bbox));
        }

        let active_vias: Vec<&ViaCutout> = self
            .vias
            .iter()
            .filter(|v| {
                v.intersects_z(self.bbox.min.z, self.bbox.max.z) && v.intersects_xy(&self.bbox)
            })
            .collect();

        if active_vias.is_empty() {
            return Ok(generate_solid_box(
                name,
                &self.bbox,
                &self.material_name,
                self.view,
            ));
        }

        generate_substrate_with_vias(
            name,
            &self.bbox,
            &active_vias,
            self.segments,
            &self.material_name,
            self.view,
        )
    }
}

fn generate_solid_box(name: &str, bbox: &BoundingBox, mat: &str, view: SpaceView) -> MeshNode {
    let x1 = bbox.min.x as f64 / 1_000_000.0;
    let y1 = bbox.min.y as f64 / 1_000_000.0;
    let z1 = bbox.min.z as f64 / 1_000_000.0;
    let x2 = bbox.max.x as f64 / 1_000_000.0;
    let y2 = bbox.max.y as f64 / 1_000_000.0;
    let z2 = bbox.max.z as f64 / 1_000_000.0;

    let map_v = |x: f64, y: f64, z: f64| -> Vertex {
        match view {
            SpaceView::Horizontal => Vertex { x, y: z, z: y },
            SpaceView::Vertical => Vertex { x, y, z },
        }
    };

    let vertices = vec![
        map_v(x1, y1, z1),
        map_v(x2, y1, z1),
        map_v(x2, y2, z1),
        map_v(x1, y2, z1),
        map_v(x1, y1, z2),
        map_v(x2, y1, z2),
        map_v(x2, y2, z2),
        map_v(x1, y2, z2),
    ];

    let faces = vec![
        Face {
            vertices: vec![0, 3, 2, 1],
        },
        Face {
            vertices: vec![4, 5, 6, 7],
        },
        Face {
            vertices: vec![0, 1, 5, 4],
        },
        Face {
            vertices: vec![1, 2, 6, 5],
        },
        Face {
            vertices: vec![2, 3, 7, 6],
        },
        Face {
            vertices: vec![3, 0, 4, 7],
        },
    ];

    MeshNode {
        name: name.into(),
        vertices,
        faces,
        material_name: mat.into(),
    }
}

fn generate_substrate_with_vias(
    name: &str,
    bbox: &BoundingBox,
    vias: &[&ViaCutout],
    segments: usize,
    mat: &str,
    view: SpaceView,
) -> Result<MeshNode, SubstrateMeshError> {
    let map_v = |x_mm: f64, y_mm: f64, z_mm: f64| -> Vertex {
        match view {
            SpaceView::Horizontal => Vertex {
                x: x_mm,
                y: z_mm,
                z: y_mm,
            },
            SpaceView::Vertical => Vertex {
                x: x_mm,
                y: y_mm,
                z: z_mm,
            },
        }
    };

    // 1. Build Clipper2 Subject (Substrate Boundary)
    let mut subject = Paths64::new();
    let mut rect_path = Path64::new();
    rect_path.push(Point64::new(bbox.min.x, bbox.min.y));
    rect_path.push(Point64::new(bbox.max.x, bbox.min.y));
    rect_path.push(Point64::new(bbox.max.x, bbox.max.y));
    rect_path.push(Point64::new(bbox.min.x, bbox.max.y));
    subject.push(rect_path);

    // 2. Build Clipper2 Clips (Via Circles AND Polygons)
    let mut clips = Paths64::new();
    for via in vias {
        match via {
            ViaCutout::Circular {
                center_x_nm,
                center_y_nm,
                diameter_nm,
                ..
            } => {
                let r_nm = diameter_nm / 2;
                let mut circle_path = Path64::new();
                for i in 0..segments {
                    let angle = (i as f64 / segments as f64) * 2.0 * PI;
                    let cx = center_x_nm + (angle.cos() * r_nm as f64) as i64;
                    let cy = center_y_nm + (angle.sin() * r_nm as f64) as i64;
                    circle_path.push(Point64::new(cx, cy));
                }
                clips.push(circle_path);
            }
            ViaCutout::Polygonal { contour, .. } => {
                // Polygonal via - use the contour directly
                clips.push(contour.clone());
            }
        }
    }

    // 3. Perform Boolean Difference: subject - clips
    let diff_result = boolean_op_64(ClipType::Difference, FillRule::NonZero, &subject, &clips);

    // Convert to mm
    let mut outer_contour_mm: Vec<(f64, f64)> = Vec::new();
    let mut hole_contours_mm: Vec<Vec<(f64, f64)>> = Vec::new();

    for path in diff_result {
        let points: Vec<(f64, f64)> = path
            .iter()
            .map(|pt| (pt.x as f64 / 1_000_000.0, pt.y as f64 / 1_000_000.0))
            .collect();

        if clipper2_rust::is_positive(&path) {
            outer_contour_mm = points;
        } else {
            hole_contours_mm.push(points);
        }
    }

    if outer_contour_mm.len() < 3 {
        return Ok(generate_solid_box(name, bbox, mat, view));
    }

    // 4. Flatten for Earcut
    let mut flat_coords: Vec<f64> = Vec::new();
    let mut hole_indices: Vec<usize> = Vec::new();

    for &(x, y) in &outer_contour_mm {
        flat_coords.push(x);
        flat_coords.push(y);
    }

    for hole in &hole_contours_mm {
        hole_indices.push(flat_coords.len() / 2);
        for &(x, y) in hole {
            flat_coords.push(x);
            flat_coords.push(y);
        }
    }

    // 5. Triangulate with Earcut
    let mut triangulator = Earcut::new();
    let mut cap_indices: Vec<usize> = Vec::new();
    let data = flat_coords.chunks_exact(2).map(|c| [c[0], c[1]]);
    triangulator.earcut(data, &hole_indices, &mut cap_indices);

    let z_min_mm = bbox.min.z as f64 / 1_000_000.0;
    let z_max_mm = bbox.max.z as f64 / 1_000_000.0;
    let vert_count_2d = flat_coords.len() / 2;

    let mut vertices = Vec::with_capacity(vert_count_2d * 2);
    let mut faces = Vec::new();

    // Create bottom and top vertices
    for i in 0..vert_count_2d {
        let x = flat_coords[i * 2];
        let y = flat_coords[i * 2 + 1];
        vertices.push(map_v(x, y, z_min_mm));
        vertices.push(map_v(x, y, z_max_mm));
    }

    // 6. Top and Bottom Caps
    for chunk in cap_indices.chunks_exact(3) {
        let (v0, v1, v2) = (chunk[0], chunk[1], chunk[2]);
        faces.push(Face {
            vertices: vec![v0 * 2, v2 * 2, v1 * 2],
        });
        faces.push(Face {
            vertices: vec![v0 * 2 + 1, v1 * 2 + 1, v2 * 2 + 1],
        });
    }

    // 7. Walls (Outer and Inner)
    let mut ring_starts = vec![0];
    ring_starts.extend(hole_indices.iter().cloned());
    ring_starts.push(vert_count_2d);

    for r in 0..(ring_starts.len() - 1) {
        let start = ring_starts[r];
        let end = ring_starts[r + 1];
        let count = end - start;
        let is_inner_hole = r > 0;

        for i in 0..count {
            let curr = start + i;
            let next = start + (i + 1) % count;
            let b_curr = curr * 2;
            let t_curr = curr * 2 + 1;
            let b_next = next * 2;
            let t_next = next * 2 + 1;

            if is_inner_hole {
                faces.push(Face {
                    vertices: vec![b_curr, t_curr, t_next, b_next],
                });
            } else {
                faces.push(Face {
                    vertices: vec![b_curr, b_next, t_next, t_curr],
                });
            }
        }
    }

    Ok(MeshNode {
        name: name.into(),
        vertices,
        faces,
        material_name: mat.into(),
    })
}
