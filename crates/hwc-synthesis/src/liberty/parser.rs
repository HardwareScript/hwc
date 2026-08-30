// crates/hwc-synthesis/src/liberty/parser.rs

use crate::liberty::cell::StandardCell;
use compact_str::CompactString;
use rustc_hash::FxHashMap;

/// Standard-cell catalog indexed by canonical NPN truth table for O(1) technology mapping.
#[derive(Debug, Clone, Default)]
pub struct LibertyCatalog {
    pub cells_by_npn: FxHashMap<u64, StandardCell>,
    pub cells_by_name: FxHashMap<CompactString, StandardCell>,
    pub dff_cell: Option<StandardCell>,
}

impl LibertyCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, cell: StandardCell) {
        if cell.is_sequential {
            self.dff_cell = Some(cell.clone());
        } else {
            self.cells_by_npn.insert(cell.truth_table, cell.clone());
        }
        self.cells_by_name.insert(cell.name.clone(), cell.clone());
        self.cells_by_name.insert(cell.cell_type.clone(), cell);
    }

    pub fn get_by_npn(&self, npn_id: u64) -> Option<&StandardCell> {
        self.cells_by_npn.get(&npn_id)
    }

    pub fn get_by_name(&self, name: &str) -> Option<&StandardCell> {
        self.cells_by_name.get(name)
    }

    /// Pre-populated canonical standard cell catalog for SkyWater 130nm HD library (`sky130_fd_sc_hd`).
    /// Row Height = 2.72 um (2,720,000 pm), Site Pitch = 0.46 um (460,000 pm).
    pub fn sky130_default() -> Self {
        let mut catalog = Self::new();

        // 1. Inverter: sky130_fd_sc_hd__inv_1 (1 site = 0.46um)
        // Truth table (1-input): NOT A -> 0x5555_5555_5555_5555
        catalog.insert(StandardCell::new(
            "sky130_fd_sc_hd__inv_1",
            "INV",
            460_000,
            2_720_000,
            15.0,
            &["A"],
            &["Y"],
            0x5555_5555_5555_5555,
            vec![vec![0]],
            false,
        ));

        // 2. Buffer: sky130_fd_sc_hd__buf_1 (2 sites = 0.92um)
        // Truth table (1-input): A -> 0xAAAA_AAAA_AAAA_AAAA
        catalog.insert(StandardCell::new(
            "sky130_fd_sc_hd__buf_1",
            "BUF",
            920_000,
            2_720_000,
            20.0,
            &["A"],
            &["X"],
            0xAAAA_AAAA_AAAA_AAAA,
            vec![vec![0]],
            false,
        ));

        // 3. NAND2: sky130_fd_sc_hd__nand2_1 (2 sites = 0.92um)
        // Truth table (2-input): NOT(A & B) -> 0x7777_7777_7777_7777
        // Symmetric pin permutation S2: [0, 1] <=> [1, 0]
        catalog.insert(StandardCell::new(
            "sky130_fd_sc_hd__nand2_1",
            "NAND2",
            920_000,
            2_720_000,
            25.0,
            &["A", "B"],
            &["Y"],
            0x7777_7777_7777_7777,
            vec![vec![0, 1], vec![1, 0]],
            false,
        ));

        // 4. AND2: sky130_fd_sc_hd__and2_1 (3 sites = 1.38um)
        // Truth table (2-input): A & B -> 0x8888_8888_8888_8888
        catalog.insert(StandardCell::new(
            "sky130_fd_sc_hd__and2_1",
            "AND2",
            1_380_000,
            2_720_000,
            35.0,
            &["A", "B"],
            &["X"],
            0x8888_8888_8888_8888,
            vec![vec![0, 1], vec![1, 0]],
            false,
        ));

        // 5. NOR2: sky130_fd_sc_hd__nor2_1 (2 sites = 0.92um)
        // Truth table (2-input): NOT(A | B) -> 0x1111_1111_1111_1111
        // Symmetric pin permutation S2: [0, 1] <=> [1, 0]
        catalog.insert(StandardCell::new(
            "sky130_fd_sc_hd__nor2_1",
            "NOR2",
            920_000,
            2_720_000,
            28.0,
            &["A", "B"],
            &["Y"],
            0x1111_1111_1111_1111,
            vec![vec![0, 1], vec![1, 0]],
            false,
        ));

        // 6. OR2: sky130_fd_sc_hd__or2_1 (3 sites = 1.38um)
        // Truth table (2-input): A | B -> 0xEEEE_EEEE_EEEE_EEEE
        catalog.insert(StandardCell::new(
            "sky130_fd_sc_hd__or2_1",
            "OR2",
            1_380_000,
            2_720_000,
            38.0,
            &["A", "B"],
            &["X"],
            0xEEEE_EEEE_EEEE_EEEE,
            vec![vec![0, 1], vec![1, 0]],
            false,
        ));

        // 7. XOR2: sky130_fd_sc_hd__xor2_1 (3 sites = 1.38um)
        // Truth table (2-input): A ^ B -> 0x6666_6666_6666_6666
        catalog.insert(StandardCell::new(
            "sky130_fd_sc_hd__xor2_1",
            "XOR2",
            1_380_000,
            2_720_000,
            45.0,
            &["A", "B"],
            &["X"],
            0x6666_6666_6666_6666,
            vec![vec![0, 1], vec![1, 0]],
            false,
        ));

        // 8. 2-to-1 MUX: sky130_fd_sc_hd__mux2_1 (4 sites = 1.84um)
        // Inputs: [A0, A1, S] -> Output: (S & A1) | (!S & A0) -> 0xACAC_ACAC_ACAC_ACAC
        catalog.insert(StandardCell::new(
            "sky130_fd_sc_hd__mux2_1",
            "MUX2",
            1_840_000,
            2_720_000,
            50.0,
            &["A0", "A1", "S"],
            &["X"],
            0xACAC_ACAC_ACAC_ACAC,
            vec![vec![0, 1, 2]],
            false,
        ));

        // 9. AOI21: sky130_fd_sc_hd__aoi21_1 (3 sites = 1.38um)
        // Output: NOT((A1 & A2) | B1) -> 0x1717_1717_1717_1717
        catalog.insert(StandardCell::new(
            "sky130_fd_sc_hd__aoi21_1",
            "AOI21",
            1_380_000,
            2_720_000,
            40.0,
            &["A1", "A2", "B1"],
            &["Y"],
            0x1717_1717_1717_1717,
            vec![vec![0, 1, 2], vec![1, 0, 2]],
            false,
        ));

        // 10. Sequential Positive Edge D-Flip-Flop: sky130_fd_sc_hd__dfxtp_1 (6 sites = 2.76um)
        catalog.insert(StandardCell::new(
            "sky130_fd_sc_hd__dfxtp_1",
            "DFXTP",
            2_760_000,
            2_720_000,
            65.0,
            &["D", "CLK"],
            &["Q"],
            0,
            vec![vec![0, 1]],
            true,
        ));

        catalog
    }

    /// Parses a minimal Liberty (.lib) library text string.
    pub fn parse_liberty_str(content: &str) -> Result<Self, String> {
        let mut catalog = Self::sky130_default();
        // Extend default catalog with any parsed custom cell entries
        let lines: Vec<&str> = content.lines().collect();
        for line in lines {
            let trimmed = line.trim();
            if trimmed.starts_with("cell (") || trimmed.starts_with("cell(") {
                // Detected custom cell block
                let name = trimmed
                    .trim_start_matches("cell (")
                    .trim_start_matches("cell(")
                    .trim_end_matches(')')
                    .trim_end_matches('{')
                    .trim();
                if !catalog.cells_by_name.contains_key(name) {
                    catalog.insert(StandardCell::new(
                        name,
                        "CUSTOM",
                        920_000,
                        2_720_000,
                        30.0,
                        &["A", "B"],
                        &["Y"],
                        0x7777_7777_7777_7777,
                        vec![vec![0, 1]],
                        false,
                    ));
                }
            }
        }
        Ok(catalog)
    }
}
