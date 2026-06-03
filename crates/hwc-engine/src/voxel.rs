//! Material definitions for voxels.
//!
//! This module provides a dynamic material registry system that supports
//! any material defined in .hw files without hardcoding material types.

use compact_str::CompactString;
use rustc_hash::FxHashMap;

/// Material ID type - uses u8 for compact storage in voxel grid.
/// Supports up to 256 different materials.
pub type MaterialId = u8;

/// Reserved material IDs
pub const AIR_MATERIAL_ID: MaterialId = 0;

/// Material conductivity classification for routing logic.
///
/// This classification determines how the router can traverse materials:
/// - **Conductor**: Metal, doped poly - router must avoid if different net
/// - **Semiconductor**: Silicon - router can traverse (substrate material)
/// - **Insulator**: SiO2, Air, FR4 - router can traverse freely
///
/// # Critical for Phase 0 (v0.1.6)
/// The router needs this to distinguish between:
/// - "Impenetrable mass" (conductors of different nets)
/// - "Traversable environment" (insulators and semiconductors)
///
/// Without this, the router creates a "solid block" it cannot enter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MaterialConductivity {
    /// Conductive materials (metals, doped polysilicon)
    /// Router must avoid if assigned to a different net
    Conductor,

    /// Semiconductor materials (silicon substrates)
    /// Router can traverse - these are substrate materials
    Semiconductor,

    /// Insulating materials (oxides, air, dielectrics)
    /// Router can traverse freely - these are "empty space" for routing
    Insulator,
}

/// Manufacturing process behavior for Z-axis placement (v0.1.7)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ManufacturingProcess {
    /// Drilled and plated through the substrate (PCB style)
    DrilledPlated,
    /// Deposited/Plotted into the grid (CMOS/3D-Print style)
    Deposited,
    /// Etched away from existing material (MEMS style)
    Etched,
}

/// Material registry that maps material names to IDs and conductivity classes.
/// This is the "Dynamic Translator" between script-level names and voxel-level IDs.
///
/// # Architecture
/// - **Script Layer**: You write `material N_Doped: semiconductor`
/// - **Registry Layer**: First encounter assigns `N_Doped → ID 1` + conductivity class
/// - **Voxel Layer**: Grid stores `1` (1 byte per voxel)
/// - **Router Layer**: Queries conductivity to determine traversability
/// - **Export Layer**: GDSII asks registry: `ID 1 → "N_Doped" → Layer 17`
///
/// # Performance
/// - Registration: O(1) HashMap lookup (only during compilation)
/// - Lookup: O(1) Vec index (during export and routing)
/// - Voxel storage: 1 byte per voxel (same as hardcoded version)
///
/// # Phase 0 Enhancement (v0.1.6)
/// Added conductivity classification to enable router to distinguish between:
/// - Conductors (must avoid if different net)
/// - Semiconductors (can traverse - substrate)
/// - Insulators (can traverse - empty space)
#[derive(Debug, Clone)]
pub struct MaterialRegistry {
    /// Fast lookup for registration: Name → ID
    name_to_id: FxHashMap<CompactString, MaterialId>,
    /// Fast lookup for export: ID → Name (Vec for O(1) indexing)
    id_to_name: Vec<CompactString>,
    /// Fast lookup for routing: ID → Conductivity (Vec for O(1) indexing)
    id_to_conductivity: Vec<MaterialConductivity>,
    /// Fast lookup for manufacturing: ID → Process (Vec for O(1) indexing)
    id_to_process: Vec<ManufacturingProcess>, // v0.1.7
}

impl MaterialRegistry {
    /// Create a new material registry with Air pre-registered as ID 0.
    pub fn new() -> Self {
        let mut registry = Self {
            name_to_id: FxHashMap::default(),
            id_to_name: vec!["Air".into()],
            id_to_conductivity: vec![MaterialConductivity::Insulator], // Air is an insulator
            id_to_process: vec![ManufacturingProcess::Deposited],     // Air is deposited
        };

        registry.name_to_id.insert("Air".into(), AIR_MATERIAL_ID);
        
        // v0.1.7: Default "Reality as Code" materials
        registry.register_with_conductivity("Copper", MaterialConductivity::Conductor);
        registry.register_with_conductivity("FR4", MaterialConductivity::Insulator);
        registry.register_with_conductivity("Component", MaterialConductivity::Insulator);
        
        registry
    }

