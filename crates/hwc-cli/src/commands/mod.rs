pub mod build_cmd;
pub mod check;
pub mod doc;
pub mod drc;
pub mod init;
pub mod materials;
pub mod physics;
pub mod simulate;

// Re-export build for backward compatibility
pub use build_cmd as build;
