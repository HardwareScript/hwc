use crate::commands::build_cmd::{BuildConfig, BuildError};
use hwc_compiler::SymbolTable;
use hwc_engine::HardwareSpace;
use hwc_export::netlist::types::PhysicalNetlist;
use hwc_parser::{Program, TopLevelItem};
use miette::Result;
use std::time::Instant;

/// Run alignment validation (Artist vs Professional mode)
/// Returns Some(PhysicalNetlist) in Professional mode, None in Artist mode
pub fn validate_alignment(
    ast: &Program,
    space: &mut HardwareSpace,
    symbol_table: &SymbolTable,
    _config: &BuildConfig,
    start_time: Instant,
) -> Result<Option<PhysicalNetlist>> {
    let space_decl = ast.items.iter().find_map(|item| {
        if let TopLevelItem::Space(s) = item {
            if s.name.as_str() == space.name.as_str() {
                Some(s)
            } else {
                None
            }
        } else {
            None
        }
    }).or_else(|| {
        ast.items.iter().find_map(|item| {
            if let TopLevelItem::Space(s) = item {
                Some(s)
            } else {
                None
            }
        })
    });

    let is_artist_mode = space_decl.map_or(true, |s| s.implements.is_none());

    if is_artist_mode {
        println!("🎨 Artist Mode: No 'implements' clause - Alignment validation skipped");
        println!("   ℹ️  Building geometry without logic verification");
        println!(
            "[{:>8.2}ms] Artist Mode check complete",
            start_time.elapsed().as_secs_f64() * 1000.0
        );
        Ok(None)
    } else {
        println!("🔍 Professional Mode: Comptime extraction enabled");

        let module_decl = space_decl
            .and_then(|s| s.implements.as_ref())
            .and_then(|mod_name| {
                ast.items.iter().find_map(|item| {
                    if let TopLevelItem::Module(m) = item {
                        if m.name.as_str() == mod_name.as_str() {
                            Some(m)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
            });

        let mut device_extractor = hwc_export::DeviceExtractor::new(
            space,
            symbol_table,
            space_decl,
        );

        let extracted_netlist = device_extractor
            .extract_devices_with_module(module_decl)
            .map_err(|errors| {
                let error_messages: Vec<String> = errors.iter().map(|e| format!("{}", e)).collect();
                BuildError::DeviceExtractionFailed {
                    message: format!("Device extraction failed:\n{}", error_messages.join("\n")),
                }
            })?;

        println!(
            "   ✅ Physical netlist extracted: {} devices",
            extracted_netlist.devices.len()
        );

        Ok(Some(extracted_netlist))
    }
}
