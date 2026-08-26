//! AstArena: Centralized storage for all AST nodes
//!
//! All AST nodes are stored in contiguous vectors and referenced
//! via type-safe u32 indices.

use super::core::IndexVec;
use super::id_types::*;
use crate::ast::declarations::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AstArena {
    pub function_defs: IndexVec<FunctionDefId, FunctionDecl>,
    pub struct_defs: IndexVec<StructDefId, StructDecl>,
    pub enum_defs: IndexVec<EnumDefId, EnumDecl>,
    pub space_defs: IndexVec<SpaceDefId, SpaceDecl>,
    pub module_defs: IndexVec<ModuleDefId, ModuleDecl>,
    pub material_defs: IndexVec<MaterialDefId, MaterialDecl>,
    pub profile_defs: IndexVec<ProfileDefId, ProfileDecl>,
    pub device_defs: IndexVec<DeviceDefId, DeviceDecl>,
    pub test_defs: IndexVec<TestDefId, TestDecl>,
}

impl AstArena {
    /// Create a new empty arena
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new arena with pre-allocated capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            function_defs: IndexVec::with_capacity(capacity),
            struct_defs: IndexVec::with_capacity(capacity),
            enum_defs: IndexVec::with_capacity(capacity),
            space_defs: IndexVec::with_capacity(capacity),
            module_defs: IndexVec::with_capacity(capacity),
            material_defs: IndexVec::with_capacity(capacity),
            profile_defs: IndexVec::with_capacity(capacity),
            device_defs: IndexVec::with_capacity(capacity),
            test_defs: IndexVec::with_capacity(capacity),
        }
    }

    #[inline]
    pub fn alloc_function_def(&mut self, fd: FunctionDecl) -> FunctionDefId {
        self.function_defs.push(fd)
    }

    #[inline]
    pub fn alloc_struct_def(&mut self, sd: StructDecl) -> StructDefId {
        self.struct_defs.push(sd)
    }

    #[inline]
    pub fn alloc_enum_def(&mut self, ed: EnumDecl) -> EnumDefId {
        self.enum_defs.push(ed)
    }

    #[inline]
    pub fn alloc_space_def(&mut self, sd: SpaceDecl) -> SpaceDefId {
        self.space_defs.push(sd)
    }

    #[inline]
    pub fn alloc_module_def(&mut self, md: ModuleDecl) -> ModuleDefId {
        self.module_defs.push(md)
    }

    #[inline]
    pub fn alloc_material_def(&mut self, md: MaterialDecl) -> MaterialDefId {
        self.material_defs.push(md)
    }

    #[inline]
    pub fn alloc_profile_def(&mut self, pd: ProfileDecl) -> ProfileDefId {
        self.profile_defs.push(pd)
    }

    #[inline]
    pub fn alloc_device_def(&mut self, dd: DeviceDecl) -> DeviceDefId {
        self.device_defs.push(dd)
    }

    #[inline]
    pub fn alloc_test_def(&mut self, td: TestDecl) -> TestDefId {
        self.test_defs.push(td)
    }

    /// Clear all arena contents
    pub fn clear(&mut self) {
        self.function_defs.clear();
        self.struct_defs.clear();
        self.enum_defs.clear();
        self.space_defs.clear();
        self.module_defs.clear();
        self.material_defs.clear();
        self.profile_defs.clear();
        self.device_defs.clear();
        self.test_defs.clear();
    }

    /// Merge another arena into this one
    pub fn merge(&mut self, other: AstArena) -> AstArenaOffsets {
        let offsets = AstArenaOffsets {
            function_defs: self.function_defs.len(),
            struct_defs: self.struct_defs.len(),
            enum_defs: self.enum_defs.len(),
            space_defs: self.space_defs.len(),
            module_defs: self.module_defs.len(),
            material_defs: self.material_defs.len(),
            profile_defs: self.profile_defs.len(),
            device_defs: self.device_defs.len(),
            test_defs: self.test_defs.len(),
        };

        self.function_defs.extend_from(other.function_defs);
        self.struct_defs.extend_from(other.struct_defs);
        self.enum_defs.extend_from(other.enum_defs);
        self.space_defs.extend_from(other.space_defs);
        self.module_defs.extend_from(other.module_defs);
        self.material_defs.extend_from(other.material_defs);
        self.profile_defs.extend_from(other.profile_defs);
        self.device_defs.extend_from(other.device_defs);
        self.test_defs.extend_from(other.test_defs);

        offsets
    }
}

/// Offsets calculated during AstArena::merge
#[derive(Debug, Clone, Copy, Default)]
pub struct AstArenaOffsets {
    pub function_defs: usize,
    pub struct_defs: usize,
    pub enum_defs: usize,
    pub space_defs: usize,
    pub module_defs: usize,
    pub material_defs: usize,
    pub profile_defs: usize,
    pub device_defs: usize,
    pub test_defs: usize,
}
