use super::config::AutoRouter;
use crate::ir::errors::IrError;

impl<'a> AutoRouter<'a> {
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
