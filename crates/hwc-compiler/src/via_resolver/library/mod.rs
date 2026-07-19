mod csg_eval;
mod eval_env;
mod math_parser;
mod shape_eval;

pub use shape_eval::{evaluate_geometry_blocks, evaluate_shape_points};

/// Parameters describing a single physical via stack to insert.
#[derive(Debug, Clone)]
pub struct ViaStackRequest {
    pub x: i64,
    pub y: i64,
    pub from_layer: usize,
    pub to_layer: usize,
    pub from_material: CompactString,
    pub to_material: CompactString,
}

use crate::ir::errors::IrError;
use clipper2_rust::Path64;
use compact_str::CompactString;

/// Via type definition for standard via library.
/// The compiler only understands polygons (Path64 contours), not named shapes.
#[derive(Debug, Clone)]
pub struct ViaType {
    pub name: CompactString,
    pub material: CompactString,
    pub from_material: CompactString,
    pub to_material: CompactString,
    pub from_layer: usize,
    pub to_layer: usize,
    pub diameter_mm: f64,
    pub min_enclosure_mm: f64,
    pub z_start_nm: i64,
    pub z_end_nm: i64,
    pub contour: Path64,
}

/// Parameters for constructing a [`ViaType`].
#[derive(Debug, Clone)]
pub struct ViaTypeSpec {
    pub name: CompactString,
    pub material: CompactString,
    pub from_material: CompactString,
    pub to_material: CompactString,
    pub from_layer: usize,
    pub to_layer: usize,
    pub diameter_mm: f64,
    pub min_enclosure_mm: f64,
    pub z_start_nm: i64,
    pub z_end_nm: i64,
    pub contour: Path64,
}

