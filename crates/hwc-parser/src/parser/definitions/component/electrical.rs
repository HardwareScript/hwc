//! Electrical properties parsing

use super::super::super::error::ParseError;
use crate::ast::*;

impl<'ast> super::super::super::Parser<'ast> {
    pub(super) fn parse_electrical_block(&mut self) -> Result<ElectricalBlock, ParseError> {
        // Use the universal property block parser (enforces ':' for declarative properties)
        let properties = self.parse_property_block()?;
        Ok(ElectricalBlock { properties })
    }
}
