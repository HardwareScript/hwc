//! Compilation of `space.*` method calls (add_polygon, add_contact, add_device)

use hwc_parser::ast::{NamedOrPositionalArg, Span};
use rustc_hash::FxHashMap;

use crate::eval::context::EvalError;
use crate::eval::opcodes::{OpCode, Register};

use super::BytecodeCompiler;

impl<'a> BytecodeCompiler<'a> {
    /// Compile `space.add_*` physical emitter method calls
    pub fn compile_space_method_call(
        &mut self,
        method: &str,
        args: &[NamedOrPositionalArg],
        span: Span,
    ) -> Result<Register, EvalError> {
        eprintln!("[BYTECODE DEBUG] Compiling space.{} with {} args", method, args.len());
        let mut arg_map = FxHashMap::default();
        for arg in args {
            if let Some(name) = &arg.name {
                let r = self.compile_expression(&arg.value)?;
                arg_map.insert(name.as_str(), r);
            }
        }

        let dst = self.alloc_reg();
        match method {
            "add_polygon" => {
                self.compile_add_polygon(&arg_map, dst, span)
            }

            "add_contact" => {
                self.compile_add_contact(&arg_map, dst, span)
            }

            "add_device" => {
                self.compile_add_device(&arg_map, dst, span)
            }

            _ => Err(EvalError::General {
                message: format!("Unknown space method 'space.{}'", method),
            }),
        }
    }

    fn compile_add_polygon(
        &mut self,
        arg_map: &FxHashMap<&str, Register>,
        dst: Register,
        span: Span,
    ) -> Result<Register, EvalError> {
        let name_r = arg_map.get("name").copied().unwrap_or_else(|| {
            let r = self.alloc_reg();
            self.chunk.emit(OpCode::LoadNull { dst: r }, span);
            r
        });
        let layer_r = arg_map.get("layer").copied().ok_or_else(|| {
            EvalError::General { message: "space.add_polygon requires 'layer'".into() }
        })?;
        let net_r = arg_map.get("net").copied().unwrap_or_else(|| {
            let r = self.alloc_reg();
            self.chunk.emit(OpCode::LoadNull { dst: r }, span);
            r
        });
        let geom_r = arg_map.get("rect").copied().or_else(|| arg_map.get("points").copied()).ok_or_else(|| {
            EvalError::General { message: "space.add_polygon requires 'rect' or 'points'".into() }
        })?;

        self.chunk.emit(
            OpCode::EmitPolygon {
                name_reg: name_r,
                layer_reg: layer_r,
                net_reg: net_r,
                points_or_rect_reg: geom_r,
            },
            span,
        );
        self.chunk.emit(OpCode::LoadNull { dst }, span);
        Ok(dst)
    }

    fn compile_add_contact(
        &mut self,
        arg_map: &FxHashMap<&str, Register>,
        dst: Register,
        span: Span,
    ) -> Result<Register, EvalError> {
        let name_r = arg_map.get("name").copied().unwrap_or_else(|| {
            let r = self.alloc_reg();
            self.chunk.emit(OpCode::LoadNull { dst: r }, span);
            r
        });
        let from_r = arg_map.get("from").copied().ok_or_else(|| {
            EvalError::General { message: "space.add_contact requires 'from'".into() }
        })?;
        let to_r = arg_map.get("to").copied().ok_or_else(|| {
            EvalError::General { message: "space.add_contact requires 'to'".into() }
        })?;
        let at_r = arg_map.get("at").copied().ok_or_else(|| {
            EvalError::General { message: "space.add_contact requires 'at'".into() }
        })?;
        let dia_r = arg_map.get("diameter").copied().ok_or_else(|| {
            EvalError::General { message: "space.add_contact requires 'diameter'".into() }
        })?;
        let net_r = arg_map.get("net").copied().unwrap_or_else(|| {
            let r = self.alloc_reg();
            self.chunk.emit(OpCode::LoadNull { dst: r }, span);
            r
        });

        self.chunk.emit(
            OpCode::EmitContact {
                name_reg: name_r,
                from_layer_reg: from_r,
                to_layer_reg: to_r,
                at_reg: at_r,
                dia_reg: dia_r,
                net_reg: net_r,
            },
            span,
        );
        self.chunk.emit(OpCode::LoadNull { dst }, span);
        Ok(dst)
    }

    fn compile_add_device(
        &mut self,
        arg_map: &FxHashMap<&str, Register>,
        dst: Register,
        span: Span,
    ) -> Result<Register, EvalError> {
        let type_r = arg_map.get("type").copied().ok_or_else(|| {
            EvalError::General { message: "space.add_device requires 'type'".into() }
        })?;
        let name_r = arg_map.get("name").copied().ok_or_else(|| {
            EvalError::General { message: "space.add_device requires 'name'".into() }
        })?;
        let term_r = arg_map.get("terminals").copied().ok_or_else(|| {
            EvalError::General { message: "space.add_device requires 'terminals'".into() }
        })?;
        let param_r = arg_map.get("params").copied().ok_or_else(|| {
            EvalError::General { message: "space.add_device requires 'params'".into() }
        })?;

        self.chunk.emit(
            OpCode::EmitDevice {
                type_reg: type_r,
                name_reg: name_r,
                terminals_reg: term_r,
                params_reg: param_r,
            },
            span,
        );
        self.chunk.emit(OpCode::LoadNull { dst }, span);
        Ok(dst)
    }
}
