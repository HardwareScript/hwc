use hwc_engine::{
    space::{ContactMetadata, KeepOutZone, NetClassification},
    HardwareSpace,
};
use hwc_parser::ContactPlacement;

use super::placement::ViaPlacementParams;
use super::{LayerTransition, OverlapRegion, ViaLibrary, ViaLocation};
use crate::ir::errors::IrError;

pub(crate) struct ViaInsertionContext<'a> {
    pub(crate) transition: &'a LayerTransition,
    pub(crate) overlap: &'a OverlapRegion,
    pub(crate) is_power_or_ground: bool,
    pub(crate) contacts: &'a [ContactMetadata],
    pub(crate) auto_via_metadata: &'a [ContactMetadata],
    pub(crate) keep_out_zones: &'a [KeepOutZone],
}

/// Automatic via inserter.
pub struct AutoViaInserter {
    pub(crate) via_library: ViaLibrary,
    pub(crate) min_spacing_nm: i64,
}

impl AutoViaInserter {
    /// Create a new auto via inserter from a profile definition.
    ///
    /// Returns an error if fabrication constraints are not provided, since the
    /// via inserter requires explicit spacing rules to place vias correctly.
    pub fn from_profile(
        profile: Option<&hwc_parser::ProfileDefinition>,
        stackup_manager: &crate::ir::stackup_manager::StackupManager,
        fabrication: &hwc_engine::constraint_manager::FabricationConstraints,
        symbol_table: Option<&crate::SymbolTable>,
    ) -> Result<Self, IrError> {
        Ok(Self {
            via_library: ViaLibrary::from_profile(
                profile,
                stackup_manager,
                Some(fabrication),
                symbol_table,
            ),
            min_spacing_nm: fabrication.min_spacing_nm,
        })
    }

    /// Insert vias automatically for all nets that transition between layers.
    pub fn insert_vias(
        &self,
        space: &HardwareSpace,
        profile: Option<&hwc_parser::ProfileDefinition>,
        stackup_manager: &crate::ir::stackup_manager::StackupManager,
    ) -> Result<Vec<ContactPlacement>, String> {
        let mut inserted_vias = Vec::new();
        let mut auto_via_metadata: Vec<ContactMetadata> = Vec::new();

        // Phase 1: Detect transitions from pours (existing logic)
        let pours_by_net = self.group_pours_by_net(&space.pours);

        println!("\n🔌 Auto Via Insertion:");
        println!(
            "   ├─ Analyzing {} nets for layer transitions...",
            pours_by_net.len()
        );

        for (net_name, pours) in &pours_by_net {
            let transitions = self.find_layer_transitions(net_name, pours, stackup_manager);
            if transitions.is_empty() {
                continue;
            }

            let is_power_or_ground = space
                .net_classifications
                .get(net_name.as_str())
                .map(|classification| {
                    matches!(
                        classification,
                        NetClassification::Power | NetClassification::Ground
                    )
                })
                .unwrap_or(false);

            println!(
                "   ├─ Net '{}' ({}): {} layer transition(s) detected",
                net_name,
                if is_power_or_ground {
                    "power/ground"
                } else {
                    "signal"
                },
                transitions.len()
            );

            for transition in transitions {
                let overlap = match self.find_overlap(&transition.from_bbox, &transition.to_bbox) {
                    Ok(o) => o,
                    Err(error) => {
                        println!(
                            "   │  ├─ ⚠️  Could not insert via for transition {} → {}: {}",
                            transition.from_pour, transition.to_pour, error
                        );
                        continue;
                    }
                };
                let ctx = ViaInsertionContext {
                    transition: &transition,
                    overlap: &overlap,
                    is_power_or_ground,
                    contacts: &space.contacts,
                    auto_via_metadata: &auto_via_metadata,
                    keep_out_zones: &space.keep_out_zones,
                };
                match self.process_transition(&ctx, profile) {
                    Ok(vias) => {
                        if vias.len() > 1 {
                            println!(
                                "   │  ├─ Auto-inserted {} vias in array for power distribution",
                                vias.len()
                            );
                        } else if vias.len() == 1 {
                            println!(
                                "   │  ├─ Auto-inserted single via at ({:.3}mm, {:.3}mm) spanning z {:.3}mm to {:.3}mm",
                                self.coord_to_mm(&vias[0].position, 'x'),
                                self.coord_to_mm(&vias[0].position, 'y'),
                                transition.from_z_nm as f64 / 1_000_000.0,
                                transition.to_z_nm as f64 / 1_000_000.0,
                            );
                        }

                        for via in &vias {
                            auto_via_metadata
                                .push(self.create_contact_metadata_for_via(via, &transition)
                                    .map_err(|e| e.to_string())?);
                        }

                        inserted_vias.extend(vias);
                    }
                    Err(error) => {
                        println!(
                            "   │  ├─ ⚠️  Could not insert via for transition {} → {}: {}",
                            transition.from_pour, transition.to_pour, error
                        );
                    }
                }
            }
        }

        // Phase 2: Detect transitions from analytic routes (manual routes with Z changes)
        if !space.analytic_routes.is_empty() {
            let route_transitions = self.find_transitions_in_analytic_routes(
                &space.analytic_routes,
                stackup_manager,
                profile,
            )?;

            if !route_transitions.is_empty() {
                println!(
                    "   ├─ Analyzing {} analytic route(s): {} layer transition(s) detected",
                    space.analytic_routes.len(),
                    route_transitions.len()
                );
            }

            for transition in route_transitions {
                let is_power_or_ground = space
                    .net_classifications
                    .get(transition.net_name.as_str())
                    .map(|classification| {
                        matches!(
                            classification,
                            NetClassification::Power | NetClassification::Ground
                        )
                    })
                    .unwrap_or(false);

                let overlap = match self.find_overlap(&transition.from_bbox, &transition.to_bbox) {
                    Ok(o) => o,
                    Err(error) => {
                        println!(
                            "   │  ├─ ⚠️  Could not insert via for route transition {} → {}: {}",
                            transition.from_pour, transition.to_pour, error
                        );
                        continue;
                    }
                };
                let ctx = ViaInsertionContext {
                    transition: &transition,
                    overlap: &overlap,
                    is_power_or_ground,
                    contacts: &space.contacts,
                    auto_via_metadata: &auto_via_metadata,
                    keep_out_zones: &space.keep_out_zones,
                };
                match self.process_transition(&ctx, profile) {
                    Ok(vias) => {
                        if vias.len() > 1 {
                            println!(
                                "   │  ├─ Auto-inserted {} vias in stack for route transition",
                                vias.len()
                            );
                        } else if vias.len() == 1 {
                            println!(
                                "   │  ├─ Auto-inserted via at ({:.3}mm, {:.3}mm) for route z {:.3}mm → {:.3}mm",
                                self.coord_to_mm(&vias[0].position, 'x'),
                                self.coord_to_mm(&vias[0].position, 'y'),
                                transition.from_z_nm as f64 / 1_000_000.0,
                                transition.to_z_nm as f64 / 1_000_000.0,
                            );
                        }

                        for via in &vias {
                            auto_via_metadata
                                .push(self.create_contact_metadata_for_via(via, &transition)
                                    .map_err(|e| e.to_string())?);
                        }

                        inserted_vias.extend(vias);
                    }
                    Err(error) => {
                        println!(
                            "   │  ├─ ⚠️  Could not insert via for route transition {} → {}: {}",
                            transition.from_pour, transition.to_pour, error
                        );
                    }
                }
            }
        }

        println!("   └─ Total vias inserted: {}", inserted_vias.len());

        Ok(inserted_vias)
    }

