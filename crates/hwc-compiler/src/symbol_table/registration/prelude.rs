use super::super::{error::SymbolError, layer::SymbolTable};
use compact_str::CompactString;
use hwc_parser::{MaterialAliasDefinition, MaterialDefinition, UnitDefinition};

impl SymbolTable {
    /// Register a material alias in the prelude layer
    pub fn register_prelude_material_alias(
        &mut self,
        def: MaterialAliasDefinition,
    ) -> Result<(), SymbolError> {
        let name_str = def.name.as_str();
        if let Some(existing) = self.prelude.material_aliases.get(name_str) {
            return Err(SymbolError::duplicate(
                def.name.to_string().into(),
                "material_alias",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
        }
        self.prelude.material_aliases.insert(name_str.into(), def);
        Ok(())
    }

    /// Register a unit definition in the prelude layer (for auto-loaded stdlib units)
    pub fn register_prelude_unit(&mut self, def: UnitDefinition) {
        let symbol = def.symbol.clone();
        self.prelude.units.insert(symbol, def);
    }

    /// Register a math constant in the prelude layer (for auto-loaded stdlib constants)
    pub fn register_prelude_constant(&mut self, name: CompactString, value: f64) {
        use hwc_parser::{ConstDefinition, Span};
        let const_def = ConstDefinition {
            name: name.clone(),
            value,
            is_exported: false, // Prelude constants are not exported
            span: Span::new(0, 0), // Prelude constants don't have source spans
        };
        self.prelude.constants.insert(name, const_def);
    }

    /// Register a definition in the prelude layer (for auto-loaded primitives)
    pub fn register_prelude_material(
        &mut self,
        def: MaterialDefinition,
    ) -> Result<(), SymbolError> {
        let name_str = def.name.as_str();
        if let Some(existing) = self.prelude.materials.get(name_str) {
            return Err(SymbolError::duplicate(
                def.name.to_string().into(),
                "material",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
        }
        self.prelude.materials.insert(name_str.into(), def);
        Ok(())
    }
}
