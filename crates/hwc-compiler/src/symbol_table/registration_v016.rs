//! Symbol Table Registration with DiagnosticCollector (v0.1.6)
//!
//! This module demonstrates how to update the Symbol Table registration
//! methods to use the DiagnosticCollector for multi-error reporting.
//!
//! This is an EXAMPLE implementation showing the migration pattern.
//! The actual implementation should be integrated into the existing
//! registration.rs file.

use crate::diagnostic_collector::DiagnosticCollector;
use crate::symbol_table::error::SymbolError;
use rustc_hash::FxHashMap;

/// Example: Material definition with span tracking
#[derive(Debug, Clone)]
pub struct Material {
    pub name: CompactString,
    pub span: (usize, usize),
    // ... other fields
}

/// Example: Component definition with span tracking
#[derive(Debug, Clone)]
pub struct Component {
    pub name: CompactString,
    pub span: (usize, usize),
    // ... other fields
}

/// Example: Symbol Table with collector-based registration
pub struct SymbolTableV016 {
    pub materials: FxHashMap<CompactString, Material>,
    pub components: FxHashMap<CompactString, Component>,
}

impl SymbolTableV016 {
    pub fn new() -> Self {
        Self {
            materials: FxHashMap::default(),
            components: FxHashMap::default(),
        }
    }

    /// Register a material with error recovery.
    ///
    /// **Before (v0.1.5 - Panic Mode)**:
    /// ```rust,ignore
    /// pub fn register_material(&mut self, name: CompactString, span: (usize, usize)) -> Result<(), SymbolError> {
    ///     if self.materials.contains_key(&name) {
    ///         return Err(SymbolError::DuplicateDefinition { ... }); // STOPS HERE
    ///     }
    ///     self.materials.insert(name, Material { name, span });
    ///     Ok(())
    /// }
    /// ```
    ///
    /// **After (v0.1.6 - Error Recovery)**:
    /// ```rust,ignore
    /// pub fn register_material(&mut self, collector: &mut DiagnosticCollector, name: CompactString, span: (usize, usize)) {
    ///     if self.materials.contains_key(&name) {
    ///         collector.report(SymbolError::DuplicateDefinition { ... }); // REPORT AND CONTINUE
    ///         return; // Skip this one, but keep going
    ///     }
    ///     self.materials.insert(name.clone(), Material { name, span });
    /// }
    /// ```
    pub fn register_material(
        &mut self,
        collector: &mut DiagnosticCollector,
        name: CompactString,
        span: (usize, usize),
    ) {
        // Check for duplicate
        if let Some(existing) = self.materials.get(&name) {
            // Report the error (don't return Err)
            collector.report(SymbolError::duplicate(
                name.clone(),
                "material",
                span,
                Some(existing.span),
            ));

            // Strategy: Skip this definition to avoid overwriting valid definitions
            // Alternative: Replace old with new (depends on use case)
            return;
        }

        // Check if we should stop (hit error limit)
        if collector.should_stop() {
            return;
        }

        // Register the material
        self.materials
            .insert(name.clone(), Material { name, span });
    }

    /// Register a component with error recovery.
    pub fn register_component(
        &mut self,
        collector: &mut DiagnosticCollector,
        name: CompactString,
        span: (usize, usize),
    ) {
        // Check for duplicate
        if let Some(existing) = self.components.get(&name) {
            // Report the error (don't return Err)
            collector.report(SymbolError::duplicate(
                name.clone(),
                "component",
                span,
                Some(existing.span),
            ));

            // Skip this definition
            return;
        }

        // Check if we should stop (hit error limit)
        if collector.should_stop() {
            return;
        }

        // Register the component
        self.components
            .insert(name.clone(), Component { name, span });
    }

