//! Validation utilities - clock domain checking

use super::{LogicSynthesizer, SynthesisError};
use crate::electrical_symbol_table::ElectricalSymbolError;

impl<'a> LogicSynthesizer<'a> {
    /// Validate clock domains: ensure all registers use the same clock
    pub(crate) fn validate_clock_domains(&self) -> Result<(), SynthesisError> {
        let domains = self.electrical_symbols.get_clock_domains();

        if domains.len() > 1 {
            let domains_str = domains.join(", ");
            // Use a default span since we don't have a specific location for this error
            return Err(SynthesisError::from(
                ElectricalSymbolError::MultipleClockDomains {
                    span: crate::span_utils::default_span(),
                    domains: domains_str.into(),
                },
            ));
        }

        Ok(())
    }
}
