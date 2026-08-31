//! Contact / via population for the v0.3.0 pipeline.
//!
//! Lowers every emitted vertical contact into a [`hwc_engine::ContactMetadata`]
//! record plus an EntityGraph `SubstrateLayer`. Contact depth and enclosure are
//! read back from the space's `profile` (mirroring the original single-pass
//! behavior).

use compact_str::CompactString;
use hwc_engine::HardwareSpace;
use hwc_parser::ast::Expression;
use hwc_parser::SpaceDecl;
use hwc_types::{ContactExemption, NetId, ViaApertureShape};
use rustc_hash::FxHashMap;

use crate::eval::MemoryEmitter;
use crate::pipeline::error::PipelineError;
use crate::symbol_table::SymbolTable;

/// Populate contacts/vias from emitted primitives and inject them into the space.
pub fn populate_contacts(
    hw_space: &mut HardwareSpace,
    space_decl: &SpaceDecl,
    mem: &MemoryEmitter,
    net_id_to_name: &FxHashMap<NetId, CompactString>,
    _symbol_table: &SymbolTable,
) -> Result<(), PipelineError> {
    // 4. Populate contacts & inject into EntityGraph
    let mut via_counters: FxHashMap<(CompactString, CompactString, Option<CompactString>), usize> =
        FxHashMap::default();

    for (idx, contact) in mem.contacts.iter().enumerate() {
        let x_nm = contact.at.0 / 1000;
        let y_nm = contact.at.1 / 1000;
        let dia_nm = contact.diameter_pm / 1000;
        let r_nm = dia_nm / 2;

        let from_st = hw_space
            .stackup_layers
            .iter()
            .find(|l| l.name == contact.from_layer)
            .ok_or_else(|| PipelineError {
                message: format!(
                    "Contact '{}' references from_layer '{}' which is not defined in profile '{}'. Available layers: {}",
                    contact.semantic_name.as_deref().unwrap_or(&format!("contact_{}", idx)),
                    contact.from_layer,
                    space_decl.profile.as_ref().map(|p| p.as_str()).unwrap_or("None"),
                    hw_space
                        .stackup_layers
                        .iter()
                        .map(|l| l.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            })?;
        let to_st = hw_space
            .stackup_layers
            .iter()
            .find(|l| l.name == contact.to_layer)
            .ok_or_else(|| PipelineError {
                message: format!(
                    "Contact '{}' references to_layer '{}' which is not defined in profile '{}'. Available layers: {}",
                    contact.semantic_name.as_deref().unwrap_or(&format!("contact_{}", idx)),
                    contact.to_layer,
                    space_decl.profile.as_ref().map(|p| p.as_str()).unwrap_or("None"),
                    hw_space
                        .stackup_layers
                        .iter()
                        .map(|l| l.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            })?;

                        
        // Read contact_depth and min_enclosure from profile
        let (_contact_depth_pct, _min_enclosure_nm) = if let Some(prof_ident) = &space_decl.profile {
            if let Ok(prof_decl) = _symbol_table.get_profile(prof_ident.as_str()) {
                let mut found_depth = None;
                let mut found_enclosure = None;
                for sec in &prof_decl.sections {
                    if sec.section_type == "via" {
                        for (field_name, field_expr) in &sec.fields {
                            if field_name == "contact_depth" {
                                match field_expr {
                                    Expression::StringLiteral { value, .. } => {
                                        if value.ends_with('%') {
                                            if let Ok(pct) = value.trim_end_matches('%').parse::<f64>() {
                                                found_depth = Some(pct / 100.0);
                                            }
                                        }
                                    }
                                    Expression::Literal { value, .. } => {
                                        found_depth = Some(*value as f64 / 100.0);
                                    }
                                    _ => {}
                                }
                            } else if field_name == "min_enclosure" {
                                if let Expression::Measurement { value, unit, .. } = field_expr {
                                    if let Ok(nm) = unit.to_nanometers(*value) {
                                        found_enclosure = Some(nm as i64);
                                    }
                                }
                            }
                        }
                    }
                }
                (found_depth.unwrap_or(0.30), found_enclosure.unwrap_or(0))
            } else {
                (0.30, 0)
            }
        } else {
            (0.30, 0)
        };

        // Calculate physical via plug start and end Z spanning the inter-layer dielectric gap
        let (via_z_start, via_z_end) = if from_st.z_top <= to_st.z_bottom {
            (from_st.z_top, to_st.z_bottom)
        } else if to_st.z_top <= from_st.z_bottom {
            (to_st.z_top, from_st.z_bottom)
        } else {
            // Overlapping or adjacent layers
            (from_st.z_bottom.min(to_st.z_bottom), from_st.z_top.max(to_st.z_top))
        };

        let (z_start_nm, z_end_nm) = (via_z_start, via_z_end);

        let footprint_r_nm = r_nm;
        let bbox = Some(hwc_engine::BoundingBox::new(
            hwc_engine::Point3D::new(x_nm - footprint_r_nm, y_nm - footprint_r_nm, z_start_nm),
            hwc_engine::Point3D::new(x_nm + footprint_r_nm, y_nm + footprint_r_nm, z_end_nm),
        ));

        let mat_name: CompactString = "Tungsten".into();
        let mat_id = hw_space
            .material_registry
            .get_id("Tungsten")
            .ok_or_else(|| PipelineError {
                message: format!(
                    "Via/contact material 'Tungsten' is not defined in the material registry. \
                     Vias require Tungsten to be declared in the material definitions. \
                     Available materials: {}",
                    hw_space
                        .material_registry
                        .all_materials()
                        .iter()
                        .map(|(_, name)| *name)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            })?;

        // Check profile for via shape specification
        let via_shape: Option<String> = if let Some(prof_ident) = &space_decl.profile {
            if let Ok(prof_decl) = _symbol_table.get_profile(prof_ident.as_str()) {
                let mut found_shape = None;
                for sec in &prof_decl.sections {
                    if sec.section_type == "via" {
                        for (field_name, field_expr) in &sec.fields {
                            if field_name == "shape" {
                                if let Expression::StringLiteral { value, .. } = field_expr {
                                    found_shape = Some(value.to_string());
                                    break;
                                } else if let Expression::Variable { name, .. } = field_expr {
                                    found_shape = Some(name.to_string());
                                    break;
                                }
                            }
                        }
                        if found_shape.is_some() {
                            break;
                        }
                    }
                }
                found_shape
            } else {
                None
            }
        } else {
            None
        };

        let mut net_name = contact.net.and_then(|id| net_id_to_name.get(&id).cloned());

        // Resolve net from touching pours if not explicit
        if net_name.is_none() {
            for pour in &hw_space.pours {
                if pour.net.is_none() {
                    continue;
                }
                if pour.layer_name == contact.from_layer || pour.layer_name == contact.to_layer {
                    if let Some(ref b) = pour.bbox {
                        if x_nm >= b.min.x - r_nm && x_nm <= b.max.x + r_nm
                            && y_nm >= b.min.y - r_nm && y_nm <= b.max.y + r_nm
                        {
                            net_name = pour.net.clone();
                            break;
                        }
                    }
                }
            }
        }

        let engine_net = if let Some(ref n) = net_name {
            if let Some(id) = mem.nets.get(n) {
                hwc_engine::netlist::NetId::new(id.0)
            } else {
                hw_space.netlist.get_or_create_net(n.as_str())
            }
        } else {
            hwc_engine::netlist::NetId::UNCONNECTED
        };

        if let Some(b) = bbox {
            let substrate_contact = if via_shape.as_deref() == Some("square") {
                hwc_engine::geometry_router::substrate_types::SubstrateLayer::new_square_via(
                    mat_id,
                    engine_net,
                    b,
                    dia_nm, // side length is exact via diameter
                )
            } else {
                hwc_engine::geometry_router::substrate_types::SubstrateLayer::new_contact_circle(
                    mat_id,
                    engine_net,
                    b,
                    footprint_r_nm,
                )
            };
            hw_space.entity_graph.substrate_layers.push(substrate_contact);
        }

        let contact_name = if let Some(semantic_name) = &contact.semantic_name {
            semantic_name.clone()
        } else {
            let counter_key = (contact.from_layer.clone(), contact.to_layer.clone(), net_name.clone());
            let counter = via_counters.entry(counter_key).or_insert(0);
            *counter += 1;

            if let Some(ref net) = net_name {
                if *counter == 1 {
                    CompactString::new(format!("Via_{}_{}_{}", net, contact.from_layer, contact.to_layer))
                } else {
                    CompactString::new(format!(
                        "Via_{}_{}_{}_{}",
                        net, contact.from_layer, contact.to_layer, *counter - 1
                    ))
                }
            } else {
                CompactString::new(format!("Via_{}_{}_{}", contact.from_layer, contact.to_layer, idx))
            }
        };

        if net_name.is_some() {
            let comp_id = hw_space
                .netlist
                .add_component(contact_name.clone(), "via".into(), (x_nm, y_nm, z_start_nm));
            let pin_virt = hw_space.netlist.add_pin(
                comp_id,
                format!("__virtual_{}", contact_name).into(),
                (0, 0, 0),
                None,
            );
            hw_space.netlist.connect_pin(pin_virt, engine_net);
        }

        let from_layer_id = hw_space.get_layer_id(&contact.from_layer);
        let to_layer_id = hw_space.get_layer_id(&contact.to_layer);
        let engine_net_id = if engine_net != hwc_engine::netlist::NetId::UNCONNECTED {
            Some(hwc_types::NetId::new(engine_net.raw()))
        } else {
            None
        };

        let aperture = if via_shape.as_deref() == Some("square") {
            ViaApertureShape::Square
        } else if via_shape.as_deref() == Some("polygon") {
            ViaApertureShape::Polygon
        } else {
            ViaApertureShape::Circular
        };

        let is_internal_head_tail =
            contact.from_layer == "polyres" || (contact.from_layer == "poly" && contact.to_layer == "li1");
        let exemption = if is_internal_head_tail {
            ContactExemption::SubcircuitInternal { device_id: 0 }
        } else {
            ContactExemption::Interconnect
        };

        hw_space.contacts.push(hwc_engine::ContactMetadata {
            name: contact_name,
            material_name: mat_name,
            material_id: Some(mat_id),
            z_start_nm,
            z_end_nm,
            net: net_name,
            net_id: engine_net_id,
            bridge: None,
            bbox,
            drill_diameter_nm: Some(dia_nm),
            is_tented: false,
            mask_clearance_diameter_nm: None,
            from_layer: Some(contact.from_layer.clone()),
            from_layer_id,
            to_layer: Some(contact.to_layer.clone()),
            to_layer_id,
            aperture,
            exemption,
        });
    }

    Ok(())
}
