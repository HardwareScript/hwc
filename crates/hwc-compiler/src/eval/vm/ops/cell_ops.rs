use compact_str::CompactString;
use rustc_hash::FxHashMap;
use std::sync::Arc;

use super::super::super::context::EvalError;
use super::super::super::value::{CellLayout, PlacedCellInstance, PlacedPort, Value};
use super::super::vm_core::VM;

impl<'a> VM<'a> {
    pub(crate) fn handle_space_place_cell(
        &mut self,
        _frame_idx: usize,
        base: usize,
        dst: u16,
        cell_reg: u16,
        at_reg: u16,
    ) -> Result<(), EvalError> {
        let space_id = self.current_space_id.ok_or(EvalError::NoActiveSpaceContext { method: "place" })?;
        let cell_val = self.stack[base + cell_reg as usize].clone();
        let at_val = self.stack[base + at_reg as usize].coerce_to_point2d()?;
        let (at_x, at_y) = match at_val {
            Value::Point2D { x, y } => (x, y),
            _ => (0, 0),
        };

        match cell_val {
            Value::CellLayout(cell_arc) => {
                let cell = (*cell_arc).clone();

                let placed_idx = self.emitted_record_count;
                self.emitted_record_count += 1;
                let instance_name = if placed_idx == 0 {
                    cell.name.clone()
                } else {
                    CompactString::new(format!("{}_{}", cell.name, placed_idx))
                };

                for poly in &cell.polygons {
                    let mut world_points = Vec::with_capacity(poly.points.len());
                    for pt in &poly.points {
                        let (tx, ty) = cell.transform.apply_point(*pt);
                        world_points.push((at_x + tx, at_y + ty));
                    }
                    self.emitter.add_polygon(space_id, poly.layer.as_str(), poly.net, world_points, Some(instance_name.clone()), poly.port.clone())?;
                }

                for c in &cell.contacts {
                    let (tx, ty) = cell.transform.apply_point(c.at);
                    let world_at = (at_x + tx, at_y + ty);
                    self.emitter.add_contact(
                        space_id,
                        c.from_layer.as_str(),
                        c.to_layer.as_str(),
                        world_at,
                        c.diameter,
                        c.net,
                        Some(c.name.clone()),
                    )?;
                }

                for dev in &cell.devices {
                    let mut term_map = FxHashMap::default();
                    let mut port_map = FxHashMap::default();
                    for (k, v) in &dev.terminals {
                        term_map.insert(k.clone(), hwc_types::NetId(0));
                        port_map.insert(k.clone(), v.clone());
                    }
                    let mut param_map = FxHashMap::default();
                    for (k, v) in &dev.params {
                        if let Value::Measurement(m) = v {
                            param_map.insert(k.clone(), m.clone());
                        }
                    }
                    self.emitter.add_device_with_ports(space_id, dev.device_type.as_str(), instance_name.as_str(), term_map, port_map, param_map)?;
                }

                let placed_instance = PlacedCellInstance {
                    cell: cell.clone(),
                    instance_name: instance_name.clone(),
                    placement_x: at_x,
                    placement_y: at_y,
                };
                self.stack[base + dst as usize] = Value::PlacedCell(Arc::new(placed_instance));
            }
            other => return Err(EvalError::TypeMismatch {
                expected: "CellLayout",
                found: other.type_name().to_string(),
            }),
        }
        Ok(())
    }

    pub(crate) fn handle_cell_rotate(
        &mut self,
        base: usize,
        dst: u16,
        cell_reg: u16,
        deg_reg: u16,
    ) -> Result<(), EvalError> {
        let cell_val = self.stack[base + cell_reg as usize].clone();
        let deg = match &self.stack[base + deg_reg as usize] {
            Value::Int(i) => *i as i32,
            Value::Float(f) => *f as i32,
            Value::Measurement(m) => (m.raw / 1_000_000) as i32,
            _ => 0,
        };
        match cell_val {
            Value::CellLayout(cell_arc) => {
                let cell = (*cell_arc).clone();
                let rotated = cell.rotate(deg);
                self.stack[base + dst as usize] = Value::CellLayout(Arc::new(rotated));
            }
            other => return Err(EvalError::TypeMismatch {
                expected: "CellLayout",
                found: other.type_name().to_string(),
            }),
        }
        Ok(())
    }

