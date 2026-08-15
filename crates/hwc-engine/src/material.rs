//! Material definitions — gridless, physical substance registry.
//!
//! # Hermetic Architecture
//! No heuristics, no guessing, no lazy defaults. Every material must be
//! explicitly declared in a `.hw` file and resolved through the parser's
//! AST → Symbol Table → MaterialRegistry pipeline.
//!
//! If a material is referenced but not declared, lookups return `None`,
//! forcing a compiler halt with a clear diagnostic.
//!
//! # Architecture: MaterialCategory Direct Storage
//! The registry stores the full MaterialCategory from the AST. This preserves
//! semantic information (Mask, OhmicContact, BarrierLayer) and enables proper
//! category-specific behavior without lossy translation.

use compact_str::CompactString;
use rustc_hash::FxHashMap;

// Re-export MaterialCategory from parser for use in engine
pub use hwc_parser::MaterialCategory;

/// Material ID type — u8 for compact storage. Supports up to 256 materials.
pub type MaterialId = u8;

/// Reserved material ID for Air.
pub const AIR_MATERIAL_ID: MaterialId = 0;

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

/// Physical properties for thermal/electrical calculations.
///
/// v0.2.1: Dynamic property storage - supports any property declared in .hw files
/// without hardcoding specific property names in the compiler.
///
/// Properties are stored as declared:
/// - resistivity: 4e-4ohm_m → stored as ("resistivity", 0.0004)
/// - thermal_conductivity: 30.0W_mK → stored as ("thermal_conductivity", 30.0)
/// - relative_permittivity: 3.9 → stored as ("relative_permittivity", 3.9)
#[derive(Debug, Clone)]
pub struct MaterialPhysicalProps {
    /// Dynamic property storage: property_name → value
    pub properties: FxHashMap<CompactString, f64>,
}

impl MaterialPhysicalProps {
    /// Create empty properties
    pub fn new() -> Self {
        Self {
            properties: FxHashMap::default(),
        }
    }

    /// Get a property value by name
    ///
    /// Returns None if property not declared in material definition.
    pub fn get(&self, name: &str) -> Option<f64> {
        self.properties.get(name).copied()
    }

    /// Set a property value
    pub fn set(&mut self, name: impl Into<CompactString>, value: f64) {
        self.properties.insert(name.into(), value);
    }

    /// Check if a property exists
    pub fn has(&self, name: &str) -> bool {
        self.properties.contains_key(name)
    }
}

impl Default for MaterialPhysicalProps {
    fn default() -> Self {
        Self::new()
    }
}

