//! Core Physical Types, Identity & Base Database (Phase 1)

pub mod freeze_lock;
pub mod identity;
pub mod registry;

pub use freeze_lock::BaseSiliconLock;
pub use identity::{EntityId, HierarchicalPath, PathSegment};
pub use registry::IdentityRegistry;
