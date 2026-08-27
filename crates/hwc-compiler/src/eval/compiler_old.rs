//! HardwareScript v0.3.0 AST-to-Bytecode Compiler
//!
//! This module is the root of the modular compiler.  Logic is split into
//! focused sub-modules, each targeting a single concern:
//!
//! | Sub-module      | Responsibility                                              |
//! |-----------------|-------------------------------------------------------------|
//! | `scope`         | `Scope` — local variable register / mutability tracking     |
//! | `core`          | `BytecodeCompiler` struct, helpers, top-level entry points  |
//! | `stmts`         | `compile_statement` — all statement arms                    |
//! | `exprs`         | `compile_expression` — all expression arms                  |
//! | `space_methods` | `compile_space_method_call` — `space.add_*` emitters        |
//! | `string_interp` | `parse_interpolated_string_template` string helper          |

pub mod core;
pub mod exprs;
pub mod scope;
pub mod space_methods;
pub mod stmts;
pub mod string_interp;

pub use core::BytecodeCompiler;
pub use scope::Scope;