impl ViaType {
    pub fn new(spec: ViaTypeSpec) -> Self {
        Self {
            name: spec.name,
            material: spec.material,
            from_material: spec.from_material,
            to_material: spec.to_material,
            from_layer: spec.from_layer,
            to_layer: spec.to_layer,
            diameter_mm: spec.diameter_mm,
            min_enclosure_mm: spec.min_enclosure_mm,
            z_start_nm: spec.z_start_nm,
            z_end_nm: spec.z_end_nm,
            contour: spec.contour,
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
        bridge_table: &crate::bridge_resolver::BridgeTable,
        _fabrication: Option<&hwc_engine::constraint_manager::FabricationConstraints>,
        symbol_table: Option<&crate::SymbolTable>,
    ) -> Result<Self, crate::ir::errors::IrError> {
        let mut vias = Vec::new();

        if let Some(profile) = profile {
            let min_diameter_mm = profile
                .via
                .as_ref()
                .map(|v| Self::measurement_to_mm(&v.min_diameter))
                .ok_or_else(|| IrError::MissingAsicConstraint {
                    message: format!(
                        "Profile '{}' is missing 'via: min_diameter'. ASIC designs require explicit via constraints.",
                        profile.name
                    ),
                    hint: "Add 'via: [min_diameter: 0.22um, ...]' to your profile definition.".into(),
                })?;
            let size_nm = (min_diameter_mm * 1_000_000.0) as i64;

            let default_contour = if let Some(shape_def) =
                profile.via.as_ref().and_then(|v| v.shape.as_ref())
            {
                let resolved = symbol_table.and_then(|st| st.get_shape(shape_def.name.as_str()));
                if let Some(def) = resolved {
                    let constants = symbol_table
                        .map(|st| st.get_all_constants())
                        .unwrap_or_default();
                    evaluate_shape_points(def, size_nm, &constants)
                } else {
                    match shape_def.name.as_str() {
                        "square" => crate::shape_generators::square_contour(size_nm),
                        _ => crate::shape_generators::circle_contour(size_nm, 16),
                    }
                }
            } else {
                crate::shape_generators::circle_contour(size_nm, 16)
            };

            // v0.1.8 Native PDK-Driven Via Generation
            // Instead of guessing layer adjacencies, we strictly follow the 'bridge' rules
            // defined in the profile. Each bridge rule defines a valid physical transition.
            if profile.is_asic() {
                println!("\n🔍 [VIA LIBRARY] Building via library from bridge rules...");
                println!(
                    "   Bridge table has {} rules",
                    bridge_table.all_rules().len()
                );

                for (key, stack) in bridge_table.all_rules() {
                    let parts: Vec<&str> = key.split(':').collect();
                    if parts.len() != 2 {
                        continue;
                    }
                    let from_material = parts[0];
                    let to_material = parts[1];

                    println!(
                        "   Processing bridge rule: {} -> {}",
                        from_material, to_material
                    );
                    println!(
                        "     Interface: {}, Fill: {}",
                        stack.interface_material, stack.fill_material
                    );

                    // v0.1.8: For same-material bridges (e.g. Aluminum->Aluminum), only use exact matches.
                    // Category matching should only apply to different materials that are physically compatible.
                    let allow_category_match = from_material != to_material;

                    // Find all layers that match these materials or compatible categories
                    let from_layers: Vec<usize> = stackup_manager
                        .ordered_layers()
                        .iter()
                        .enumerate()
                        .filter(|(i, layer_name)| {
                            let stack_mat = stackup_manager.get_material_for_layer_index(*i);
                            if let (Some(stack_mat_name), Some(st)) = (stack_mat, symbol_table) {
                                println!(
                                    "       Checking layer {} ({}): material = {}",
                                    i, layer_name, stack_mat_name
                                );

                                if stack_mat_name == from_material {
                                    println!("         ✓ Exact match!");
                                    return true;
                                }

                                // v0.1.8: Category-based compatibility
                                // If materials share the same category (e.g. both are Semiconductors),
                                // they are physically compatible for the same stackup layer.
                                // IMPORTANT: Only use category matching for different materials!
                                if allow_category_match {
                                    let stack_cat =
                                        st.get_material(&stack_mat_name).map(|m| &m.category);
                                    let from_cat =
                                        st.get_material(from_material).map(|m| &m.category);

                                    if let (Ok(sc), Ok(fc)) = (stack_cat, from_cat) {
                                        if sc == fc {
                                            println!(
                                                "         ✓ Category match: {:?} == {:?}",
                                                sc, fc
                                            );
                                            return true;
                                        }
                                    }
                                }
                            }
                            false
                        })
                        .map(|(i, _)| i)
                        .collect();

                    println!(
                        "     Found {} matching 'from' layers: {:?}",
                        from_layers.len(),
                        from_layers
                    );

                    let to_layers: Vec<usize> = stackup_manager
                        .ordered_layers()
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| {
                            let stack_mat = stackup_manager.get_material_for_layer_index(*i);
                            if let (Some(stack_mat_name), Some(st)) = (stack_mat, symbol_table) {
                                if stack_mat_name == to_material {
                                    return true;
                                }

                                // Only use category matching for different materials
                                if allow_category_match {
                                    let stack_cat =
                                        st.get_material(&stack_mat_name).map(|m| &m.category);
                                    let to_cat = st.get_material(to_material).map(|m| &m.category);

                                    if let (Ok(sc), Ok(tc)) = (stack_cat, to_cat) {
                                        return sc == tc;
                                    }
                                }
                            }
                            false
                        })
                        .map(|(i, _)| i)
                        .collect();

                    println!(
                        "     Found {} matching 'to' layers: {:?}",
                        to_layers.len(),
                        to_layers
                    );

                    for &from_idx in &from_layers {
                        for &to_idx in &to_layers {
                            // Only bridge in the direction defined (or handle both if intended)
                            // Usually bridges are bottom-to-top in the stackup.
                            if from_idx >= to_idx {
                                println!(
                                    "     Skipping: Layer {} >= Layer {} (must be ascending)",
                                    from_idx, to_idx
                                );
                                continue;
                            }

                            let from_layer_name = &stackup_manager.ordered_layers()[from_idx];
                            let to_layer_name = &stackup_manager.ordered_layers()[to_idx];

                            let from_bottom_z =
                                stackup_manager.get_layer_bottom_z(from_idx).unwrap_or(0);
                            let to_top_z = stackup_manager.get_layer_top_z(to_idx).unwrap_or(0);

                            let via_name = format!("via_{}_to_{}", from_layer_name, to_layer_name);

                            println!(
                                "     ✅ Creating via: {} (Layer {} -> Layer {})",
                                via_name, from_idx, to_idx
                            );
                            println!("        Z range: {} to {}", from_bottom_z, to_top_z);

                            vias.push(ViaType::new(ViaTypeSpec {
                                name: via_name.into(),
                                material: stack.fill_material.clone(),
                                from_material: from_material.into(),
                                to_material: to_material.into(),
                                from_layer: from_idx,
                                to_layer: to_idx,
                                diameter_mm: min_diameter_mm,
                                min_enclosure_mm: 0.0, // enclosure handled by annular ring
                                z_start_nm: from_bottom_z,
                                z_end_nm: to_top_z,
                                contour: default_contour.clone(),
                            }));
                        }
                    }
                }

                println!("\n   📊 Total vias generated: {}", vias.len());
                for (i, via) in vias.iter().enumerate() {
                    println!(
                        "   Via {}: {} -> {} (Layer {} -> Layer {})",
                        i, via.from_material, via.to_material, via.from_layer, via.to_layer
                    );
                }
            }
        }

        Ok(Self { vias })
    }

    fn measurement_to_mm(m: &hwc_parser::Measurement) -> f64 {
        match m.unit {
            hwc_parser::Unit::Nanometer => m.value / 1_000_000.0,
            hwc_parser::Unit::Micrometer => m.value / 1_000.0,
            hwc_parser::Unit::Millimeter => m.value,
            _ => m.value * 10.0, // cm
        }
    }