    fn process_transition(
        &self,
        ctx: &ViaInsertionContext<'_>,
        profile: Option<&hwc_parser::ProfileDefinition>,
    ) -> Result<Vec<ContactPlacement>, String> {
        println!(
            "   │  [PROCESS] Transition L{}→L{} at z {}nm→{}nm, mat '{}'→'{}'",
            ctx.transition.from_layer,
            ctx.transition.to_layer,
            ctx.transition.from_z_nm,
            ctx.transition.to_z_nm,
            ctx.transition.from_material,
            ctx.transition.to_material
        );

        println!(
            "   │  [PROCESS] Overlap center: ({}, {}) nm",
            ctx.overlap.center_x_nm, ctx.overlap.center_y_nm
        );

        self.validate_via_stack(ctx.transition, ctx.overlap, ctx.is_power_or_ground)?;

        let use_array = ctx.is_power_or_ground
            && profile
                .and_then(|profile| profile.via.as_ref())
                .and_then(|via| via.min_spacing.as_ref())
                .is_some();

        let profile_bridge_table = profile.map(crate::bridge_resolver::BridgeTable::from_profile);
        let bridge_stack = crate::bridge_resolver::resolve_bridge(
            &ctx.transition.from_material,
            &ctx.transition.to_material,
            profile_bridge_table.as_ref(),
            None,
            None,
        )
        .map_err(|error| error.to_string())?;

        println!(
            "   │  [PROCESS] Bridge: fill='{}', interface='{}'",
            bridge_stack.fill_material, bridge_stack.interface_material
        );

        let layer_gap = ctx.transition.to_layer - ctx.transition.from_layer;
        println!(
            "   │  [PROCESS] Layer gap: {} (from L{}, to L{})",
            layer_gap, ctx.transition.from_layer, ctx.transition.to_layer
        );

        if layer_gap > 1 {
            if use_array {
                self.insert_via_stack_array(ctx, profile, &bridge_stack)
            } else {
                self.insert_via_stack(ctx, &bridge_stack)
            }
        } else if use_array {
            self.insert_via_array(ctx, profile, &bridge_stack)
        } else {
            let via_type = self
                .via_library
                .find_via_for_layers(
                    ctx.transition.from_layer,
                    ctx.transition.to_layer,
                    ctx.is_power_or_ground,
                )
                .ok_or_else(|| {
                    format!(
                        "No via type found for layers {} to {}",
                        ctx.transition.from_layer, ctx.transition.to_layer
                    )
                })?;

            let diameter_nm = (via_type.diameter_mm * 1_000_000.0) as i64;
            let location = ViaLocation {
                x_nm: ctx.overlap.center_x_nm,
                y_nm: ctx.overlap.center_y_nm,
                from_z_nm: ctx.transition.from_z_nm,
                to_z_nm: ctx.transition.to_z_nm,
                diameter_nm,
            };
            if self.is_colliding(
                &location,
                ctx.contacts,
                ctx.auto_via_metadata,
                ctx.keep_out_zones,
                Some(&ctx.transition.net_name),
            ) {
                return Ok(vec![]);
            }

            Ok(vec![self.create_via_placement(
                ctx.transition,
                ctx.overlap,
                via_type,
                &bridge_stack,
            )])
        }
    }

