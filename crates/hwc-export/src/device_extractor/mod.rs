//! Device Extractor for HardwareScript v0.3.0

pub mod error;
pub mod spice;

pub use error::DeviceExtractionError;
pub use spice::format_spice;

use crate::netlist::types::PhysicalNetlist;
use hwc_engine::HardwareSpace;

/// Device extractor bridging physical space records to PhysicalNetlist
pub struct DeviceExtractor<'a> {
    pub space: &'a HardwareSpace,
    pub symbol_table: &'a hwc_compiler::SymbolTable,
    pub space_def: Option<&'a hwc_parser::SpaceDecl>,
}

impl<'a> DeviceExtractor<'a> {
    pub fn new(
        space: &'a HardwareSpace,
        symbol_table: &'a hwc_compiler::SymbolTable,
        space_def: Option<&'a hwc_parser::SpaceDecl>,
    ) -> Self {
        Self {
            space,
            symbol_table,
            space_def,
        }
    }

    pub fn extract_devices_with_module(
        &mut self,
        _module: Option<&hwc_parser::ModuleDecl>,
    ) -> Result<PhysicalNetlist, Vec<DeviceExtractionError>> {
        let mut netlist = PhysicalNetlist::new();

        for dev in &self.space.device_instances {
            let mut terms = rustc_hash::FxHashMap::default();
            for (k, v) in &dev.terminal_nets {
                terms.insert(k.clone(), v.clone());
            }
            let mut params = rustc_hash::FxHashMap::default();
            for (k, v) in &dev.parameters {
                params.insert(
                    k.clone(),
                    hwc_compiler::eval::MeasurementValue {
                        raw: (*v * 1e12) as i128,
                        dimension: hwc_compiler::eval::UnitDimension::Length,
                    },
                );
            }
            let type_id = netlist.device_registry.get_or_register(dev.device_type.as_str());
            netlist.devices.push(crate::netlist::types::PhysicalDevice {
                name: dev.name.clone(),
                device_type: dev.device_type.clone(),
                device_type_id: type_id,
                terminals: terms,
                params,
            });
        }

        Ok(netlist)
    }
}
