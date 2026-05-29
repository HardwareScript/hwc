//! Error types for logic synthesis

use crate::electrical_symbol_table::ElectricalSymbolError;
use crate::span_utils::span_to_source_span;
use crate::width_inference::WidthError;
use compact_str::CompactString;
use hwc_parser::Span;
use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

/// Errors that can occur during logic synthesis
#[derive(Error, Debug, Diagnostic)]
pub enum SynthesisError {
    #[error(transparent)]
    #[diagnostic(transparent)]
    ElectricalSymbolError(Box<ElectricalSymbolError>),

    #[error(transparent)]
    #[diagnostic(transparent)]
    WidthError(#[from] WidthError),

    #[error("Combinational loop detected: {chain}")]
    #[diagnostic(
        code(L03),
        url("https://docs.hw-script.org/errors/L03"),
        help("Insert a register to break the loop:\n  let {first_wire} = reg(clock: Clk, reset: Rst, init: 0)\n  {first_wire}.next = ...")
    )]
    CombinationalLoop {
        #[label("combinational loop detected here")]
        span: SourceSpan,
        chain: CompactString,
        first_wire: CompactString,
    },

    #[error("Unknown enum variant '{variant}' for enum '{enum_name}'")]
    #[diagnostic(
        code(L07),
        url("https://docs.hw-script.org/errors/L07"),
        help("Valid variants are defined in the enum declaration")
    )]
    UnknownEnumVariant {
        #[label("unknown variant")]
        span: SourceSpan,
        enum_name: CompactString,
        variant: CompactString,
    },

    #[error("Unknown field '{field}' for struct '{struct_name}'")]
    #[diagnostic(
        code(L08),
        url("https://docs.hw-script.org/errors/L08"),
        help("Check the struct definition for valid field names")
    )]
    UnknownStructField {
        #[label("unknown field")]
        span: SourceSpan,
        struct_name: CompactString,
        field: CompactString,
    },

    #[error("Standard library component '{component}' not found")]
    #[diagnostic(
        code(C24),
        url("https://docs.hw-script.org/errors/C24"),
        help("Check that the standard library is properly installed")
    )]
    StdlibComponentNotFound {
        #[label("component not found")]
        span: Option<SourceSpan>,
        component: CompactString,
    },

    #[error("Field access on non-struct type '{base_type}'")]
    #[diagnostic(
        code(L09),
        url("https://docs.hw-script.org/errors/L09"),
        help("Field access is only valid on struct types")
    )]
    InvalidFieldAccess {
        #[label("invalid field access")]
        span: SourceSpan,
        base_type: CompactString,
    },

    #[error("Internal synthesis error: {message}")]
    #[diagnostic(
        code(L99),
        url("https://docs.hw-script.org/errors/L99"),
        help("This is a compiler bug. Please report it at https://github.com/hardware-script/hardware-script/issues")
    )]
    Internal {
        #[label("internal error occurred here")]
        span: Option<SourceSpan>,
        message: CompactString,
    },
}

impl SynthesisError {
    /// Create a combinational loop error with span
    pub fn combinational_loop(span: Span, chain: CompactString) -> Self {
        let first_wire = chain.split(" → ").next().unwrap_or("wire").to_string();
        Self::CombinationalLoop {
            span: span_to_source_span(&span),
            chain,
            first_wire: first_wire.into(),
        }
    }

    /// Create an unknown enum variant error with span
    pub fn unknown_enum_variant(
        span: Span,
        enum_name: CompactString,
        variant: CompactString,
    ) -> Self {
        Self::UnknownEnumVariant {
            span: span_to_source_span(&span),
            enum_name,
            variant,
        }
    }

    /// Create an unknown struct field error with span
    pub fn unknown_struct_field(
        span: Span,
        struct_name: CompactString,
        field: CompactString,
    ) -> Self {
        Self::UnknownStructField {
            span: span_to_source_span(&span),
            struct_name,
            field,
        }
    }

    /// Create a stdlib component not found error
    pub fn stdlib_component_not_found(component: CompactString, span: Option<Span>) -> Self {
        Self::StdlibComponentNotFound {
            span: span.as_ref().map(span_to_source_span),
            component,
        }
    }

    /// Create an invalid field access error with span
    pub fn invalid_field_access(span: Span, base_type: CompactString) -> Self {
        Self::InvalidFieldAccess {
            span: span_to_source_span(&span),
            base_type,
        }
    }

    /// Create an internal error
    pub fn internal(message: CompactString, span: Option<Span>) -> Self {
        Self::Internal {
            span: span.as_ref().map(span_to_source_span),
            message,
        }
    }
}

impl From<ElectricalSymbolError> for SynthesisError {
    fn from(err: ElectricalSymbolError) -> Self {
        Self::ElectricalSymbolError(Box::new(err))
    }
}
