//! Metal-Only ECO Jumpers (Layers M1-M4)
//!
//! Generates discrete metal jumpers between reconfigured GA-fillers and existing nets,
//! leaving base silicon masks 100% untouched to save millions in refabrication costs.

use crate::traits::RoutingError;
use crate::types::{RoutedOutput, RoutedTraceSegment, RoutedViaInstance};
use compact_str::CompactString;
use hwc_engine::geometry::Point3D;
use hwc_engine::netlist::NetId;

pub struct MetalEcoRouter {
    pub allowed_layers: Vec<CompactString>,
}

impl Default for MetalEcoRouter {
    fn default() -> Self {
        Self {
            allowed_layers: vec![
                CompactString::new("metal1"),
                CompactString::new("metal2"),
                CompactString::new("metal3"),
                CompactString::new("metal4"),
            ],
        }
    }
}

impl MetalEcoRouter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Routes metal-only jumpers between source and target points.
    pub fn route_jumper(
        &self,
        net_id: NetId,
        from: Point3D,
        to: Point3D,
    ) -> Result<RoutedOutput, RoutingError> {
        let mut traces = Vec::new();
        let mut vias = Vec::new();

        // Horizontal jumper on Metal 2
        let m2_layer = CompactString::new("metal2");
        let m3_layer = CompactString::new("metal3");

        let corner = Point3D::new(to.x, from.y, 0);

        traces.push(RoutedTraceSegment {
            net_id,
            layer_name: m2_layer.clone(),
            start: from,
            end: corner,
            width_pm: 140_000,
        });

        // Vertical jumper on Metal 3
        traces.push(RoutedTraceSegment {
            net_id,
            layer_name: m3_layer.clone(),
            start: corner,
            end: to,
            width_pm: 140_000,
        });

        // Via at the corner between M2 and M3
        vias.push(RoutedViaInstance {
            net_id,
            position: corner,
            from_layer_name: m2_layer,
            to_layer_name: m3_layer,
            diameter_pm: 150_000,
        });

        Ok(RoutedOutput {
            traces,
            vias,
            cut_masks: None,
        })
    }
}