    /// Batch register multiple materials (demonstrates loop pattern).
    ///
    /// This shows how to process multiple items with error recovery.
    pub fn register_materials_batch(
        &mut self,
        collector: &mut DiagnosticCollector,
        materials: Vec<(String, (usize, usize))>,
    ) {
        for (name, span) in materials {
            self.register_material(collector, name, span);

            // Stop if we hit the error limit
            if collector.should_stop() {
                break;
            }
        }
    }
}

/// Backward compatibility adapter for old Result-based code.
///
/// This allows gradual migration from Result<T, E> to DiagnosticCollector.
impl SymbolTableV016 {
    /// Register a material using the old Result-based API.
    ///
    /// This adapter bridges old code with the new collector-based approach.
    pub fn register_material_compat(
        &mut self,
        name: CompactString,
        span: (usize, usize),
    ) -> Result<(), SymbolError> {
        // Create a temporary collector with limit of 1
        let mut collector = DiagnosticCollector::new("", 1);

        // Call the new collector-based method
        self.register_material(&mut collector, name, span);

        // Convert collector result to Result
        if collector.has_errors() {
            // Extract the first error
            // Note: This is a simplified conversion; real implementation
            // would need to convert miette::Report back to SymbolError
            Err(SymbolError::undefined(
                "conversion error".into(),
                "material",
                Some(span),
            ))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
use compact_str::CompactString;

    #[test]
    fn test_register_material_success() {
        let mut table = SymbolTableV016::new();
        let mut collector = DiagnosticCollector::new("", 10);

        table.register_material(&mut collector, "FR4".into(), (0, 10));

        assert_eq!(collector.error_count(), 0);
        assert!(table.materials.contains_key("FR4"));
    }

    #[test]
    fn test_register_material_duplicate() {
        let mut table = SymbolTableV016::new();
        let mut collector = DiagnosticCollector::new("", 10);

        // Register first material
        table.register_material(&mut collector, "FR4".into(), (0, 10));
        assert_eq!(collector.error_count(), 0);

        // Try to register duplicate
        table.register_material(&mut collector, "FR4".into(), (20, 30));
        assert_eq!(collector.error_count(), 1);

        // First definition should still be in the table
        assert_eq!(table.materials.get("FR4").unwrap().span, (0, 10));
    }

    #[test]
    fn test_register_materials_batch() {
        let mut table = SymbolTableV016::new();
        let mut collector = DiagnosticCollector::new("", 10);

        let materials = vec![
            ("FR4".into(), (0, 10)),
            ("Copper".into(), (20, 30)),
            ("FR4".into(), (40, 50)), // Duplicate
            ("Air".into(), (60, 70)),
        ];

        table.register_materials_batch(&mut collector, materials);

        // Should have 1 error (duplicate FR4)
        assert_eq!(collector.error_count(), 1);

        // Should have registered 3 materials (FR4, Copper, Air)
        assert_eq!(table.materials.len(), 3);
        assert!(table.materials.contains_key("FR4"));
        assert!(table.materials.contains_key("Copper"));
        assert!(table.materials.contains_key("Air"));
    }

    #[test]
    fn test_error_limit() {
        let mut table = SymbolTableV016::new();
        let mut collector = DiagnosticCollector::new("", 3);

        // Register first material
        table.register_material(&mut collector, "FR4".into(), (0, 10));

        // Try to register 5 duplicates (should stop at 3 errors)
        for i in 1..=5 {
            table.register_material(&mut collector, "FR4".into(), (i * 10, i * 10 + 10));
            if collector.should_stop() {
                break;
            }
        }

        // Should have exactly 3 errors (hit the limit)
        assert_eq!(collector.error_count(), 3);
    }

    #[test]
    fn test_backward_compat() {
        let mut table = SymbolTableV016::new();

        // Old API should still work
        let result = table.register_material_compat("FR4".into(), (0, 10));
        assert!(result.is_ok());

        // Duplicate should return Err
        let result = table.register_material_compat("FR4".into(), (20, 30));
        assert!(result.is_err());
    }
}