    pub(crate) fn handle_cell_mirror_x(
        &mut self,
        base: usize,
        dst: u16,
        cell_reg: u16,
    ) -> Result<(), EvalError> {
        let cell_val = self.stack[base + cell_reg as usize].clone();
        match cell_val {
            Value::CellLayout(cell_arc) => {
                let cell = (*cell_arc).clone();
                let mirrored = cell.mirror_x();
                self.stack[base + dst as usize] = Value::CellLayout(Arc::new(mirrored));
            }
            other => return Err(EvalError::TypeMismatch {
                expected: "CellLayout",
                found: other.type_name().to_string(),
            }),
        }
        Ok(())
    }

    pub(crate) fn handle_cell_mirror_y(
        &mut self,
        base: usize,
        dst: u16,
        cell_reg: u16,
    ) -> Result<(), EvalError> {
        let cell_val = self.stack[base + cell_reg as usize].clone();
        match cell_val {
            Value::CellLayout(cell_arc) => {
                let cell = (*cell_arc).clone();
                let mirrored = cell.mirror_y();
                self.stack[base + dst as usize] = Value::CellLayout(Arc::new(mirrored));
            }
            other => return Err(EvalError::TypeMismatch {
                expected: "CellLayout",
                found: other.type_name().to_string(),
            }),
        }
        Ok(())
    }

    pub(crate) fn handle_cell_offset(
        &mut self,
        base: usize,
        dst: u16,
        cell_reg: u16,
        dx_reg: u16,
        dy_reg: u16,
    ) -> Result<(), EvalError> {
        let cell_val = self.stack[base + cell_reg as usize].clone();
        let dx = match &self.stack[base + dx_reg as usize] {
            Value::Measurement(m) => m.raw as i64,
            Value::Int(i) => *i,
            _ => 0,
        };
        let dy = match &self.stack[base + dy_reg as usize] {
            Value::Measurement(m) => m.raw as i64,
            Value::Int(i) => *i,
            _ => 0,
        };
        match cell_val {
            Value::CellLayout(cell_arc) => {
                let cell = (*cell_arc).clone();
                let offsetted = cell.offset(dx, dy);
                self.stack[base + dst as usize] = Value::CellLayout(Arc::new(offsetted));
            }
            other => return Err(EvalError::TypeMismatch {
                expected: "CellLayout",
                found: other.type_name().to_string(),
            }),
        }
        Ok(())
    }

    pub(crate) fn handle_cell_port(
        &mut self,
        frame_idx: usize,
        base: usize,
        dst: u16,
        target_reg: u16,
        port_name_idx: u16,
    ) -> Result<(), EvalError> {
        let target_val = self.stack[base + target_reg as usize].clone();
        let port_name = self.frames[frame_idx].chunk.constants[port_name_idx as usize].as_compact_str()?.clone();

        match target_val {
            Value::PlacedCell(inst) => {
                let placed_port = inst.port(port_name.as_str()).ok_or_else(|| EvalError::General {
                    message: format!("Port '{}' not found on placed cell '{}'", port_name, inst.cell.name),
                })?;
                self.stack[base + dst as usize] = Value::PlacedPort(placed_port);
            }
            Value::CellLayout(cell_arc) => {
                let cell = (*cell_arc).clone();
                let port = cell.ports.iter().find(|p| p.name == port_name).ok_or_else(|| EvalError::General {
                    message: format!("Port '{}' not found on cell '{}'", port_name, cell.name),
                })?;
                let (tx, ty) = cell.transform.apply_point(port.at);
                self.stack[base + dst as usize] = Value::PlacedPort(PlacedPort {
                    cell_name: cell.name.clone(),
                    instance_name: CompactString::default(),
                    port_name: port.name.clone(),
                    world_x: tx,
                    world_y: ty,
                    layer: port.layer.clone(),
                    net: port.net,
                });
            }
            other => return Err(EvalError::TypeMismatch {
                expected: "PlacedCell or CellLayout",
                found: other.type_name().to_string(),
            }),
        }
        Ok(())
    }

