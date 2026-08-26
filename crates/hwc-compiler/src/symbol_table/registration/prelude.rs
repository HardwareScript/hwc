use super::super::{error::SymbolError, layer::SymbolTable, Definition};
use hwc_parser::MaterialDecl;

impl SymbolTable {
    /// Register a definition in the prelude layer (for auto-loaded primitives)
    pub fn register_prelude_material(
        &mut self,
        def: MaterialDecl,
    ) -> Result<(), SymbolError> {
        let name_str = def.name.name.as_str().to_string();
        if let Some(Definition::Material(existing)) = self.prelude.get(name_str.as_str()) {
            return Err(SymbolError::duplicate(
                name_str.into(),
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