    fn insert_via_stack(
        &self,
        ctx: &ViaInsertionContext<'_>,
        bridge_stack: &crate::bridge_resolver::BridgeStack,
    ) -> Result<Vec<ContactPlacement>, String> {
        let from = ctx.transition.from_layer;
        let to = ctx.transition.to_layer;

        println!(
            "   │  [STACK] Looking for via L{}→L{} (direct match)...",
            from, to
        );

        if let Some(via_type) =
            self.via_library
                .find_via_for_layers(from, to, ctx.is_power_or_ground)
        {
            println!(
                "   │  [STACK] Found direct via '{}': dia={:.3}mm, ring={:.3}mm",
                via_type.name, via_type.diameter_mm, via_type.min_enclosure_mm
            );

            let diameter_nm = (via_type.diameter_mm * 1_000_000.0) as i64;
            let location = ViaLocation {
                x_nm: ctx.overlap.center_x_nm,
                y_nm: ctx.overlap.center_y_nm,
                from_z_nm: ctx.transition.from_z_nm,
                to_z_nm: ctx.transition.to_z_nm,
                diameter_nm,
            };
            if self.is_colliding(
                &location,
                ctx.contacts,
                ctx.auto_via_metadata,
                ctx.keep_out_zones,
                Some(&ctx.transition.net_name),
            ) {
                println!("   │  [STACK] Collision detected - skipping");
                return Ok(vec![]);
            }

            let placement =
                self.create_via_placement(ctx.transition, ctx.overlap, via_type, bridge_stack);
            println!(
                "   │  [STACK] Created via placement at ({}, {}) z {}nm→{}nm, dia {}nm",
                ctx.overlap.center_x_nm,
                ctx.overlap.center_y_nm,
                ctx.transition.from_z_nm,
                ctx.transition.to_z_nm,
                diameter_nm
            );
            return Ok(vec![placement]);
        }

        println!("   │  [STACK] No direct via found, building layer-by-layer stack...");
        let mut stack = Vec::new();
        for layer in from..to {
            let via_type = self
                .via_library
                .find_via_for_layers(layer, layer + 1, ctx.is_power_or_ground)
                .expect("Via type should have been validated");
            let diameter_nm = (via_type.diameter_mm * 1_000_000.0) as i64;

            let location = ViaLocation {
                x_nm: ctx.overlap.center_x_nm,
                y_nm: ctx.overlap.center_y_nm,
                from_z_nm: ctx.transition.from_z_nm,
                to_z_nm: ctx.transition.to_z_nm,
                diameter_nm,
            };
            if self.is_colliding(
                &location,
                ctx.contacts,
                ctx.auto_via_metadata,
                ctx.keep_out_zones,
                Some(&ctx.transition.net_name),
            ) {
                continue;
            }

            stack.push(self.create_via_placement(
                ctx.transition,
                ctx.overlap,
                via_type,
                bridge_stack,
            ));
        }

        Ok(stack)
    }