    pub(crate) fn handle_cell_bbox(
        &mut self,
        base: usize,
        dst: u16,
        target_reg: u16,
    ) -> Result<(), EvalError> {
        let target_val = self.stack[base + target_reg as usize].clone();
        match target_val {
            Value::PlacedCell(inst) => {
                self.stack[base + dst as usize] = inst.bounding_box();
            }
            Value::CellLayout(cell_arc) => {
                let cell = (*cell_arc).clone();
                let (min_x, min_y, max_x, max_y) = cell.bounding_box();
                self.stack[base + dst as usize] = Value::BoundingBox { min_x, min_y, max_x, max_y };
            }
            other => return Err(EvalError::TypeMismatch {
                expected: "PlacedCell or CellLayout",
                found: other.type_name().to_string(),
            }),
        }
        Ok(())
    }

    pub(crate) fn handle_cell_new(
        &mut self,
        base: usize,
        dst: u16,
        name_reg: u16,
    ) -> Result<(), EvalError> {
        let name = match &self.stack[base + name_reg as usize] {
            Value::String(s) => s.clone(),
            _ => CompactString::new("cell"),
        };
        self.stack[base + dst as usize] = Value::CellLayout(Arc::new(CellLayout::new(name)));
        Ok(())
    }

    pub(crate) fn handle_cell_add_polygon(
        &mut self,
        base: usize,
        cell_reg: u16,
        layer_reg: u16,
        net_reg: u16,
        rect_or_points_reg: u16,
        port_reg: u16,
    ) -> Result<(), EvalError> {
        let layer = self.stack[base + layer_reg as usize].as_compact_str()?.clone();
        let net = match &self.stack[base + net_reg as usize] {
            Value::NetHandle(id) => Some(*id),
            _ => None,
        };
        let port = match &self.stack[base + port_reg as usize] {
            Value::String(s) => Some(s.clone()),
            _ => None,
        };
        let geom = &self.stack[base + rect_or_points_reg as usize];
        let points = match geom {
            Value::Array(items)
                if items.len() == 4
                    && matches!(&items[0], Value::Measurement(_) | Value::Int(_))
                    && matches!(&items[1], Value::Measurement(_) | Value::Int(_))
                    && matches!(&items[2], Value::Measurement(_) | Value::Int(_))
                    && matches!(&items[3], Value::Measurement(_) | Value::Int(_)) =>
            {
                let x = match &items[0] { Value::Measurement(m) => m.raw as i64, Value::Int(i) => *i, _ => 0 };
                let y = match &items[1] { Value::Measurement(m) => m.raw as i64, Value::Int(i) => *i, _ => 0 };
                let w = match &items[2] { Value::Measurement(m) => m.raw as i64, Value::Int(i) => *i, _ => 0 };
                let h = match &items[3] { Value::Measurement(m) => m.raw as i64, Value::Int(i) => *i, _ => 0 };
                vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)]
            }
            Value::Array(items) => {
                let mut pts = Vec::new();
                for item in items.iter() {
                    let p = item.coerce_to_point2d()?;
                    if let Value::Point2D { x, y } = p {
                        pts.push((x, y));
                    }
                }
                pts
            }
            _ => vec![],
        };

        match &mut self.stack[base + cell_reg as usize] {
            Value::CellLayout(cell_arc) => {
                Arc::make_mut(cell_arc).add_polygon(layer, points, net, port);
            }
            other => return Err(EvalError::TypeMismatch {
                expected: "CellLayout",
                found: other.type_name().to_string(),
            }),
        }
        Ok(())
    }

    pub(crate) fn handle_cell_add_contact(
        &mut self,
        base: usize,
        cell_reg: u16,
        from_layer_reg: u16,
        to_layer_reg: u16,
        at_reg: u16,
        dia_reg: u16,
        net_reg: u16,
    ) -> Result<(), EvalError> {
        let from_layer = self.stack[base + from_layer_reg as usize].as_compact_str()?.clone();
        let to_layer = self.stack[base + to_layer_reg as usize].as_compact_str()?.clone();
        let at_val = self.stack[base + at_reg as usize].coerce_to_point2d()?;
        let at = match at_val { Value::Point2D { x, y } => (x, y), _ => (0, 0) };
        let dia_pm = match &self.stack[base + dia_reg as usize] {
            Value::Measurement(m) => m.raw as i64,
            Value::Int(i) => *i,
            _ => 170_000,
        };
        let net = match &self.stack[base + net_reg as usize] {
            Value::NetHandle(id) => Some(*id),
            _ => None,
        };

        match &mut self.stack[base + cell_reg as usize] {
            Value::CellLayout(cell_arc) => {
                Arc::make_mut(cell_arc).add_contact(from_layer, to_layer, at, dia_pm, None, net);
            }
            other => return Err(EvalError::TypeMismatch {
                expected: "CellLayout",
                found: other.type_name().to_string(),
            }),
        }
        Ok(())
    }

    pub(crate) fn handle_cell_add_port(
        &mut self,
        base: usize,
        cell_reg: u16,
        name_reg: u16,
        at_reg: u16,
        layer_reg: u16,
        net_reg: u16,
    ) -> Result<(), EvalError> {
        let name = self.stack[base + name_reg as usize].as_compact_str()?.clone();
        let at_val = self.stack[base + at_reg as usize].coerce_to_point2d()?;
        let at = match at_val { Value::Point2D { x, y } => (x, y), _ => (0, 0) };
        let layer = self.stack[base + layer_reg as usize].as_compact_str()?.clone();
        let net = match &self.stack[base + net_reg as usize] {
            Value::NetHandle(id) => Some(*id),
            _ => None,
        };

        match &mut self.stack[base + cell_reg as usize] {
            Value::CellLayout(cell_arc) => {
                Arc::make_mut(cell_arc).add_port(name, at, layer, net);
            }
            other => return Err(EvalError::TypeMismatch {
                expected: "CellLayout",
                found: other.type_name().to_string(),
            }),
        }
        Ok(())
    }

    pub(crate) fn handle_cell_add_device(
        &mut self,
        base: usize,
        cell_reg: u16,
        type_reg: u16,
        terms_reg: u16,
        params_reg: u16,
    ) -> Result<(), EvalError> {
        let dev_type = match &self.stack[base + type_reg as usize] {
            Value::String(s) => s.clone(),
            Value::EnumVariant { variant_name, .. } => variant_name.clone(),
            other => CompactString::new(format!("{}", other)),
        };
        let terms = match &self.stack[base + terms_reg as usize] {
            Value::StructInstance { fields, .. } => {
                let mut vec = Vec::new();
                for (k, v) in fields.iter() {
                    let term_str = match v {
                        Value::String(s) => s.clone(),
                        other => CompactString::new(format!("{}", other)),
                    };
                    vec.push((k.clone(), term_str));
                }
                vec
            }
            _ => Vec::new(),
        };
        let params = match &self.stack[base + params_reg as usize] {
            Value::StructInstance { fields, .. } => {
                let mut vec = Vec::new();
                for (k, v) in fields.iter() {
                    vec.push((k.clone(), v.clone()));
                }
                vec
            }
            _ => Vec::new(),
        };

        match &mut self.stack[base + cell_reg as usize] {
            Value::CellLayout(cell_arc) => {
                let cell_name = cell_arc.name.clone();
                Arc::make_mut(cell_arc).add_device(dev_type, cell_name, terms, params);
            }
            other => return Err(EvalError::TypeMismatch {
                expected: "CellLayout",
                found: other.type_name().to_string(),
            }),
        }
        Ok(())
    }

    pub(crate) fn handle_cell_place(
        &mut self,
        base: usize,
        cell_reg: u16,
        child_cell_reg: u16,
        at_reg: u16,
    ) -> Result<(), EvalError> {
        let at_val = self.stack[base + at_reg as usize].coerce_to_point2d()?;
        let at = match at_val { Value::Point2D { x, y } => (x, y), _ => (0, 0) };
        let child_val = self.stack[base + child_cell_reg as usize].clone();

        match (&mut self.stack[base + cell_reg as usize], child_val) {
            (Value::CellLayout(cell_arc), Value::CellLayout(child_arc)) => {
                Arc::make_mut(cell_arc).place(&child_arc, at);
            }
            (other, _) => return Err(EvalError::TypeMismatch {
                expected: "CellLayout",
                found: other.type_name().to_string(),
            }),
        }
        Ok(())
    }
}
