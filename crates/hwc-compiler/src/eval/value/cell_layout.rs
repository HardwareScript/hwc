use compact_str::CompactString;
use hwc_types::NetId;
use super::cell::{CellContact, CellDevice, CellPolygon, CellPort};
use super::transform::Transform2D;
use super::Value;

/// Pure, self-contained cell layout container (Pillar 1)
#[derive(Debug, Clone, PartialEq)]
pub struct CellLayout {
    pub name: CompactString,
    pub polygons: Vec<CellPolygon>,
    pub contacts: Vec<CellContact>,
    pub ports: Vec<CellPort>,
    pub devices: Vec<CellDevice>,
    pub transform: Transform2D,
}

impl CellLayout {
    pub fn new(name: impl Into<CompactString>) -> Self {
        Self {
            name: name.into(),
            polygons: Vec::new(),
            contacts: Vec::new(),
            ports: Vec::new(),
            devices: Vec::new(),
            transform: Transform2D::default(),
        }
    }

    pub fn rotate(&self, deg: i32) -> Self {
        let mut copy = self.clone();
        copy.transform.rotation_deg = ((copy.transform.rotation_deg + deg) % 360 + 360) % 360;
        copy
    }

    pub fn mirror_x(&self) -> Self {
        let mut copy = self.clone();
        copy.transform.mirror_x = !copy.transform.mirror_x;
        copy
    }

    pub fn mirror_y(&self) -> Self {
        let mut copy = self.clone();
        copy.transform.mirror_y = !copy.transform.mirror_y;
        copy
    }

    pub fn offset(&self, dx: i64, dy: i64) -> Self {
        let mut copy = self.clone();
        copy.transform.offset_x += dx;
        copy.transform.offset_y += dy;
        copy
    }

    pub fn add_polygon(&mut self, layer: impl Into<CompactString>, points: Vec<(i64, i64)>, net: Option<NetId>, port: Option<CompactString>) {
        self.polygons.push(CellPolygon {
            layer: layer.into(),
            points,
            net,
            port,
        });
    }

    pub fn add_contact(
        &mut self,
        from: impl Into<CompactString>,
        to: impl Into<CompactString>,
        at: (i64, i64),
        diameter: i64,
        name: Option<CompactString>,
        net: Option<NetId>,
    ) {
        self.contacts.push(CellContact {
            name: name.unwrap_or_default(),
            from_layer: from.into(),
            to_layer: to.into(),
            at,
            diameter,
            net,
        });
    }

    pub fn add_port(
        &mut self,
        name: impl Into<CompactString>,
        at: (i64, i64),
        layer: impl Into<CompactString>,
        net: Option<NetId>,
    ) {
        self.ports.push(CellPort {
            name: name.into(),
            at,
            layer: layer.into(),
            net,
        });
    }

    pub fn add_device(
        &mut self,
        device_type: impl Into<CompactString>,
        instance_name: impl Into<CompactString>,
        terminals: Vec<(CompactString, CompactString)>,
        params: Vec<(CompactString, Value)>,
    ) {
        self.devices.push(CellDevice {
            device_type: device_type.into(),
            instance_name: instance_name.into(),
            terminals,
            params,
        });
    }

    pub fn place(&mut self, child: &CellLayout, at: (i64, i64)) {
        for poly in &child.polygons {
            let mut pts = Vec::with_capacity(poly.points.len());
            for pt in &poly.points {
                let (tx, ty) = child.transform.apply_point(*pt);
                pts.push((at.0 + tx, at.1 + ty));
            }
            self.polygons.push(CellPolygon {
                layer: poly.layer.clone(),
                points: pts,
                net: poly.net,
                port: poly.port.clone(),
            });
        }
        for c in &child.contacts {
            let (tx, ty) = child.transform.apply_point(c.at);
            self.contacts.push(CellContact {
                name: c.name.clone(),
                from_layer: c.from_layer.clone(),
                to_layer: c.to_layer.clone(),
                at: (at.0 + tx, at.1 + ty),
                diameter: c.diameter,
                net: c.net,
            });
        }
        for port in &child.ports {
            let (tx, ty) = child.transform.apply_point(port.at);
            self.ports.push(CellPort {
                name: port.name.clone(),
                at: (at.0 + tx, at.1 + ty),
                layer: port.layer.clone(),
                net: port.net,
            });
        }
        for dev in &child.devices {
            self.devices.push(dev.clone());
        }
    }

    pub fn bounding_box(&self) -> (i64, i64, i64, i64) {
        let mut min_x = i64::MAX;
        let mut min_y = i64::MAX;
        let mut max_x = i64::MIN;
        let mut max_y = i64::MIN;

        for p in &self.polygons {
            for pt in &p.points {
                let (tx, ty) = self.transform.apply_point(*pt);
                min_x = min_x.min(tx);
                min_y = min_y.min(ty);
                max_x = max_x.max(tx);
                max_y = max_y.max(ty);
            }
        }
        for c in &self.contacts {
            let (tx, ty) = self.transform.apply_point(c.at);
            let r = c.diameter / 2;
            min_x = min_x.min(tx - r);
            min_y = min_y.min(ty - r);
            max_x = max_x.max(tx + r);
            max_y = max_y.max(ty + r);
        }
        for port in &self.ports {
            let (tx, ty) = self.transform.apply_point(port.at);
            min_x = min_x.min(tx);
            min_y = min_y.min(ty);
            max_x = max_x.max(tx);
            max_y = max_y.max(ty);
        }

        if min_x > max_x {
            (0, 0, 0, 0)
        } else {
            (min_x, min_y, max_x, max_y)
        }
    }
}
