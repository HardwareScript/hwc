//! PhysicalInterface struct and constructors.

use std::sync::Arc;

use crate::geometry::{BoundingBox, Point3D};
use crate::geometry_router::routing_intent::RoutingIntent;
use crate::netlist::ComponentId;
use smallvec::{smallvec, SmallVec};

use super::access_region::AccessRegion;
use super::capability::InterfaceCapability;
use super::geometry::InterfaceGeometry;
use super::types::{DerivedConstraint, InterfaceId, Normal2D, Orientation, RoutingDatabase};

/// A concrete physical interface on a component.
///
/// Contains geometry, capabilities, and pre-computed cached properties.
/// Generated once during interface creation and stored as immutable data
/// owned by the EntityGraph.
#[derive(Debug, Clone)]
pub struct PhysicalInterface {
    /// Unique interface identifier
    pub id: InterfaceId,
    /// Owning component
    pub component_id: ComponentId,
    /// Physical geometry of this interface
    pub geometry: InterfaceGeometry,
    /// Physical capabilities (current, bandwidth, thermal)
    pub capabilities: SmallVec<[InterfaceCapability; 4]>,
    /// Routing intent hint for this interface
    pub routing_intent: RoutingIntent,

    // ── Cached Derived Properties (computed once, immutable) ──
    /// Pre-computed outward normals (one per polygon edge)
    pub boundary_normals: Arc<Vec<Normal2D>>,
    /// Pre-computed approach zones
    pub access_regions: Arc<SmallVec<[AccessRegion; 8]>>,
    /// Minkowski-inflated keepout boundaries
    pub expanded_keepouts: Arc<Vec<BoundingBox>>,
    /// Derived constraints from capabilities
    pub derived_constraints: Arc<Vec<DerivedConstraint>>,
}

/// Parameters for constructing a `PhysicalInterface`.
pub struct PhysicalInterfaceParams {
    pub id: InterfaceId,
    pub component_id: ComponentId,
    pub geometry: InterfaceGeometry,
    pub capabilities: SmallVec<[InterfaceCapability; 4]>,
    pub routing_intent: RoutingIntent,
    pub orientation: Option<Orientation>, // User-declared orientation (None = auto-derive)
    pub trace_width_nm: i64,
    pub escape_stub_length_nm: i64,
}

impl PhysicalInterface {
    /// Create a new interface with cached properties pre-computed.
    ///
    /// Computes normals, access regions, and derived constraints once,
    /// then caches them as immutable data.
    ///
    /// **IMPORTANT**: The `orientation` parameter should be passed from user
    /// declarations if available (via shape attributes or routing directives).
    /// Only use `Orientation::Derived` as a fallback when no user intent exists.
    pub fn new(params: PhysicalInterfaceParams, db: &dyn RoutingDatabase) -> Self {
        // Determine orientation: use explicit if available, otherwise derive from geometry
        let orientation = if let Some(user_orient) = params.orientation {
            user_orient
        } else {
            match &params.geometry {
                InterfaceGeometry::Point(_) => Orientation::None,
                _ => Orientation::Derived,
            }
        };

        let boundary_normals = Arc::new(params.geometry.derive_normals(orientation));
        let access_regions = Arc::new(Self::compute_access_regions(
            &params.geometry,
            &boundary_normals,
            params.trace_width_nm,
            params.escape_stub_length_nm,
        ));
        let expanded_keepouts = Arc::new(Self::compute_keepouts(
            &params.geometry,
            params.trace_width_nm,
        ));
        let derived_constraints: SmallVec<[DerivedConstraint; 4]> = params
            .capabilities
            .iter()
            .map(|cap| cap.derive_constraint(db))
            .filter(|c| !matches!(c, DerivedConstraint::None))
            .collect();
        let derived_constraints = Arc::new(derived_constraints.into_vec());

        Self {
            id: params.id,
            component_id: params.component_id,
            geometry: params.geometry,
            capabilities: params.capabilities,
            routing_intent: params.routing_intent,
            boundary_normals,
            access_regions,
            expanded_keepouts,
            derived_constraints,
        }
    }

    fn compute_access_regions(
        geometry: &InterfaceGeometry,
        normals: &[Normal2D],
        trace_width_nm: i64,
        escape_stub_length_nm: i64,
    ) -> SmallVec<[AccessRegion; 8]> {
        match geometry {
            InterfaceGeometry::Point(_) => smallvec::smallvec![],
            InterfaceGeometry::Edge { start, end } => {
                if normals.is_empty() {
                    return smallvec::smallvec![];
                }
                smallvec![AccessRegion::generate(
                    start,
                    end,
                    &normals[0],
                    escape_stub_length_nm,
                    trace_width_nm,
                )]
            }
            InterfaceGeometry::Polygon(vertices) => AccessRegion::generate_polygon(
                vertices,
                normals,
                escape_stub_length_nm,
                trace_width_nm,
            ),
        }
    }

    fn compute_keepouts(geometry: &InterfaceGeometry, trace_width_nm: i64) -> Vec<BoundingBox> {
        let inflate = trace_width_nm / 2;
        match geometry {
            InterfaceGeometry::Point(p) => vec![BoundingBox::from_point(*p, inflate)],
            InterfaceGeometry::Edge { start: _, end: _ } => {
                vec![geometry.bounding_box().inflate_xy(inflate)]
            }
            InterfaceGeometry::Polygon(vertices) => Self::polygon_keepout(vertices, inflate),
        }
    }

    fn polygon_keepout(vertices: &[Point3D], inflate: i64) -> Vec<BoundingBox> {
        let mut min_x = i64::MAX;
        let mut min_y = i64::MAX;
        let mut max_x = i64::MIN;
        let mut max_y = i64::MIN;
        let mut z = 0;
        for v in vertices {
            min_x = min_x.min(v.x);
            min_y = min_y.min(v.y);
            max_x = max_x.max(v.x);
            max_y = max_y.max(v.y);
            z = v.z;
        }
        vec![BoundingBox::new(
            Point3D::new(min_x - inflate, min_y - inflate, z),
            Point3D::new(max_x + inflate, max_y + inflate, z),
        )]
    }
}
