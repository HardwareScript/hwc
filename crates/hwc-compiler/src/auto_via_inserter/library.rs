use compact_str::CompactString;

/// Via type definition for standard via library.
#[derive(Debug, Clone)]
pub struct ViaType {
    pub name: CompactString,
    pub material: CompactString,
    pub from_layer: usize,
    pub to_layer: usize,
    pub diameter_mm: f64,
    pub min_enclosure_mm: f64,
    pub z_start_nm: i64,
    pub z_end_nm: i64,
}

impl ViaType {
    pub fn new(
        name: CompactString,
        material: CompactString,
        from_layer: usize,
        to_layer: usize,
        diameter_mm: f64,
        min_enclosure_mm: f64,
        z_start_nm: i64,
        z_end_nm: i64,
    ) -> Self {
        Self {
            name,
            material,
            from_layer,
            to_layer,
            diameter_mm,
            min_enclosure_mm,
            z_start_nm,
            z_end_nm,
        }
    }
}

/// Standard via library with common via types.
pub struct ViaLibrary {
    pub(crate) vias: Vec<ViaType>,
}

impl ViaLibrary {
    /// Create a via library from a profile definition.
    pub fn from_profile(
        profile: Option<&hwc_parser::ProfileDefinition>,
        stackup_manager: &crate::ir::stackup_manager::StackupManager,
        _fabrication: Option<&hwc_engine::constraint_manager::FabricationConstraints>,
    ) -> Self {
        let mut vias = Vec::new();

        if let Some(profile) = profile {
            for via_def in &profile.vias {
                let from_layer = stackup_manager.get_index_for_layer(via_def.from_layer.as_str());
                let to_layer = stackup_manager.get_index_for_layer(via_def.to_layer.as_str());

                if let (Some(from), Some(to)) = (from_layer, to_layer) {
                    let z_start = stackup_manager.get_z_start_nm_for_layer_index(from);
                    let z_end = stackup_manager.get_z_start_nm_for_layer_index(to);

                    vias.push(ViaType::new(
                        via_def.name.name.clone(),
                        via_def
                            .material
                            .as_ref()
                            .map(|material| material.name.clone())
                            .unwrap_or_else(|| "Copper".into()),
                        from,
                        to,
                        Self::measurement_to_mm(&via_def.diameter),
                        Self::measurement_to_mm(&via_def.annular_ring),
                        z_start,
                        z_end,
                    ));
                }
            }
        }

        Self { vias }
    }

    fn measurement_to_mm(measurement: &hwc_parser::Measurement) -> f64 {
        match measurement.unit {
            hwc_parser::Unit::Millimeter => measurement.value,
            hwc_parser::Unit::Micrometer => measurement.value / 1000.0,
            hwc_parser::Unit::Nanometer => measurement.value / 1_000_000.0,
            hwc_parser::Unit::Centimeter => measurement.value * 10.0,
            _ => measurement.value,
        }
    }

    /// Find the appropriate via type for a layer pair.
    pub fn find_via_for_layers(
        &self,
        from_layer: usize,
        to_layer: usize,
        prefer_large: bool,
    ) -> Option<&ViaType> {
        let (start, end) = if from_layer < to_layer {
            (from_layer, to_layer)
        } else {
            (to_layer, from_layer)
        };

        let mut matches: Vec<&ViaType> = self
            .vias
            .iter()
            .filter(|via| {
                let exact = via.from_layer == start && via.to_layer == end;
                let spanning_through_hole =
                    via.from_layer == 0 && via.to_layer >= end && start >= via.from_layer;
                exact || spanning_through_hole
            })
            .collect();

        if matches.is_empty() {
            return None;
        }

        matches.sort_by(|a, b| a.diameter_mm.partial_cmp(&b.diameter_mm).unwrap());

        if prefer_large {
            matches.last().copied()
        } else {
            matches.first().copied()
        }
    }

    /// Find a via type by its exact Z-span.
    pub fn find_via_by_z_span(&self, z_start_nm: i64, z_end_nm: i64) -> Option<&ViaType> {
        let (start, end) = if z_start_nm < z_end_nm {
            (z_start_nm, z_end_nm)
        } else {
            (z_end_nm, z_start_nm)
        };

        self.vias.iter().find(|via| {
            let (v_start, v_end) = if via.z_start_nm < via.z_end_nm {
                (via.z_start_nm, via.z_end_nm)
            } else {
                (via.z_end_nm, via.z_start_nm)
            };
            v_start == start && v_end == end
        })
    }
}
