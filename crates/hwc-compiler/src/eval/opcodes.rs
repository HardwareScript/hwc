//! HardwareScript v0.3.0 Bytecode Instruction Set Architecture (ISA)
//!
//! Stack-register hybrid instruction format consumed by the VM dispatch loop.

use compact_str::CompactString;
use super::value::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Register(pub u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConstantIndex(pub u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JumpOffset(pub i32);

#[derive(Debug, Clone, PartialEq)]
pub enum OpCode {
    // ── Constants & Stack Management ──
    LoadConst {
        dst: Register,
        const_idx: ConstantIndex,
    },
    Move {
        dst: Register,
        src: Register,
    },
    LoadNull {
        dst: Register,
    },
    LoadBool {
        dst: Register,
        val: bool,
    },
    LoadInt {
        dst: Register,
        val: i64,
    },
    LoadFloat {
        dst: Register,
        val: f64,
    },

    // ── Arithmetic & Dimensional Math ──
    Add {
        dst: Register,
        lhs: Register,
        rhs: Register,
    },
    Sub {
        dst: Register,
        lhs: Register,
        rhs: Register,
    },
    Mul {
        dst: Register,
        lhs: Register,
        rhs: Register,
    },
    Div {
        dst: Register,
        lhs: Register,
        rhs: Register,
    },
    Mod {
        dst: Register,
        lhs: Register,
        rhs: Register,
    },
    Neg {
        dst: Register,
        src: Register,
    },

    // ── Comparison & Logic ──
    Eq {
        dst: Register,
        lhs: Register,
        rhs: Register,
    },
    Ne {
        dst: Register,
        lhs: Register,
        rhs: Register,
    },
    Lt {
        dst: Register,
        lhs: Register,
        rhs: Register,
    },
    Le {
        dst: Register,
        lhs: Register,
        rhs: Register,
    },
    Gt {
        dst: Register,
        lhs: Register,
        rhs: Register,
    },
    Ge {
        dst: Register,
        lhs: Register,
        rhs: Register,
    },
    Not {
        dst: Register,
        src: Register,
    },
    And {
        dst: Register,
        lhs: Register,
        rhs: Register,
    },
    Or {
        dst: Register,
        lhs: Register,
        rhs: Register,
    },

    // ── Control Flow & Loops ──
    Jump {
        offset: JumpOffset,
    },
    JumpIfTrue {
        cond: Register,
        offset: JumpOffset,
    },
    JumpIfFalse {
        cond: Register,
        offset: JumpOffset,
    },
    LoopStep {
        iter_reg: Register,
        end_reg: Register,
        step_val: i64,
        offset: JumpOffset,
    },

    // ── Functions & Call Stack ──
    Call {
        func_name_idx: ConstantIndex,
        args_start: Register,
        arg_count: u8,
        dst: Register,
    },
    Return {
        val: Register,
    },

    // ── Data Structures & Coercion ──
    AllocArray {
        dst: Register,
        start_reg: Register,
        count: u16,
    },
    AllocStruct {
        dst: Register,
        struct_name_idx: ConstantIndex,
        fields_start: Register,
        count: u16,
    },
    GetField {
        dst: Register,
        obj: Register,
        field_idx: ConstantIndex,
    },
    SetField {
        obj: Register,
        field_idx: ConstantIndex,
        src: Register,
    },
    GetIndex {
        dst: Register,
        obj: Register,
        index: Register,
    },
    CoercePoint2D {
        dst: Register,
        src: Register,
    },
    CoerceType {
        dst: Register,
        src: Register,
        type_name_idx: ConstantIndex,
    },
    BuiltinCall {
        builtin_id: u8,
        args_start: Register,
        arg_count: u8,
        dst: Register,
    },
    InterpolateString {
        dst: Register,
        pattern_idx: ConstantIndex,
        args_start: Register,
        arg_count: u8,
    },

    // ── Native Emitter Operations (`space.*`) ──
    EmitPolygon {
        layer_reg: Register,
        net_reg: Register,
        points_or_rect_reg: Register,
    },
    EmitContact {
        from_layer_reg: Register,
        to_layer_reg: Register,
        at_reg: Register,
        dia_reg: Register,
        net_reg: Register,
    },
    EmitDevice {
        type_reg: Register,
        name_reg: Register,
        terminals_reg: Register,
        params_reg: Register,
    },
    EmitRoute {
        from_reg: Register,
        to_reg: Register,
        intent_idx: ConstantIndex,
        props_reg: Register,
    },

    // ── Diagnostics ──
    Assert {
        cond: Register,
        msg_idx: ConstantIndex,
    },
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Chunk {
    pub name: CompactString,
    pub code: Vec<OpCode>,
    pub constants: Vec<Value>,
    pub spans: Vec<hwc_parser::ast::Span>,
    pub max_registers: u16,
}

impl Chunk {
    pub fn new(name: impl Into<CompactString>) -> Self {
        Self {
            name: name.into(),
            code: Vec::new(),
            constants: Vec::new(),
            spans: Vec::new(),
            max_registers: 0,
        }
    }

    pub fn add_constant(&mut self, value: Value) -> ConstantIndex {
        for (i, existing) in self.constants.iter().enumerate() {
            if existing == &value {
                return ConstantIndex(i as u16);
            }
        }
        let idx = self.constants.len() as u16;
        self.constants.push(value);
        ConstantIndex(idx)
    }

    pub fn emit(&mut self, op: OpCode, span: hwc_parser::ast::Span) -> usize {
        let idx = self.code.len();
        self.code.push(op);
        self.spans.push(span);
        idx
    }

    pub fn disassemble(&self) -> String {
        let mut out = format!("=== Chunk: {} (regs: {}, consts: {}) ===\n", self.name, self.max_registers, self.constants.len());
        for (i, op) in self.code.iter().enumerate() {
            out.push_str(&format!("{:04}  {:?}\n", i, op));
        }
        out
    }
}