    fn insert_via_array(
        &self,
        ctx: &ViaInsertionContext<'_>,
        profile: Option<&hwc_parser::ProfileDefinition>,
        bridge_stack: &crate::bridge_resolver::BridgeStack,
    ) -> Result<Vec<ContactPlacement>, String> {
        let via_type = self
            .via_library
            .find_via_for_layers(
                ctx.transition.from_layer,
                ctx.transition.to_layer,
                ctx.is_power_or_ground,
            )
            .ok_or_else(|| {
                format!(
                    "No via type found for layers {} to {}",
                    ctx.transition.from_layer, ctx.transition.to_layer
                )
            })?;

        let array_config = self.calculate_via_array(ctx.overlap, via_type, profile)?;
        let mut vias = Vec::new();
        let diameter_nm = (via_type.diameter_mm * 1_000_000.0) as i64;

        for row in 0..array_config.rows {
            for col in 0..array_config.cols {
                let x_nm = array_config.start_x_nm + (col as i64 * array_config.pitch_x_nm);
                let y_nm = array_config.start_y_nm + (row as i64 * array_config.pitch_y_nm);

                let location = ViaLocation {
                    x_nm,
                    y_nm,
                    from_z_nm: ctx.transition.from_z_nm,
                    to_z_nm: ctx.transition.to_z_nm,
                    diameter_nm,
                };
                if self.is_colliding(
                    &location,
                    ctx.contacts,
                    ctx.auto_via_metadata,
                    ctx.keep_out_zones,
                    Some(&ctx.transition.net_name),
                ) {
                    continue;
                }

                vias.push(self.create_via_placement_at(
                    ctx.transition,
                    via_type,
                    &ViaPlacementParams {
                        x_nm,
                        y_nm,
                        row,
                        col,
                        bridge_stack,
                    },
                ));
            }
        }

        Ok(vias)
    }

    fn insert_via_stack_array(
        &self,
        ctx: &ViaInsertionContext<'_>,
        profile: Option<&hwc_parser::ProfileDefinition>,
        bridge_stack: &crate::bridge_resolver::BridgeStack,
    ) -> Result<Vec<ContactPlacement>, String> {
        let from = ctx.transition.from_layer;
        let to = ctx.transition.to_layer;
        let direct_via_type =
            self.via_library
                .find_via_for_layers(from, to, ctx.is_power_or_ground);
        let mut all_vias = Vec::new();

        if let Some(via_type) = direct_via_type {
            let array_config = self.calculate_via_array(ctx.overlap, via_type, profile)?;
            let diameter_nm = (via_type.diameter_mm * 1_000_000.0) as i64;

            for row in 0..array_config.rows {
                for col in 0..array_config.cols {
                    let x_nm = array_config.start_x_nm + (col as i64 * array_config.pitch_x_nm);
                    let y_nm = array_config.start_y_nm + (row as i64 * array_config.pitch_y_nm);

                    let location = ViaLocation {
                        x_nm,
                        y_nm,
                        from_z_nm: ctx.transition.from_z_nm,
                        to_z_nm: ctx.transition.to_z_nm,
                        diameter_nm,
                    };
                    if self.is_colliding(
                        &location,
                        ctx.contacts,
                        ctx.auto_via_metadata,
                        ctx.keep_out_zones,
                        Some(&ctx.transition.net_name),
                    ) {
                        continue;
                    }

                    all_vias.push(self.create_via_placement_at(
                        ctx.transition,
                        via_type,
                        &ViaPlacementParams {
                            x_nm,
                            y_nm,
                            row,
                            col,
                            bridge_stack,
                        },
                    ));
                }
            }

            return Ok(all_vias);
        }

        for layer in from..to {
            let via_type = self
                .via_library
                .find_via_for_layers(layer, layer + 1, ctx.is_power_or_ground)
                .expect("Via type should have been validated");
            let array_config = self.calculate_via_array(ctx.overlap, via_type, profile)?;
            let diameter_nm = (via_type.diameter_mm * 1_000_000.0) as i64;

            for row in 0..array_config.rows {
                for col in 0..array_config.cols {
                    let x_nm = array_config.start_x_nm + (col as i64 * array_config.pitch_x_nm);
                    let y_nm = array_config.start_y_nm + (row as i64 * array_config.pitch_y_nm);

                    let location = ViaLocation {
                        x_nm,
                        y_nm,
                        from_z_nm: ctx.transition.from_z_nm,
                        to_z_nm: ctx.transition.to_z_nm,
                        diameter_nm,
                    };
                    if self.is_colliding(
                        &location,
                        ctx.contacts,
                        ctx.auto_via_metadata,
                        ctx.keep_out_zones,
                        Some(&ctx.transition.net_name),
                    ) {
                        continue;
                    }

                    all_vias.push(self.create_via_placement_at(
                        ctx.transition,
                        via_type,
                        &ViaPlacementParams {
                            x_nm,
                            y_nm,
                            row,
                            col,
                            bridge_stack,
                        },
                    ));
                }
            }
        }

        Ok(all_vias)
    }
}
