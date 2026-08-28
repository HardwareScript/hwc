//! Device instance population for the v0.3.0 pipeline.
//!
//! Lowers every emitted semiconductor device contract into a
//! [`hwc_engine::space::DeviceInstance`] with resolved terminal nets.

use compact_str::CompactString;
use hwc_engine::HardwareSpace;
use hwc_types::NetId;
use rustc_hash::FxHashMap;

use crate::eval::MemoryEmitter;

/// Populate device instances from emitted primitives.
pub fn populate_devices(
    hw_space: &mut HardwareSpace,
    mem: &MemoryEmitter,
    net_id_to_name: &FxHashMap<NetId, CompactString>,
) {
    // 5. Populate devices
    for dev in &mem.devices {
        let mut terms = Vec::new();
        let mut term_nets = FxHashMap::default();
        for (term_name, net_id) in &dev.terminals {
            terms.push(term_name.clone());
            let resolved_net = net_id_to_name
                .get(net_id)
                .cloned()
                .unwrap_or_else(|| CompactString::new(format!("NET_{}", net_id.0)));
            term_nets.insert(term_name.clone(), resolved_net);
        }

        let mut params_map = FxHashMap::default();
        for (p_name, m_val) in &dev.params {
            params_map.insert(p_name.clone(), (m_val.raw as f64) * 1e-12);
        }

        hw_space.device_instances.push(hwc_engine::space::DeviceInstance {
            name: dev.name.clone(),
            def_path: None,
            device_type: dev.device_type.clone(),
            terminals: terms,
            terminal_nets: term_nets,
            parameters: params_map,
        });
    }
}
