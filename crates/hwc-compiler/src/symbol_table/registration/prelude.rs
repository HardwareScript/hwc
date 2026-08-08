use super::super::{error::SymbolError, layer::SymbolTable, Definition};
use compact_str::CompactString;
use hwc_parser::{MaterialAliasDefinition, MaterialDefinition, UnitDefinition};

impl<'ast> SymbolTable<'ast> {
    /// Register a material alias in the prelude layer
    pub fn register_prelude_material_alias(
        &mut self,
        def: MaterialAliasDefinition,
    ) -> Result<(), SymbolError> {
        let name_str = def.name.as_str();
        if let Some(Definition::MaterialAlias(existing)) = self.prelude.get(name_str) {
            return Err(SymbolError::duplicate(
                def.name.to_string().into(),
                "material_alias",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
        }
        self.prelude
            .insert(name_str.into(), Definition::MaterialAlias(def));
        Ok(())
    }

    /// Register a unit definition in the prelude layer (for auto-loaded stdlib units)
    pub fn register_prelude_unit(&mut self, def: UnitDefinition) {
        let symbol = def.symbol.clone();
        self.prelude.insert(symbol, Definition::Unit(def));
    }

    /// Register a math constant in the prelude layer (for auto-loaded stdlib constants)
    pub fn register_prelude_constant(&mut self, name: CompactString, value: f64) {
        use hwc_parser::{ConstDefinition, Span};
        let const_def = ConstDefinition {
            name: name.clone(),
            value,
            is_exported: false,
            span: Span::new(0, 0),
        };
        self.prelude.insert(name, Definition::Const(const_def));
    }

    /// Register a definition in the prelude layer (for auto-loaded primitives)
    pub fn register_prelude_material(
        &mut self,
        def: MaterialDefinition,
    ) -> Result<(), SymbolError> {
        let name_str = def.name.as_str();
        if let Some(Definition::Material(existing)) = self.prelude.get(name_str) {
            return Err(SymbolError::duplicate(
                def.name.to_string().into(),
                "material",
                (def.span.start, def.span.end),
                Some((existing.span.start, existing.span.end)),
            ));
        }
        self.prelude
            .insert(name_str.into(), Definition::Material(def));
        Ok(())
    }
}
