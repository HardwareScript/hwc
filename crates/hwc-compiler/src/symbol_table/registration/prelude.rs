use super::super::{error::SymbolError, layer::SymbolTable, Definition};
use compact_str::CompactString;
use hwc_parser::{MaterialAliasDefinition, MaterialDefinition, UnitDefinition};

impl SymbolTable {
    /// Register a material alias in the prelude layer
    pub fn register_prelude_material_alias(
        &mut self,
        def: MaterialAliasDefinition,
    ) -> Result<(), SymbolError> {
        let name_str = def.name.as_str().to_string();
        if let Some(Definition::MaterialAlias(existing)) = self.prelude.get(name_str.as_str()) {
            return Err(SymbolError::duplicate(
                def.name.to_string().into(),
                "material_alias",
                (def.span.start, def.span.end),
                Some((
                    self.arena.material_alias_defs[*existing].span.start,
                    self.arena.material_alias_defs[*existing].span.end,
                )),
            ));
        }
        let id = self.arena.material_alias_defs.push(def);
        self.prelude
            .insert(name_str.into(), Definition::MaterialAlias(id));
        Ok(())
    }

    /// Register a unit definition in the prelude layer (for auto-loaded stdlib units)
    pub fn register_prelude_unit(&mut self, def: UnitDefinition) {
        let symbol = def.symbol.clone();
        let id = self.arena.unit_defs.push(def);
        self.prelude.insert(symbol, Definition::Unit(id));
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
        let id = self.arena.const_defs.push(const_def);
        self.prelude.insert(name, Definition::Const(id));
    }

    /// Register a definition in the prelude layer (for auto-loaded primitives)
    pub fn register_prelude_material(
        &mut self,
        def: MaterialDefinition,
    ) -> Result<(), SymbolError> {
        let name_str = def.name.as_str().to_string();
        if let Some(Definition::Material(existing)) = self.prelude.get(name_str.as_str()) {
            return Err(SymbolError::duplicate(
                def.name.to_string().into(),
                "material",
                (def.span.start, def.span.end),
                Some((
                    self.arena.material_defs[*existing].span.start,
                    self.arena.material_defs[*existing].span.end,
                )),
            ));
        }
        let id = self.arena.material_defs.push(def);
        self.prelude
            .insert(name_str.into(), Definition::Material(id));
        Ok(())
    }
}
