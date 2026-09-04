//! HardwareScript v0.3.1 Evaluation Context, Scopes, and Diagnostic Error Types (Phase 2)

use compact_str::CompactString;
use hwc_parser::ast::{FunctionDecl, StructDecl};
use hwc_types::UnitRegistry;
use miette::Diagnostic;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use thiserror::Error;

use super::emitter::{MemoryEmitter, SpaceEmitter};
use super::sandbox::{DeterministicGuard, SandboxError};
use super::value::{SpaceId, UnitDimension, Value};

/// Comprehensive Diagnostic Errors for Compile-Time Evaluation Engine
#[derive(Error, Diagnostic, Debug, Clone, PartialEq)]
pub enum EvalError {
    #[error("Comptime Evaluation Fuel Exhausted: executed {fuel_consumed} instructions")]
    #[diagnostic(
        code(C01),
        help("A potential infinite loop was intercepted. If this large array synthesis is intentional, increase the budget using '#[comptime_fuel({suggested_fuel})]' on the space declaration.")
    )]
    FuelExhausted {
        fuel_consumed: u64,
        suggested_fuel: u64,
    },

    #[error("Recursion depth limit exceeded (Maximum {0} stack frames)")]
    #[diagnostic(
        code(C02),
        help("Comptime generators cannot recurse deeper than 256 frames. Convert recursive generators to iterative loops.")
    )]
    RecursionDepthExceeded(usize),

    #[error("Memory quota exceeded: Comptime evaluation allocated {allocated_mb} MB (Quota limit: {limit_mb} MB)")]
    #[diagnostic(
        code(C03),
        help("The design exceeded the maximum allowed memory footprint. Check for unbounded array growth or infinite collection allocation.")
    )]
    MemoryLimitExceeded {
        allocated_mb: usize,
        limit_mb: usize,
    },

    #[error("Assertion failed: {message}")]
    #[diagnostic(code(C04))]
    AssertionFailed { message: String },

    #[error("Error S10: Undefined variable or handle `{name}`")]
    #[diagnostic(code(S10))]
    UndefinedVariable { name: CompactString },

    #[error(
        "Error S14: Mutability Error: Cannot re-assign immutable variable `{name}`. Declare with `let mut {name}`."
    )]
    ImmutableAssignment { name: CompactString },

    #[error("Error S21: Missing required argument `{param}` for function `{func}`")]
    #[diagnostic(code(S21))]
    MissingArgument {
        param: CompactString,
        func: CompactString,
    },

    #[error("Type mismatch: expected {expected}, found {found}")]
    #[diagnostic(code(S22))]
    TypeMismatch {
        expected: &'static str,
        found: String,
    },

    #[error(
        "Unit Mismatch in operation '{op}': cannot combine dimension {expected:?} with {found:?}"
    )]
    #[diagnostic(code(S22))]
    UnitMismatch {
        expected: UnitDimension,
        found: UnitDimension,
        op: &'static str,
    },

    #[error("Coercion failed: expected {expected}, found {found}. {hint}")]
    #[diagnostic(code(S22))]
    CoercionFailed {
        expected: &'static str,
        found: String,
        hint: &'static str,
    },

    #[error("Invalid dimensional multiplication: {0:?} * {1:?}")]
    #[diagnostic(code(S22))]
    InvalidDimensionalMultiplication(UnitDimension, UnitDimension),

    #[error("Invalid dimensional division: {0:?} / {1:?}")]
    #[diagnostic(code(S22))]
    InvalidDimensionalDivision(UnitDimension, UnitDimension),

    #[error("Division by zero in compile-time evaluation")]
    #[diagnostic(code(S23))]
    DivisionByZero,

    #[error(
        "Error S30: No Active Space Context: Cannot perform 'space.{method}()' outside of an active space block"
    )]
    #[diagnostic(
        code(S30),
        help("Physical geometry emitters must be invoked inside a space block.")
    )]
    NoActiveSpaceContext { method: &'static str },

    #[error("Function `{name}` not found in function registry")]
    #[diagnostic(code(S10))]
    UnknownFunction { name: CompactString },

    #[error("Struct `{name}` not found in struct registry")]
    #[diagnostic(code(S10))]
    UnknownStruct { name: CompactString },

    #[error("Field `{field}` does not exist on struct `{struct_name}`")]
    #[diagnostic(code(S10))]
    FieldNotFound {
        field: CompactString,
        struct_name: CompactString,
    },

    #[error("Index {index} out of bounds for array of length {len}")]
    #[diagnostic(code(S22))]
    IndexOutOfBounds { index: i64, len: usize },

    #[error("{message}")]
    General { message: String },
}

