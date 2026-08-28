//! Pipeline-level errors for the v0.3.0 compilation entry points.

/// Error returned by v0.3.0 pipeline stub functions.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("v0.3.0 pipeline: {message}")]
#[diagnostic(
    code(P00),
    help(
        "The v0.2.x `program_to_space` pipeline has been replaced by `evaluate_program` + \
         SpaceEmitter in v0.3.0. Wire up the comptime evaluator in `hwc-cli/build_cmd` to \
         use the new API."
    )
)]
pub struct PipelineError {
    pub message: String,
}
