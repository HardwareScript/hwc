//! EntityGraph management for component obstacles and pins

use super::types::GeometryRouter;
use crate::geometry::transform::FixedTransform2D;
use crate::geometry::BoundingBox;
use crate::geometry_router::scene_graph::ComponentStamp;
use crate::geometry_router::EntityGraph;
use smallvec::SmallVec;

/// Parameters for adding a component pin
pub struct ComponentPinParams {
    pub x_nm: i64,
    pub y_nm: i64,
    pub z_nm: i64,
    pub component_name: compact_str::CompactString,
    pub pin_name: compact_str::CompactString,
    pub net: Option<compact_str::CompactString>,
}

impl ComponentPinParams {
    pub fn new(
        x_nm: i64,
        y_nm: i64,
        z_nm: i64,
        component_name: compact_str::CompactString,
        pin_name: compact_str::CompactString,
        net: Option<compact_str::CompactString>,
    ) -> Self {
        Self {
            x_nm,
            y_nm,
            z_nm,
            component_name,
            pin_name,
            net,
        }
    }
}

impl GeometryRouter {
    /// Add a component obstacle (GAP3).
    pub fn add_component_obstacle(
        &mut self,
        entity_graph: &mut EntityGraph,
        bbox: BoundingBox,
        material: u8,
        name: compact_str::CompactString,
        component_type: compact_str::CompactString,
    ) {
        entity_graph.add_component_metadata(bbox, material, name, component_type, SmallVec::new());
    }

    /// Add a component pin (GAP3).
    pub fn add_component_pin(
        &mut self,
        entity_graph: &mut EntityGraph,
        params: ComponentPinParams,
    ) {
        entity_graph.add_component_pin(
            params.x_nm,
            params.y_nm,
            params.z_nm,
            params.component_name,
            params.pin_name,
            params.net,
        );
    }

    /// Build the Entity Graph spatial index from current component metadata.
    pub fn build_entity_graph(&mut self, entity_graph: &mut EntityGraph) {
        let metadata = entity_graph.get_component_metadata().to_vec();
        for (idx, meta) in metadata.iter().enumerate() {
            let width = meta.bbox.max.x - meta.bbox.min.x;
            let height = meta.bbox.max.y - meta.bbox.min.y;
            let stamp =
                ComponentStamp::rectangle(idx, meta.component_type.to_string(), width, height);
            let stamp_id = entity_graph.scene_mut().register_stamp(stamp);

            let transform = FixedTransform2D::from_translation(meta.bbox.min.x, meta.bbox.min.y);

            let net_bindings: Vec<usize> = Vec::new();
            entity_graph
                .scene_mut()
                .place_instance(stamp_id, transform, net_bindings);
        }
    }
}
