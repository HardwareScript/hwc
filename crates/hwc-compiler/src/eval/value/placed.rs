use compact_str::CompactString;
use hwc_types::NetId;
use super::cell_layout::CellLayout;
use super::Value;

/// Strongly-typed connection port on a placed cell instance (World Coordinates)
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedPort {
    pub cell_name: CompactString,
    pub instance_name: CompactString,
    pub port_name: CompactString,
    pub world_x: i64,
    pub world_y: i64,
    pub layer: CompactString,
    pub net: Option<NetId>,
}

/// Placed cell instance in top-level space (World Coordinates)
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedCellInstance {
    pub cell: CellLayout,
    pub instance_name: CompactString,
    pub placement_x: i64, // pm
    pub placement_y: i64, // pm
}

impl PlacedCellInstance {
    pub fn port(&self, port_name: &str) -> Option<PlacedPort> {
        self.cell.ports.iter().find(|p| p.name == port_name).map(|p| {
            let transformed_local = self.cell.transform.apply_point(p.at);
            let world_x = self.placement_x + transformed_local.0;
            let world_y = self.placement_y + transformed_local.1;
            PlacedPort {
                cell_name: self.cell.name.clone(),
                instance_name: self.instance_name.clone(),
                port_name: p.name.clone(),
                world_x,
                world_y,
                layer: p.layer.clone(),
                net: p.net,
            }
        })
    }

    pub fn bounding_box(&self) -> Value {
        let (lx, ly, hx, hy) = self.cell.bounding_box();
        Value::BoundingBox {
            min_x: self.placement_x + lx,
            min_y: self.placement_y + ly,
            max_x: self.placement_x + hx,
            max_y: self.placement_y + hy,
        }
    }
}