/// Strict material registry — maps material names to IDs and material categories.
///
/// # Design Philosophy
/// No heuristics. No guessing. If a material is used but not declared,
/// `get_id()` returns `None`, forcing a compiler halt.
///
/// # Architecture (v0.2.1)
/// - Script Layer: `material RPM: mask`
/// - Registry Layer: First encounter assigns `RPM → ID 5` + MaterialCategory::Mask
/// - Export Layer: `ID 5 → "RPM" → Layer 86` (GDSII)
///
/// The registry stores the full MaterialCategory from the AST, preserving
/// semantic distinctions (Mask vs Insulator, OhmicContact vs Conductor, etc.)
#[derive(Debug, Clone)]
pub struct MaterialRegistry {
    /// Fast lookup: Name → ID
    name_to_id: FxHashMap<CompactString, MaterialId>,
    /// Fast lookup for export: ID → Name (Vec for O(1) indexing)
    id_to_name: Vec<CompactString>,
    /// Fast lookup for category: ID → MaterialCategory (Vec for O(1) indexing)
    /// v0.2.1: Stores full category instead of simplified conductivity
    id_to_category: Vec<MaterialCategory>,
    /// Fast lookup for manufacturing: ID → Process (Vec for O(1) indexing)
    id_to_process: Vec<ManufacturingProcess>,
    /// Fast lookup for physics: ID → Physical properties (resistivity, thermal conductivity)
    id_to_physical: FxHashMap<MaterialId, MaterialPhysicalProps>,
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
            id_to_category: vec![MaterialCategory::Insulator],
            id_to_process: vec![ManufacturingProcess::Deposited],
            id_to_physical: FxHashMap::default(),
        };
        registry.name_to_id.insert("Air".into(), AIR_MATERIAL_ID);
        registry
    }

    /// Register a material with explicit category and manufacturing process.
    ///
    /// v0.2.1: Stores full MaterialCategory instead of simplified conductivity.
    ///
    /// Returns the assigned `MaterialId`. If the material is already registered,
    /// updates its properties and returns the existing ID.
    pub fn register_with_properties(
        &mut self,
        name: &str,
        category: MaterialCategory,
        process: ManufacturingProcess,
    ) -> MaterialId {
        if let Some(&id) = self.name_to_id.get(name) {
            if self.id_to_category[id as usize] != category {
                self.id_to_category[id as usize] = category;
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
        self.id_to_category.push(category);
        self.id_to_process.push(process);
        self.name_to_id.insert(name.into(), id);
        id
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

    /// Get the full material category for a material ID.
    ///
    /// Returns `None` if the ID is out of range (unregistered).
    #[inline]
    pub fn get_category(&self, id: MaterialId) -> Option<MaterialCategory> {
        self.id_to_category.get(id as usize).cloned()
    }

    /// Get the full material category strictly by material name.
    ///
    /// # Returns
    /// - `Some(MaterialCategory)` if the material was explicitly declared/imported
    /// - `None` if the material is unregistered (forces compiler halt)
    pub fn get_category_by_name(&self, name: &str) -> Option<MaterialCategory> {
        let name_clean = name.trim();

        // 1. Direct lookup
        if let Some(id) = self.name_to_id.get(name_clean) {
            return self.get_category(*id);
        }

        // 2. Case-insensitive lookup
        let name_lower = name_clean.to_lowercase();
        if let Some(id) = self.name_to_id.get(name_lower.as_str()) {
            return self.get_category(*id);
        }

        // No guessing. No heuristics. If not registered, return None.
        None
    }

    /// Check if a material is electrically conductive (semiconductor or conductor
    /// or any bridge category). Mirrors `MaterialCategory::is_conductive()`.
    #[inline]
    pub fn is_conductive(&self, id: MaterialId) -> bool {
        self.get_category(id)
            .map(|c| c.is_conductive())
            .unwrap_or(false)
    }

    /// Check if a material is a conductor (fundamental conductor category only).
    #[inline]
    pub fn is_conductor(&self, id: MaterialId) -> bool {
        self.get_category(id) == Some(MaterialCategory::Conductor)
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

    /// Check if a material is a semiconductor (fundamental semiconductor category only).
    #[inline]
    pub fn is_semiconductor(&self, id: MaterialId) -> bool {
        self.get_category(id) == Some(MaterialCategory::Semiconductor)
    }

    /// Check if a material is an insulator (fundamental insulator category only).
    ///
    /// Explicitly checks for `MaterialCategory::Insulator` and EXCLUDES masks,
    /// barrier layers, and other non-fundamental categories. This prevents the
    /// parasitic extractor from treating fabrication masks as dielectrics.
    #[inline]
    pub fn is_insulator(&self, id: MaterialId) -> bool {
        self.get_category(id) == Some(MaterialCategory::Insulator)
    }

    /// Store physical properties for a material (dynamic key-value pairs)
    ///
    /// Properties are stored exactly as declared in .hw files.
    pub fn set_physical_props(&mut self, id: MaterialId, props: MaterialPhysicalProps) {
        self.id_to_physical.insert(id, props);
    }

    /// Get physical properties for a material. Returns `None` if not stored.
    #[inline]
    pub fn get_physical_props(&self, id: MaterialId) -> Option<&MaterialPhysicalProps> {
        self.id_to_physical.get(&id)
    }

    /// Get physical properties for a material by name.
    ///
    /// Convenience method that resolves name→ID→props in one call.
    /// Returns `None` if material is unregistered or has no properties.
    #[inline]
    pub fn get_physical_props_by_name(&self, name: &str) -> Option<&MaterialPhysicalProps> {
        let id = self.name_to_id.get(name.trim())?;
        self.id_to_physical.get(id)
    }

    /// Validate that a conductor material has all required physical properties.
    pub fn validate_conductor_props(&self, id: MaterialId, name: &str) -> Result<(), String> {
        if !self.is_conductor(id) {
            return Ok(());
        }

        let props = self.get_physical_props(id).ok_or_else(|| {
            format!(
                "Conductor material '{}' has no physical properties defined",
                name
            )
        })?;

        let mut missing = Vec::new();

        if !props.has("resistivity") {
            missing.push("resistivity");
        }
        if !props.has("thermal_conductivity") {
            missing.push("thermal_conductivity");
        }
        if !props.has("max_current_density") {
            missing.push("max_current_density");
        }

        if missing.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "Conductor material '{}' is missing required physical properties: {}",
                name,
                missing.join(", ")
            ))
        }
    }

    /// Get material physical properties by material ID (alias for get_physical_props).
    /// Returns `None` if no physical properties are stored for this material.
    #[inline]
    pub fn get_material(&self, id: MaterialId) -> Option<&MaterialPhysicalProps> {
        self.get_physical_props(id)
    }
}

impl Default for MaterialRegistry {
    fn default() -> Self {
        Self::new()
    }
}
