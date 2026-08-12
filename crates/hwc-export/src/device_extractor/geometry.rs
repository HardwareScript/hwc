use compact_str::CompactString;
use hwc_engine::geometry::BoundingBox;
use hwc_engine::space::PourMetadata;
use rustc_hash::FxHashMap;

use super::error::DeviceExtractionError;
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

    /// Calculate parasitic parameters from source/drain pours
    pub(super) fn calculate_parasitics_from_pours(
        &self,
        source_pour: &PourMetadata,
        drain_pour: &PourMetadata,
    ) -> Option<(f64, f64, f64, f64)> {
        let as_m2 = (source_pour.area_nm2 as f64) / 1e18;
        let ad_m2 = (drain_pour.area_nm2 as f64) / 1e18;

        let ps_m = source_pour
            .bbox
            .as_ref()
            .map(|bbox| self.calculate_perimeter(bbox))
            .unwrap_or(0.0);

        let pd_m = drain_pour
            .bbox
            .as_ref()
            .map(|bbox| self.calculate_perimeter(bbox))
            .unwrap_or(0.0);

        Some((as_m2, ad_m2, ps_m, pd_m))
    }

    /// Calculate perimeter of a bounding box
    pub(super) fn calculate_perimeter(&self, bbox: &BoundingBox) -> f64 {
        let width_nm = (bbox.max.x - bbox.min.x).abs() as f64;
        let height_nm = (bbox.max.y - bbox.min.y).abs() as f64;
        let perimeter_nm = 2.0 * (width_nm + height_nm);
        perimeter_nm / 1e9
    }

    /// Calculate channel dimensions from gate geometry
    pub(super) fn calculate_channel_dimensions(
        &self,
        gate_pour: &PourMetadata,
    ) -> Result<(f64, f64), DeviceExtractionError> {
        let area_nm2 = gate_pour.area_nm2 as f64;
        let side_nm = area_nm2.sqrt();
        let side_um = side_nm / 1000.0;
        Ok((side_um, side_um))
    }
}
