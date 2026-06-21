//! Material definitions — gridless, physical substance registry.
//!
//! # Hermetic Architecture
//! No heuristics, no guessing, no lazy defaults. Every material must be
//! explicitly declared in a `.hw` file and resolved through the parser's
//! AST → Symbol Table → MaterialRegistry pipeline.
//!
//! If a material is referenced but not declared, lookups return `None`,
//! forcing a compiler halt with a clear diagnostic.

use compact_str::CompactString;
use rustc_hash::FxHashMap;

/// Material ID type — u8 for compact storage. Supports up to 256 materials.
pub type MaterialId = u8;

/// Reserved material ID for Air.
pub const AIR_MATERIAL_ID: MaterialId = 0;

/// Material conductivity classification.
///
/// Determines how the router traverses materials:
/// - **Conductor**: Metal, doped poly — router must avoid if different net
/// - **Semiconductor**: Silicon — router can traverse (substrate material)
/// - **Insulator**: SiO2, Air, FR4 — router can traverse freely
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MaterialConductivity {
    /// Conductive materials (metals, doped polysilicon)
    Conductor,
    /// Semiconductor materials (silicon substrates)
    Semiconductor,
    /// Insulating materials (oxides, air, dielectrics)
    Insulator,
}

/// Manufacturing process behavior for Z-axis placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ManufacturingProcess {
    /// Drilled and plated through the substrate (PCB style)
    DrilledPlated,
    /// Deposited/Plotted into the grid (CMOS/3D-Print style)
    Deposited,
    /// Etched away from existing material (MEMS style)
    Etched,
}

/// Strict material registry — maps material names to IDs and conductivity classes.
///
/// # Design Philosophy
/// No heuristics. No guessing. If a material is used but not declared,
/// `get_id()` returns `None`, forcing a compiler halt.
///
/// # Architecture
/// - Script Layer: `material N_Doped: semiconductor`
/// - Registry Layer: First encounter assigns `N_Doped → ID 1` + conductivity class
/// - Export Layer: `ID 1 → "N_Doped" → Layer 17`
#[derive(Debug, Clone)]
pub struct MaterialRegistry {
    /// Fast lookup: Name → ID
    name_to_id: FxHashMap<CompactString, MaterialId>,
    /// Fast lookup for export: ID → Name (Vec for O(1) indexing)
    id_to_name: Vec<CompactString>,
    /// Fast lookup for routing: ID → Conductivity (Vec for O(1) indexing)
    id_to_conductivity: Vec<MaterialConductivity>,
    /// Fast lookup for manufacturing: ID → Process (Vec for O(1) indexing)
    id_to_process: Vec<ManufacturingProcess>,
}

impl MaterialRegistry {
    /// Create a new material registry with Air pre-registered as ID 0.
    ///
    /// Only Air is registered by default. All other materials must be
    /// explicitly registered via `register_with_properties()` from
    /// the symbol table.
    pub fn new() -> Self {
        let mut registry = Self {
            name_to_id: FxHashMap::default(),
            id_to_name: vec!["Air".into()],
            id_to_conductivity: vec![MaterialConductivity::Insulator],
            id_to_process: vec![ManufacturingProcess::Deposited],
        };
        registry.name_to_id.insert("Air".into(), AIR_MATERIAL_ID);
        registry
    }

    /// Register a material with explicit conductivity and manufacturing process.
    ///
    /// Returns the assigned `MaterialId`. If the material is already registered,
    /// updates its properties and returns the existing ID.
    pub fn register_with_properties(
        &mut self,
        name: &str,
        conductivity: MaterialConductivity,
        process: ManufacturingProcess,
    ) -> MaterialId {
        if let Some(&id) = self.name_to_id.get(name) {
            if self.id_to_conductivity[id as usize] != conductivity {
                self.id_to_conductivity[id as usize] = conductivity;
            }
            if self.id_to_process[id as usize] != process {
                self.id_to_process[id as usize] = process;
            }
            return id;
        }
        if self.id_to_name.len() >= 255 {
            panic!("Material registry full! Maximum 255 materials supported.");
        }
        let id = self.id_to_name.len() as u8;
        self.id_to_name.push(name.into());
        self.id_to_conductivity.push(conductivity);
        self.id_to_process.push(process);
        self.name_to_id.insert(name.into(), id);
        id
    }

    /// Register a material with explicit conductivity classification.
    pub fn register_with_conductivity(
        &mut self,
        name: &str,
        conductivity: MaterialConductivity,
    ) -> MaterialId {
        self.register_with_properties(name, conductivity, ManufacturingProcess::Deposited)
    }

    /// Get material ID by name. Returns `None` if not registered.
    ///
    /// # Strict Lookup
    /// No heuristics. No guessing. If not registered, returns `None`.
    #[inline]
    pub fn get_id(&self, name: &str) -> Option<MaterialId> {
        self.name_to_id.get(name).copied()
    }

    /// Get material name by ID (O(1) array access).
    #[inline]
    pub fn get_name(&self, id: MaterialId) -> Option<&str> {
        self.id_to_name.get(id as usize).map(|s| s.as_str())
    }

    /// Get all registered materials.
    pub fn all_materials(&self) -> Vec<(MaterialId, &str)> {
        self.id_to_name
            .iter()
            .enumerate()
            .map(|(id, name)| (id as MaterialId, name.as_str()))
            .collect()
    }

    /// Get conductivity classification for a material ID.
    #[inline]
    pub fn get_conductivity(&self, id: MaterialId) -> Option<MaterialConductivity> {
        self.id_to_conductivity.get(id as usize).copied()
    }

    /// Get conductivity classification strictly by material name.
    ///
    /// # Returns
    /// - `Some(MaterialConductivity)` if the material was explicitly declared/imported
    /// - `None` if the material is unregistered (forces compiler halt)
    pub fn get_conductivity_by_name(&self, name: &str) -> Option<MaterialConductivity> {
        let name_clean = name.trim();

        // 1. Direct lookup
        if let Some(id) = self.name_to_id.get(name_clean) {
            return self.get_conductivity(*id);
        }

        // 2. Case-insensitive lookup
        let name_lower = name_clean.to_lowercase();
        if let Some(id) = self.name_to_id.get(name_lower.as_str()) {
            return self.get_conductivity(*id);
        }

        // No guessing. No heuristics. If not registered, return None.
        None
    }

    /// Check if a material is a conductor.
    #[inline]
    pub fn is_conductor(&self, id: MaterialId) -> bool {
        self.get_conductivity(id) == Some(MaterialConductivity::Conductor)
    }

    /// Check the manufacturing process for a material ID.
    #[inline]
    pub fn get_process(&self, id: MaterialId) -> Option<ManufacturingProcess> {
        self.id_to_process.get(id as usize).copied()
    }

    /// Set the manufacturing process for a material ID.
    pub fn set_process(&mut self, id: MaterialId, process: ManufacturingProcess) {
        if let Some(p) = self.id_to_process.get_mut(id as usize) {
            *p = process;
        }
    }

    /// Check if a material is a semiconductor.
    #[inline]
    pub fn is_semiconductor(&self, id: MaterialId) -> bool {
        matches!(self.get_conductivity(id), Some(MaterialConductivity::Semiconductor))
    }

    /// Check if a material is an insulator.
    #[inline]
    pub fn is_insulator(&self, id: MaterialId) -> bool {
        matches!(self.get_conductivity(id), Some(MaterialConductivity::Insulator))
    }
}

impl Default for MaterialRegistry {
    fn default() -> Self {
        Self::new()
    }
}
