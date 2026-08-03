use crate::ir::errors::IrError;
use crate::SymbolTable;

/// Finalize the hardware space: bridge validation, net sync, via resolution, dummy fill, error gate.
pub fn finalize(
    space: &mut hwc_engine::HardwareSpace,
    profile: Option<hwc_parser::ProfileDefinition>,
    stackup_manager: &crate::ir::stackup_manager::StackupManager,
    symbol_table: &SymbolTable,
    collector: &hwc_diagnostics::DiagnosticCollector,
    space_def: &hwc_parser::SpaceDefinition,
    eval_context: &hwc_parser::EvaluationContext,
) -> Result<(), IrError> {
    // Check for Artist Mode vs Professional Mode
    let is_artist_mode = space_def.implements_module.is_none();

    // P45 Forbidden Junction Detection (Assembly Level)
    // Only run in Professional Mode - Artist Mode skips bridge validation
    if !is_artist_mode {
        crate::ir::bridge_validator::validate_bridges(space, profile.as_ref(), Some(symbol_table))?;
    }

    // Synchronize net names from pins to bound pours
    space.synchronize_nets();

    // Native Via Resolution (v0.1.8)
    // Only run in Professional Mode - Artist Mode skips via resolution
    if !is_artist_mode {
        let via_resolver = crate::via_resolver::ViaResolver::from_profile(
            profile.as_ref(),
            stackup_manager,
            symbol_table,
            eval_context,
        )?;
        via_resolver.resolve_connectivity(space, stackup_manager)?;
    }

    // v0.1.9 DFM: Profile-controlled dummy metal fill (thieving)
    apply_dummy_fill(space, profile.as_ref(), symbol_table, space_def)?;

    // Sprint 9 (Task 9.1): PLACEMENT GATE
    if collector.has_errors() {
        let n = collector.error_count();
        return Err(IrError::CompilationAborted { error_count: n });
    }

    Ok(())
}

/// Apply dummy fill if the profile explicitly enables it.
fn apply_dummy_fill(
    space: &mut hwc_engine::HardwareSpace,
    _profile: Option<&hwc_parser::ProfileDefinition>,
    symbol_table: &SymbolTable,
    space_def: &hwc_parser::SpaceDefinition,
) -> Result<(), IrError> {
    let profile_def = if let Some(ref profile_name) = space_def.profile {
        symbol_table.get_profile(profile_name.as_str()).ok()
    } else {
        None
    };

    if let Some(profile) = profile_def {
        if let Some(ref manufacturing) = profile.manufacturing {
            if manufacturing.dummy_fill == Some(true) {
                let target_density_pct = manufacturing
                    .dummy_fill_density
                    .map(|d| (d * 100.0) as u8)
                    .expect("dummy_fill enabled but dummy_fill_density not declared in profile");

                let dummy_size_nm = manufacturing
                    .dummy_fill_size
                    .as_ref()
                    .map(|m| {
                        symbol_table
                            .measurement_to_nm(m)
                            .expect("Failed to convert dummy_fill_size to nanometers")
                    })
                    .expect("dummy_fill enabled but dummy_fill_size not declared in profile");

                let dummy_spacing_nm = manufacturing
                    .dummy_fill_spacing
                    .as_ref()
                    .map(|m| {
                        symbol_table
                            .measurement_to_nm(m)
                            .expect("Failed to convert dummy_fill_spacing to nanometers")
                    })
                    .expect("dummy_fill enabled but dummy_fill_spacing not declared in profile");

                let dummy_fill_config = hwc_engine::DummyFillConfig {
                    enabled: true,
                    target_density_pct,
                    dummy_size_nm,
                    dummy_spacing_nm,
                    ..hwc_engine::DummyFillConfig::default()
                };

                let mut dummy_fill_engine = hwc_engine::DummyFillEngine::new();
                let fill_stats = dummy_fill_engine.run(&mut space.entity_graph, &dummy_fill_config);
                if fill_stats.zones_filled > 0 {
                    eprintln!(
                        "[DFM] Dummy fill: {} zones analyzed, {} zones filled, {} dummies placed (avg density before: {:.1}%)",
                        fill_stats.zones_analyzed,
                        fill_stats.zones_filled,
                        fill_stats.total_dummies_placed,
                        fill_stats.average_density_before,
                    );
                }
            }
        }
    }

    Ok(())
}
