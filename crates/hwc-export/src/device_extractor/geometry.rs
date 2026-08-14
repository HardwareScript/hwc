use compact_str::CompactString;
use hwc_engine::space::PourMetadata;
use rustc_hash::FxHashMap;

use super::DeviceExtractor;

impl<'a> DeviceExtractor<'a> {
    /// Group all pours by their device binding
    ///
    /// Creates a map: DeviceName -> (Terminal -> Vec<PourMetadata>)
    ///
    /// A single terminal may have MULTIPLE pours bound to it (e.g. a resistor
    /// terminal bound to both the Polysilicon channel body and a silicide contact
    /// pad). All bound pours are retained so downstream extractors can decide
    /// explicitly which pour serves which physical role.
    /// 
    /// **v0.2.2: Pours are sorted by binding priority** (Channel=0 before Contact=100)
    /// This ensures parameter extractors always see the resistive body first.
    pub(super) fn group_pours_by_device_binding(
        &self,
    ) -> FxHashMap<CompactString, FxHashMap<CompactString, Vec<PourMetadata>>> {
        let mut bindings: FxHashMap<CompactString, FxHashMap<CompactString, Vec<PourMetadata>>> =
            FxHashMap::default();

        println!(
            "   ├─ Scanning {} pours for device bindings...",
            self.space.pours.len()
        );

        for pour in &self.space.pours {
            println!(
                "      ├─ Pour '{}': device_binding = {:?}",
                pour.name, pour.device_binding
            );

            if let Some(ref device_binding) = pour.device_binding {
                let device_name = &device_binding.device_name;
                
                // v0.2.2: Handle multi-terminal bindings
                for terminal in &device_binding.terminals {
                    bindings
                        .entry(device_name.clone())
                        .or_default()
                        .entry(terminal.clone())
                        .or_insert_with(Vec::new)
                        .push(pour.clone());

                    println!(
                        "      ├─ Bound: {}.{} → {} ({}, priority={:?})",
                        device_name, terminal, pour.name, pour.material_name, device_binding.priority
                    );
                }
            }
        }

        // v0.2.2: Sort pours by binding priority (Channel before Contact)
        // This ensures parameter extractors always see the resistive body first
        for device_pours in bindings.values_mut() {
            for terminal_pours in device_pours.values_mut() {
                terminal_pours.sort_by_key(|pour| {
                    pour.device_binding
                        .as_ref()
                        .map(|b| b.priority)
                        .unwrap_or_default()
                });
            }
        }

        bindings
    }
}