impl From<SandboxError> for EvalError {
    fn from(err: SandboxError) -> Self {
        match err {
            SandboxError::FuelExhausted {
                fuel_consumed,
                suggested_fuel,
            } => EvalError::FuelExhausted {
                fuel_consumed,
                suggested_fuel,
            },
            SandboxError::RecursionDepthExceeded { max_depth } => {
                EvalError::RecursionDepthExceeded(max_depth)
            }
            SandboxError::MemoryLimitExceeded {
                allocated_mb,
                limit_mb,
            } => EvalError::MemoryLimitExceeded {
                allocated_mb,
                limit_mb,
            },
        }
    }
}

impl From<String> for EvalError {
    fn from(message: String) -> Self {
        Self::General { message }
    }
}

impl From<&str> for EvalError {
    fn from(msg: &str) -> Self {
        Self::General {
            message: msg.to_string(),
        }
    }
}

/// Variable binding in local scope
#[derive(Debug, Clone)]
pub struct Binding {
    pub value: Value,
    pub is_mutable: bool,
}

/// Lexical scope frame
#[derive(Debug, Default, Clone)]
pub struct ScopeFrame {
    pub bindings: FxHashMap<CompactString, Binding>,
}

impl ScopeFrame {
    pub fn new() -> Self {
        Self {
            bindings: FxHashMap::default(),
        }
    }

    pub fn bind(&mut self, name: CompactString, value: Value, is_mutable: bool) {
        self.bindings.insert(name, Binding { value, is_mutable });
    }

    pub fn get(&self, name: &str) -> Option<&Binding> {
        self.bindings.get(name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Binding> {
        self.bindings.get_mut(name)
    }
}

/// Evaluation Context for HardwareScript v0.3.1
#[derive(Debug)]
pub struct EvaluationContext {
    pub scopes: Vec<ScopeFrame>,
    pub functions: FxHashMap<CompactString, FunctionDecl>,
    pub structs: FxHashMap<CompactString, StructDecl>,
    pub enum_types: FxHashMap<CompactString, Value>,
    pub constants: FxHashMap<CompactString, Value>,
    pub current_space_id: Option<u32>,
    pub sandbox: DeterministicGuard,
    pub emitter: Box<dyn SpaceEmitter>,
    pub unit_registry: Option<Arc<UnitRegistry>>,
}

impl Default for EvaluationContext {
    fn default() -> Self {
        Self::new()
    }
}

impl EvaluationContext {
    pub fn new() -> Self {
        Self {
            scopes: vec![ScopeFrame::new()],
            functions: FxHashMap::default(),
            structs: FxHashMap::default(),
            enum_types: FxHashMap::default(),
            constants: FxHashMap::default(),
            current_space_id: None,
            sandbox: DeterministicGuard::default(),
            emitter: Box::new(MemoryEmitter::new()),
            unit_registry: None,
        }
    }

    pub fn with_emitter(emitter: Box<dyn SpaceEmitter>) -> Self {
        Self {
            scopes: vec![ScopeFrame::new()],
            functions: FxHashMap::default(),
            structs: FxHashMap::default(),
            enum_types: FxHashMap::default(),
            constants: FxHashMap::default(),
            current_space_id: None,
            sandbox: DeterministicGuard::default(),
            emitter,
            unit_registry: None,
        }
    }

    pub fn with_registry(mut self, registry: Arc<UnitRegistry>) -> Self {
        self.unit_registry = Some(registry);
        self
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(ScopeFrame::new());
    }

    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    pub fn bind(&mut self, name: impl Into<CompactString>, value: Value, is_mutable: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.bind(name.into(), value, is_mutable);
        }
    }

    pub fn insert_variable(&mut self, name: impl Into<CompactString>, value: Value) {
        let name_str: CompactString = name.into();
        self.constants.insert(name_str.clone(), value.clone());
        self.bind(name_str, value, false);
    }

    pub fn lookup(&self, name: &str) -> Option<Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(b) = scope.get(name) {
                return Some(b.value.clone());
            }
        }

        if let Some(val) = self.constants.get(name) {
            return Some(val.clone());
        }

        // Implicit contextual space handle
        if name == "space" {
            if let Some(space_id) = self.current_space_id {
                return Some(Value::SpaceHandle(SpaceId(space_id)));
            }
        }

        None
    }

    pub fn assign(&mut self, name: &str, new_value: Value) -> Result<(), EvalError> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(b) = scope.get_mut(name) {
                if !b.is_mutable {
                    return Err(EvalError::ImmutableAssignment {
                        name: CompactString::new(name),
                    });
                }
                b.value = new_value;
                return Ok(());
            }
        }
        Err(EvalError::UndefinedVariable {
            name: CompactString::new(name),
        })
    }

    pub fn enter_space(&mut self, space_id: u32) {
        self.current_space_id = Some(space_id);
        self.push_scope();
        self.bind("space", Value::SpaceHandle(SpaceId(space_id)), false);
    }

    pub fn exit_space(&mut self) {
        self.pop_scope();
        self.current_space_id = None;
    }
}
