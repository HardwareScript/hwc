use compact_str::CompactString;
use hwc_engine::entity_graph::identity::EntityId;
use rustc_hash::FxHashMap;

use super::super::super::context::EvalError;
use super::super::super::geometry_record::GeometryRecord;
use super::super::super::value::{SpaceId, Value};
use super::super::vm_core::VM;

impl<'a> VM<'a> {
    pub(crate) fn handle_emit_polygon(
        &mut self,
        frame_idx: usize,
        base: usize,
        name_reg: u16,
        layer_reg: u16,
        net_reg: u16,
        points_or_rect_reg: u16,
    ) -> Result<(), EvalError> {
        let space_id = self.current_space_id.ok_or(EvalError::NoActiveSpaceContext { method: "add_polygon" })?;
        let semantic_name = match &self.stack[base + name_reg as usize] {
            Value::String(s) => Some(s.clone()),
            _ => None,
        };
        let layer = self.stack[base + layer_reg as usize].as_compact_str()?.clone();
        let net = match &self.stack[base + net_reg as usize] {
            Value::NetHandle(id) => Some(*id),
            _ => None,
        };
        let geom = &self.stack[base + points_or_rect_reg as usize];

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

        let id = EntityId::compute(
            &self.frames[frame_idx].path,
            "Polygon",
            semantic_name.as_deref(),
            self.emitted_record_count,
        );
        self.emitted_record_count += 1;

        let record_size = std::mem::size_of::<GeometryRecord>() + points.len() * 16;
        self.guard.track_allocation(record_size)?;

        if let Some(buf) = &mut self.output_buffer {
            buf.push(GeometryRecord::Polygon {
                id,
                space_id: SpaceId(space_id),
                layer: layer.clone(),
                net_id: net.map(|n| n.0),
                points_pm: points.clone(),
            });
        }

        self.emitter.add_polygon(space_id, layer.as_str(), net, points, semantic_name, None)?;
        Ok(())
    }

    pub(crate) fn handle_emit_contact(
        &mut self,
        frame_idx: usize,
        base: usize,
        name_reg: u16,
        from_layer_reg: u16,
        to_layer_reg: u16,
        at_reg: u16,
        dia_reg: u16,
        net_reg: u16,
    ) -> Result<(), EvalError> {
        let space_id = self.current_space_id.ok_or(EvalError::NoActiveSpaceContext { method: "add_contact" })?;
        let semantic_name = match &self.stack[base + name_reg as usize] {
            Value::String(s) => Some(s.clone()),
            _ => None,
        };
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

        let id = EntityId::compute(
            &self.frames[frame_idx].path,
            "Contact",
            semantic_name.as_deref(),
            self.emitted_record_count,
        );
        self.emitted_record_count += 1;

        self.guard.track_allocation(std::mem::size_of::<GeometryRecord>())?;

        if let Some(buf) = &mut self.output_buffer {
            buf.push(GeometryRecord::Contact {
                id,
                space_id: SpaceId(space_id),
                from_layer: from_layer.clone(),
                to_layer: to_layer.clone(),
                center_pm: at,
                diameter_pm: dia_pm,
                net_id: net.map(|n| n.0),
            });
        }

        self.emitter.add_contact(space_id, from_layer.as_str(), to_layer.as_str(), at, dia_pm, net, semantic_name)?;
        Ok(())
    }

    pub(crate) fn handle_emit_device(
        &mut self,
        frame_idx: usize,
        base: usize,
        type_reg: u16,
        name_reg: u16,
        terminals_reg: u16,
        params_reg: u16,
    ) -> Result<(), EvalError> {
        let space_id = self.current_space_id.ok_or(EvalError::NoActiveSpaceContext { method: "add_device" })?;
        let dev_type = match &self.stack[base + type_reg as usize] {
            Value::String(s) => s.clone(),
            Value::EnumVariant { variant_name, .. } => variant_name.clone(),
            other => CompactString::new(format!("{}", other)),
        };
        let name = match &self.stack[base + name_reg as usize] {
            Value::String(s) => s.clone(),
            _ => CompactString::new("DEV"),
        };
        let terms = match &self.stack[base + terminals_reg as usize] {
            Value::StructInstance { fields, .. } => {
                let mut map = FxHashMap::default();
                for (k, v) in fields.iter() {
                    if let Value::NetHandle(id) = v {
                        map.insert(k.clone(), *id);
                    }
                }
                map
            }
            _ => FxHashMap::default(),
        };
        let params = match &self.stack[base + params_reg as usize] {
            Value::StructInstance { fields, .. } => {
                let mut map = FxHashMap::default();
                for (k, v) in fields.iter() {
                    if let Value::Measurement(m) = v {
                        map.insert(k.clone(), *m);
                    }
                }
                map
            }
            _ => FxHashMap::default(),
        };

        let id = EntityId::compute(
            &self.frames[frame_idx].path,
            "Device",
            Some(name.as_str()),
            self.emitted_record_count,
        );
        self.emitted_record_count += 1;

        let dev_size = std::mem::size_of::<GeometryRecord>() + terms.len() * 32 + params.len() * 32;
        self.guard.track_allocation(dev_size)?;

        if let Some(buf) = &mut self.output_buffer {
            let mut term_vec = Vec::with_capacity(terms.len());
            for (k, v) in &terms {
                term_vec.push((k.clone(), v.0));
            }
            let mut param_vec = Vec::with_capacity(params.len());
            for (k, v) in &params {
                param_vec.push((k.clone(), v.raw as f64));
            }
            buf.push(GeometryRecord::Device {
                id,
                space_id: SpaceId(space_id),
                device_type: dev_type.clone(),
                instance_name: name.clone(),
                terminals: term_vec,
                params: param_vec,
            });
        }

        self.emitter.add_device(space_id, dev_type.as_str(), name.as_str(), terms, params)?;
        Ok(())
    }

    pub(crate) fn handle_emit_route(
        &mut self,
        frame_idx: usize,
        base: usize,
        from_reg: u16,
        to_reg: u16,
        intent_idx: u16,
        props_reg: u16,
    ) -> Result<(), EvalError> {
        let space_id = self.current_space_id.ok_or(EvalError::NoActiveSpaceContext { method: "route" })?;
        let from_val = self.stack[base + from_reg as usize].clone();
        let to_val = self.stack[base + to_reg as usize].clone();
        let intent = self.frames[frame_idx].chunk.constants[intent_idx as usize].as_compact_str()?.clone();
        let props = match &self.stack[base + props_reg as usize] {
            Value::StructInstance { fields, .. } => {
                let mut map = FxHashMap::default();
                for (k, v) in fields.iter() {
                    map.insert(k.clone(), v.clone());
                }
                map
            }
            _ => FxHashMap::default(),
        };

        let id = EntityId::compute(
            &self.frames[frame_idx].path,
            "RouteIntent",
            Some(intent.as_str()),
            self.emitted_record_count,
        );
        self.emitted_record_count += 1;

        self.guard.track_allocation(std::mem::size_of::<GeometryRecord>())?;

        if let Some(buf) = &mut self.output_buffer {
            let from_port = match &from_val {
                Value::Point2D { x, y } => (*x, *y, 0),
                _ => (0, 0, 0),
            };
            let to_port = match &to_val {
                Value::Point2D { x, y } => (*x, *y, 0),
                _ => (0, 0, 0),
            };
            buf.push(GeometryRecord::RouteIntent {
                id,
                space_id: SpaceId(space_id),
                from_port,
                to_port,
                intent: intent.clone(),
            });
        }

        self.emitter.add_route(space_id, from_val, to_val, Some(intent), props)?;
        Ok(())
    }
}
