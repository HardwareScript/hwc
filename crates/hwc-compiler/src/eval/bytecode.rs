//! HardwareScript v0.3.0 Bytecode Instruction Set Architecture (ISA)
//!
//! Stack-register hybrid instruction format consumed by the VM dispatch loop.

use super::value::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
        func: Register,
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
    BuiltinCall {
        builtin_id: u8,
        args_start: Register,
        arg_count: u8,
        dst: Register,
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
        type_idx: ConstantIndex,
        name_reg: Register,
        terminals_reg: Register,
        params_reg: Register,
    },
    EmitRoute {
        from_reg: Register,
        to_reg: Register,
        intent_idx: ConstantIndex,
    },

    // ── Diagnostics ──
    Assert {
        cond: Register,
        msg_idx: ConstantIndex,
    },
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Chunk {
    pub code: Vec<OpCode>,
    pub constants: Vec<Value>,
    pub spans: Vec<miette::SourceSpan>,
}