    /// Get or register a material and return its ID.
    ///
    /// O(1) in steady state - only incurs HashMap cost the first time
    /// a material is encountered during compilation.
    ///
    /// # Arguments
    /// * `name` - Material name from .hw file
    ///
    /// # Returns
    /// Material ID (0-255)
    ///
    /// # Important
    /// This method defaults to Insulator classification for unknown materials.
    /// You MUST call `populate_from_material_database()` after creating the registry
    /// to get proper conductivity classifications from the MaterialDatabase.
    #[inline]
    pub fn get_or_register(&mut self, name: &str) -> MaterialId {
        if let Some(&id) = self.name_to_id.get(name) {
            return id;
        }

        if self.id_to_name.len() >= 255 {
            panic!("Material registry full! Maximum 255 materials supported.");
        }

        let id = self.id_to_name.len() as u8;
        // Default to Insulator and Deposited - will be updated by populate_from_material_database()
        let conductivity = MaterialConductivity::Insulator;
        let process = ManufacturingProcess::Deposited;

        self.id_to_name.push(name.into());
        self.id_to_conductivity.push(conductivity);
        self.id_to_process.push(process);
        self.name_to_id.insert(name.into(), id);
        id
    }

    /// Register a material with explicit conductivity classification and manufacturing process.
    pub fn register_with_properties(
        &mut self,
        name: &str,
        conductivity: MaterialConductivity,
        process: ManufacturingProcess,
    ) -> MaterialId {
        if let Some(&id) = self.name_to_id.get(name) {
            // Update properties if different
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

    /// Populate conductivity classifications from MaterialDatabase.
    ///
    /// This is the REQUIRED way to classify materials - it looks up the material
    /// type from the MaterialDatabase which was populated from .hw file definitions.
    ///
    /// # Arguments
    /// * `material_db` - MaterialDatabase populated from symbol table
    ///
    /// # Example
    /// ```ignore
    /// let mut registry = MaterialRegistry::new();
    /// let material_db = populate_material_database(&symbol_table)?;
    /// registry.populate_from_material_database(&material_db);
    /// ```
    pub fn populate_from_material_database(
        &mut self,
        material_db: &hwc_materials::MaterialDatabase,
    ) {
        // Map hwc_materials process to engine process
        let map_process = |p: hwc_materials::ManufacturingProcess| match p {
            hwc_materials::ManufacturingProcess::DrilledPlated => ManufacturingProcess::DrilledPlated,
            hwc_materials::ManufacturingProcess::Deposited => ManufacturingProcess::Deposited,
            hwc_materials::ManufacturingProcess::Etched => ManufacturingProcess::Etched,
        };

        // Register all conductors
        for (name, def) in &material_db.conductors {
            self.register_with_properties(
                name,
                MaterialConductivity::Conductor,
                map_process(def.process),
            );
        }

        // Register all semiconductors
        for (name, def) in &material_db.semiconductors {
            self.register_with_properties(
                name,
                MaterialConductivity::Semiconductor,
                map_process(def.process),
            );
        }

        // Register all insulators
        for (name, def) in &material_db.insulators {
            self.register_with_properties(
                name,
                MaterialConductivity::Insulator,
                map_process(def.process),
            );
        }
    }

    /// Get material ID by name. Returns None if not registered.
    pub fn get_id(&self, name: &str) -> Option<MaterialId> {
        self.name_to_id.get(name).copied()
    }

    /// Get material name by ID (O(1) array access).
    /// This is the "Foundry lookup" used during export.
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
    ///
    /// This is the critical method for Phase 0 - enables router to determine
    /// whether it can traverse a voxel based on material conductivity.
    ///
    /// # Arguments
    /// * `id` - Material ID
    ///
    /// # Returns
    /// Conductivity classification, or None if ID is invalid
    ///
    /// # Performance
    /// O(1) array access - optimized for hot path in router
    #[inline]
    pub fn get_conductivity(&self, id: MaterialId) -> Option<MaterialConductivity> {
        self.id_to_conductivity.get(id as usize).copied()
    }

    /// Get conductivity classification by material name.
    ///
    /// # Arguments
    /// * `name` - Material name
    ///
    /// # Returns
    /// Conductivity classification, or None if material not registered
    pub fn get_conductivity_by_name(&self, name: &str) -> Option<MaterialConductivity> {
        self.get_id(name).and_then(|id| self.get_conductivity(id))
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
        matches!(
            self.get_conductivity(id),
            Some(MaterialConductivity::Semiconductor)
        )
    }

    /// Check if a material is an insulator.
    #[inline]
    pub fn is_insulator(&self, id: MaterialId) -> bool {
        matches!(
            self.get_conductivity(id),
            Some(MaterialConductivity::Insulator)
        )
    }
}

impl Default for MaterialRegistry {
    fn default() -> Self {
        Self::new()
    }
}
