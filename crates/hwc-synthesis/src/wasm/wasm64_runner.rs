// crates/hwc-synthesis/src/wasm/wasm64_runner.rs

use crate::mapper::priority_cuts::MappedInstance;
use crate::wasm::c_abi::{HwcSynthesisOutput64, HwcSynthesisTask64};
use compact_str::CompactString;
use std::ffi::{CStr, CString};
use std::path::Path;

/// Universal wasm64 Synthesis Runner interface for Tier 3 synthesis engines (Yosys, ABC).
pub struct Wasm64SynthesisRunner;

impl Wasm64SynthesisRunner {
    /// Check if a given path is a valid .wasm plugin binary.
    pub fn is_wasm_plugin(path: &Path) -> bool {
        path.extension().map_or(false, |ext| ext == "wasm")
    }

    /// Prepares a task payload for wasm64 plugin execution.
    pub fn create_task_payload(
        top_module: &str,
        rtl_json: &str,
        liberty_content: &str,
        target_freq_mhz: f32,
    ) -> (
        CString,
        CString,
        CString,
        HwcSynthesisTask64,
    ) {
        let top_c = CString::new(top_module).unwrap_or_default();
        let rtl_c = CString::new(rtl_json).unwrap_or_default();
        let lib_c = CString::new(liberty_content).unwrap_or_default();

        let task = HwcSynthesisTask64 {
            top_module_name: top_c.as_ptr(),
            rtl_source_json: rtl_c.as_ptr(),
            rtl_source_len: rtl_json.len() as u64,
            liberty_db_ptr: lib_c.as_ptr(),
            liberty_db_len: liberty_content.len() as u64,
            target_freq_mhz,
        };

        (top_c, rtl_c, lib_c, task)
    }

    /// Converts output cells from C-ABI struct to native `MappedInstance` records.
    ///
    /// # Safety
    /// `output.cells_ptr` must be valid for reads of `output.cell_count` elements if `status_code == 0`.
    pub unsafe fn parse_output(
        output: &HwcSynthesisOutput64,
        catalog: &crate::liberty::parser::LibertyCatalog,
    ) -> Result<Vec<MappedInstance>, String> {
        if output.status_code != 0 {
            let msg = if !output.error_msg.is_null() {
                CStr::from_ptr(output.error_msg)
                    .to_string_lossy()
                    .into_owned()
            } else {
                format!("Wasm synthesis failed with code {}", output.status_code)
            };
            return Err(msg);
        }

        if output.cells_ptr.is_null() || output.cell_count == 0 {
            return Ok(Vec::new());
        }

        let mut instances = Vec::with_capacity(output.cell_count as usize);
        let cells_slice = std::slice::from_raw_parts(output.cells_ptr, output.cell_count as usize);

        for (idx, cell_ref) in cells_slice.iter().enumerate() {
            let name_str = if !cell_ref.cell_name.is_null() {
                CStr::from_ptr(cell_ref.cell_name).to_string_lossy()
            } else {
                "unknown".into()
            };

            let std_cell = catalog
                .get_by_name(&name_str)
                .cloned()
                .unwrap_or_else(|| {
                    crate::liberty::cell::StandardCell::new(
                        &name_str,
                        "WASM_CELL",
                        920_000,
                        2_720_000,
                        30.0,
                        &["A", "B"],
                        &["Y"],
                        0x7777_7777_7777_7777,
                        vec![vec![0, 1]],
                        false,
                    )
                });

            let mut input_nodes = Vec::new();
            if !cell_ref.net_ids.is_null() && cell_ref.pin_count > 0 {
                let nets = std::slice::from_raw_parts(cell_ref.net_ids, cell_ref.pin_count as usize);
                input_nodes.extend_from_slice(nets);
            }

            instances.push(MappedInstance {
                node_id: cell_ref.instance_id as u32,
                instance_name: CompactString::new(format!("wasm_gate_{}_{}", name_str, idx)),
                cell: std_cell,
                input_nodes,
                output_node: cell_ref.instance_id as u32,
            });
        }

        Ok(instances)
    }
}
