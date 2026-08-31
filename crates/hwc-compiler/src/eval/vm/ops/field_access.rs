use compact_str::CompactString;
use std::sync::Arc;

use super::super::super::context::EvalError;
use super::super::super::value::{MeasurementValue, Value};
use super::super::vm_core::VM;

impl<'a> VM<'a> {
    pub(crate) fn handle_get_field(
        &mut self,
        frame_idx: usize,
        base: usize,
        dst: u16,
        obj: u16,
        field_idx: u16,
    ) -> Result<(), EvalError> {
        let field_name = self.frames[frame_idx].chunk.constants[field_idx as usize].as_compact_str()?.clone();
        let target_obj = &self.stack[base + obj as usize];
        match target_obj {
            Value::StructInstance { fields, name } => {
                let val = fields
                    .iter()
                    .find(|(k, _)| k == &field_name)
                    .map(|(_, v)| v.clone())
                    .ok_or_else(|| EvalError::FieldNotFound {
                        field: field_name.clone(),
                        struct_name: name.clone(),
                    })?;
                self.stack[base + dst as usize] = val;
            }
            Value::EnumType { name, variants } => {
                let val = variants.get(&field_name).cloned().ok_or_else(|| {
                    EvalError::FieldNotFound {
                        field: field_name.clone(),
                        struct_name: name.clone(),
                    }
                })?;
                self.stack[base + dst as usize] = val;
            }
            Value::Point2D { x, y } => {
                let result = match field_name.as_str() {
                    "x" => Value::Measurement(MeasurementValue::length_pm(*x as i128)),
                    "y" => Value::Measurement(MeasurementValue::length_pm(*y as i128)),
                    _ => return Err(EvalError::FieldNotFound {
                        field: field_name.clone(),
                        struct_name: CompactString::new("Point2D"),
                    }),
                };
                self.stack[base + dst as usize] = result;
            }
            Value::PlacedPort(p) => {
                let result = match field_name.as_str() {
                    "x" => Value::Measurement(MeasurementValue::length_pm(p.world_x as i128)),
                    "y" => Value::Measurement(MeasurementValue::length_pm(p.world_y as i128)),
                    "layer" => Value::String(p.layer.clone()),
                    "name" => Value::String(p.port_name.clone()),
                    "cell" => Value::String(p.cell_name.clone()),
                    "instance" => Value::String(p.instance_name.clone()),
                    "net" => p.net.map(Value::NetHandle).unwrap_or(Value::Void),
                    _ => return Err(EvalError::FieldNotFound {
                        field: field_name.clone(),
                        struct_name: CompactString::new("PlacedPort"),
                    }),
                };
                self.stack[base + dst as usize] = result;
            }
            Value::Point3D { x, y, z } => {
                let result = match field_name.as_str() {
                    "x" => Value::Measurement(MeasurementValue::length_pm(*x as i128)),
                    "y" => Value::Measurement(MeasurementValue::length_pm(*y as i128)),
                    "z" => Value::Measurement(MeasurementValue::length_pm(*z as i128)),
                    _ => return Err(EvalError::FieldNotFound {
                        field: field_name.clone(),
                        struct_name: CompactString::new("Point3D"),
                    }),
                };
                self.stack[base + dst as usize] = result;
            }
            Value::Vector2D { dx, dy } => {
                let result = match field_name.as_str() {
                    "dx" => Value::Measurement(MeasurementValue::length_pm(*dx as i128)),
                    "dy" => Value::Measurement(MeasurementValue::length_pm(*dy as i128)),
                    _ => return Err(EvalError::FieldNotFound {
                        field: field_name.clone(),
                        struct_name: CompactString::new("Vector2D"),
                    }),
                };
                self.stack[base + dst as usize] = result;
            }
            Value::BoundingBox { min_x, min_y, max_x, max_y } => {
                let result = match field_name.as_str() {
                    "min_x" => Value::Measurement(MeasurementValue::length_pm(*min_x as i128)),
                    "min_y" => Value::Measurement(MeasurementValue::length_pm(*min_y as i128)),
                    "max_x" => Value::Measurement(MeasurementValue::length_pm(*max_x as i128)),
                    "max_y" => Value::Measurement(MeasurementValue::length_pm(*max_y as i128)),
                    _ => return Err(EvalError::FieldNotFound {
                        field: field_name.clone(),
                        struct_name: CompactString::new("BoundingBox"),
                    }),
                };
                self.stack[base + dst as usize] = result;
            }
            other => return Err(EvalError::TypeMismatch {
                expected: "StructInstance, EnumType, Point2D, Point3D, Vector2D, or BoundingBox",
                found: other.type_name().to_string(),
            }),
        }
        Ok(())
    }

    pub(crate) fn handle_set_field(
        &mut self,
        frame_idx: usize,
        base: usize,
        obj: u16,
        field_idx: u16,
        src: u16,
    ) -> Result<(), EvalError> {
        let field_name = self.frames[frame_idx].chunk.constants[field_idx as usize].as_compact_str()?.clone();
        let src_val = self.stack[base + src as usize].clone();
        let target_obj = &mut self.stack[base + obj as usize];
        match target_obj {
            Value::StructInstance { fields, .. } => {
                let mut new_fields = (**fields).clone();
                if let Some(entry) = new_fields.iter_mut().find(|(k, _)| k == &field_name) {
                    entry.1 = src_val;
                    *fields = Arc::new(new_fields);
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn handle_get_index(
        &mut self,
        base: usize,
        dst: u16,
        obj: u16,
        index: u16,
    ) -> Result<(), EvalError> {
        let target_obj = &self.stack[base + obj as usize];
        let idx_val = &self.stack[base + index as usize];
        match (target_obj, idx_val) {
            (Value::Array(items) | Value::Tuple(items), Value::Int(i)) => {
                if *i >= 0 && (*i as usize) < items.len() {
                    self.stack[base + dst as usize] = items[*i as usize].clone();
                } else {
                    self.stack[base + dst as usize] = Value::Void;
                }
            }
            _ => {
                self.stack[base + dst as usize] = Value::Void;
            }
        }
        Ok(())
    }
}
