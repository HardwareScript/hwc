use compact_str::CompactString;

#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
    #[error("Invalid measurement unit for conversion: {0}")]
    InvalidUnit(String),

    #[error("Missing required property '{property}' in material '{material}'")]
    MissingProperty {
        material: CompactString,
        property: String,
    },

    #[error("Missing profile constraint: {0}")]
    MissingProfileConstraint(String),
}