    pub fn find_via_for_layers(&self, from: usize, to: usize, _is_power: bool) -> Option<&ViaType> {
        self.vias
            .iter()
            .find(|v| v.from_layer == from && v.to_layer == to)
    }

    pub fn find_via_by_z_span(&self, start_z: i64, end_z: i64) -> Option<&ViaType> {
        self.vias
            .iter()
            .find(|v| v.z_start_nm == start_z && v.z_end_nm == end_z)
    }

    /// Find a via that can bridge between two layers with specific materials.
    /// Provides detailed diagnostics when no via is found.
    pub fn find_via(
        &self,
        from_layer: usize,
        to_layer: usize,
        from_material: &str,
        to_material: &str,
        _stackup_manager: &crate::ir::stackup_manager::StackupManager,
    ) -> Result<ViaType, IrError> {
        eprintln!("   Searching for via at Layer {}:", from_layer);
        eprintln!(
            "     Looking for: from_layer={}, to_layer<={}",
            from_layer, to_layer
        );

        // First pass: find candidates that match layer criteria
        let layer_candidates: Vec<&ViaType> = self
            .vias
            .iter()
            .filter(|v| v.from_layer == from_layer && v.to_layer <= to_layer)
            .collect();

        eprintln!(
            "     Found {} candidates matching layer criteria",
            layer_candidates.len()
        );
        for (i, via) in layer_candidates.iter().enumerate() {
            eprintln!(
                "       Candidate {}: {} -> {} (Layer {} -> {})",
                i, via.from_material, via.to_material, via.from_layer, via.to_layer
            );
        }

        // Second pass: filter by material compatibility
        // Prioritize vias that go directly to the target layer
        let material_match = layer_candidates
            .iter()
            .filter(|v| {
                // Exact material match
                v.from_material == from_material && v.to_material == to_material
            })
            .max_by_key(|v| v.to_layer); // Prefer via that goes furthest (closest to target)

        if let Some(via) = material_match {
            return Ok((*via).clone());
        }

        // No match found - provide detailed diagnostic
        eprintln!("     ❌ No matching via found!");
        eprintln!(
            "        This means none of the {} vias matched the material criteria",
            self.vias.len()
        );

        // Diagnostic: check if the issue is missing bridge rules or incorrect layer detection
        let has_any_from_layer = self.vias.iter().any(|v| v.from_layer == from_layer);
        let has_any_to_layer = self.vias.iter().any(|v| v.to_layer == to_layer);
        let has_from_material = self.vias.iter().any(|v| v.from_material == from_material);
        let has_to_material = self.vias.iter().any(|v| v.to_material == to_material);

        if !has_any_from_layer {
            eprintln!("        💡 No vias start from Layer {}", from_layer);
            eprintln!("           This layer may not have any defined bridge rules");
        }

        if !has_any_to_layer {
            eprintln!("        💡 No vias go to Layer {}", to_layer);
        }

        if !has_from_material {
            eprintln!("        💡 No vias start from material '{}'", from_material);
            eprintln!("           Possible causes:");
            eprintln!("             - Material is not declared in any bridge rules");
            eprintln!(
                "             - Material exists on a layer but shouldn't (wrong layer assignment)"
            );
            eprintln!("             - Internal component geometry is using the wrong material");
        }

        if !has_to_material {
            eprintln!("        💡 No vias connect to material '{}'", to_material);
        }

        // Special diagnostic for same-material transitions
        if from_material == to_material {
            eprintln!("        ⚠️  SAME MATERIAL TRANSITION DETECTED");
            eprintln!(
                "           Trying to connect {} (Layer {}) to {} (Layer {})",
                from_material, from_layer, to_material, to_layer
            );
            eprintln!("           Common causes:");
            eprintln!("             1. Component pad shapes creating geometry on wrong layers");
            eprintln!("             2. Internal pours inheriting component placement layer instead of semantic layer");
            eprintln!(
                "             3. Pin geometry placed at component Z instead of its semantic layer"
            );
            eprintln!("           For ASIC designs, same-material layer transitions usually indicate a bug");
            eprintln!(
                "           in component unrolling or pad placement, not a missing bridge rule."
            );
        }

        Err(IrError::MissingAsicConstraint {
            message: format!(
                "FORBIDDEN JUNCTION: No physical bridge exists in PDK to connect {} (Layer {}) to {} (Layer {}). The transition is blocked at Layer {}.",
                from_material, from_layer, to_material, to_layer, from_layer
            ),
            hint: "Under ASIC technology, all physical constraints must be explicitly declared. No implicit defaults are permitted.\n\
                   Ensure your profile has 'bridge' rules for all required material transitions.".into(),
        })
    }
}
