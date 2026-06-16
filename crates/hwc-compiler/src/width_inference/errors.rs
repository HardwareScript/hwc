use compact_str::CompactString;
use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

#[derive(Error, Debug, Clone, Diagnostic)]
pub enum WidthError {
    #[error(
        "Width mismatch: Cannot assign {src_width}-bit value to {dst_width}-bit wire '{name}'"
    )]
    #[diagnostic(
        code(L02),
        url("https://docs.hw-script.org/errors/L02"),
        help("Use slicing to truncate: {name}[{dst_width_minus_1}..0]\nOr extend the destination: let {name}[{src_width}] = ...")
    )]
    WidthMismatch {
        #[label("{src_width}-bit value assigned to {dst_width}-bit wire")]
        span: SourceSpan,
        name: CompactString,
        src_width: usize,
        dst_width: usize,
        dst_width_minus_1: usize,
    },

    #[error("Cannot infer width for variable '{name}'")]
    #[diagnostic(
        code(L02),
        url("https://docs.hw-script.org/errors/L02"),
        help("Specify the width explicitly: let {name}[8] = ...")
    )]
    CannotInferWidth {
        #[label("width cannot be inferred")]
        span: SourceSpan,
        name: CompactString,
    },

    #[error("Width not specified for wire '{name}'")]
    #[diagnostic(
        code(L02),
        url("https://docs.hw-script.org/errors/L02"),
        help("Add bit width: let {name}[8] = ...")
    )]
    WidthNotSpecified {
        #[label("width not specified")]
        span: SourceSpan,
        name: CompactString,
    },

    #[error("Invalid bit slice [{high}..{low}] for {width}-bit wire '{name}'")]
    #[diagnostic(
        code(L02),
        url("https://docs.hw-script.org/errors/L02"),
        help("Valid range is [0..{width_minus_1}]")
    )]
    InvalidSlice {
        #[label("invalid slice range")]
        span: SourceSpan,
        name: CompactString,
        high: usize,
        low: usize,
        width: usize,
        width_minus_1: usize,
    },

    #[error(
        "Operand width mismatch: {left_width}-bit and {right_width}-bit values in {operation}"
    )]
    #[diagnostic(
        code(L02),
        url("https://docs.hw-script.org/errors/L02"),
        help("Ensure both operands have the same bit width, or use explicit casting")
    )]
    OperandWidthMismatch {
        #[label("{left_width}-bit operand")]
        left_span: Option<SourceSpan>,
        #[label("{right_width}-bit operand")]
        right_span: Option<SourceSpan>,
        operation: CompactString,
        left_width: usize,
        right_width: usize,
    },
}

#[derive(Error, Debug, Clone, Diagnostic)]
#[diagnostic(severity(Warning))]
pub enum WidthWarning {
    #[error(
        "Implicit truncation: {src_width}-bit expression assigned to {dst_width}-bit wire '{name}'"
    )]
    #[diagnostic(
        code(L10),
        url("https://docs.hw-script.org/errors/L10"),
        help("The upper {truncated_bits} bit(s) will be discarded. Use explicit slicing to make this clear: {name} = expr[{dst_width_minus_1}..0]")
    )]
    ImplicitTruncation {
        #[label("{src_width}-bit value truncated to {dst_width} bits")]
        span: SourceSpan,
        name: CompactString,
        src_width: usize,
        dst_width: usize,
        dst_width_minus_1: usize,
        truncated_bits: usize,
    },
}

pub enum WidthValidationResult {
    Ok,
    Warning(WidthWarning),
    Error(WidthError),
}
