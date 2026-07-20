//! Component pin management methods for EntityGraph.

use crate::geometry::BoundingBox;
use crate::geometry_router::substrate_types::ComponentPin;

use super::EntityGraph;

impl EntityGraph {
    /// Add component pin for physical continuity validation.
    pub fn add_component_pin(
        &mut self,
        x_nm: i64,
        y_nm: i64,
        z_nm: i64,
        component_name: compact_str::CompactString,
        pin_name: compact_str::CompactString,
        net: Option<compact_str::CompactString>,
    ) {
        let pin = ComponentPin::new(x_nm, y_nm, z_nm, component_name, pin_name, net);
        self.component_pins.push(pin);
    }

    /// Set the net for a specific component pin.
    pub fn set_pin_net(&mut self, component_name: &str, pin_name: &str, net_name: &str) {
        if let Some(pin) = self.component_pins.iter_mut().find(|p| {
            p.component_name.as_str() == component_name && p.pin_name.as_str() == pin_name
        }) {
            pin.net = Some(net_name.into());
        }
    }

    /// Get all component pins.
    pub fn get_component_pins(&self) -> &[ComponentPin] {
        &self.component_pins
    }

    /// Get the bounding box of a pour associated with a pin.
    pub fn get_pour_bbox_for_pin(
        &self,
        component_name: &str,
        pin_name: &str,
    ) -> Option<BoundingBox> {
        self.component_pins
            .iter()
            .find(|p| {
                p.component_name.as_str() == component_name && p.pin_name.as_str() == pin_name
            })
            .and_then(|p| self.get_pour_bbox_at_position(p.x_nm, p.y_nm, p.z_nm))
    }
}
