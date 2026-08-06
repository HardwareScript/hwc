use super::config::AutoRouter;
use crate::ir::errors::IrError;
use hwc_engine::netlist::NetId;

impl<'a> AutoRouter<'a> {
    pub(crate) fn find_net_id_for_name(&mut self, name: &str) -> Result<NetId, IrError> {
        let is_asic = self
            .space
            .fabrication_constraints
            .as_ref()
            .is_some_and(|c| {
                c.technology.is_asic()
            });
        let min_width = self
            .space
            .fabrication_constraints
            .as_ref()
            .map(|c| c.trace.min_width_nm)
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: format!("Net '{}' requires fabrication constraints but none are loaded.", name),
                hint: "Ensure a profile with 'trace:' constraints is declared in the space definition.".into(),
            })?;

        Ok(self
            .space
            .netlist
            .get_or_create_net_with_technology(name, is_asic, min_width))
    }

    pub(crate) fn resolve_sample_copper_id(
        &self,
    ) -> Result<hwc_engine::material::MaterialId, IrError> {
        let sample_z = self.space.resolution_nm; // Default: bottom of board
        if let Some(layer_name) = self.stackup_manager.get_layer_name_at_z(sample_z) {
            let mat_name = self
                .profile
                .and_then(|p| p.stackup.as_ref())
                .and_then(|stackup| {
                    stackup
                        .layers
                        .iter()
                        .find(|l| l.name.name == layer_name)
                        .map(|l| l.material.clone())
                })
                .ok_or_else(|| IrError::UndeclaredMaterial {
                    material: format!("No material defined for layer '{}'", layer_name).into(),
                })?;
            self.space
                .material_registry
                .get_id(&mat_name)
                .ok_or_else(|| IrError::UndeclaredMaterial { material: mat_name })
        } else {
            Err(IrError::UndeclaredMaterial {
                material: "No stackup layer found for routing material resolution".into(),
            })
        }
    }
}
