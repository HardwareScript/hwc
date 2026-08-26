//! Path resolution logic for imports (relative, stdlib, external).

use crate::embedded_stdlib;
use crate::module_resolver::ResolverError;
use std::path::{Path, PathBuf};

impl super::ModuleResolver {
    /// Find the standard library path relative to the compiler crate
    pub(super) fn find_stdlib_path() -> Result<PathBuf, ResolverError> {
        let compiler_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let stdlib_path = compiler_dir
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("stdlib"))
            .ok_or_else(|| ResolverError::StdlibNotFound {
                path: "stdlib directory".into(),
                span: None,
            })?;

        if !stdlib_path.exists() {
            return Err(ResolverError::StdlibNotFound {
                path: stdlib_path.display().to_string(),
                span: None,
            });
        }

        Ok(stdlib_path)
    }

    /// Resolve a module path to a file path (from string)
    pub(super) fn resolve_import_path(
        &self,
        from_str: &str,
        source_file: &Path,
    ) -> Result<PathBuf, ResolverError> {
        let clean = from_str.trim_matches('"');
        if clean.starts_with("@std/") {
            self.resolve_stdlib_path(&clean["@std/".len()..])
        } else if clean.starts_with('@') {
            let parts: Vec<&str> = clean.splitn(2, '/').collect();
            let org = parts[0].trim_start_matches('@').to_string();
            let name = parts.get(1).unwrap_or(&"").to_string();
            if org == "std" {
                self.resolve_stdlib_path(&name)
            } else {
                Err(ResolverError::ExternalPackageNotSupported {
                    org: org.into(),
                    name: name.into(),
                    span: None,
                })
            }
        } else {
            self.resolve_relative_path(clean, source_file)
        }
    }



    /// Resolve a path relative to the source file's directory
    pub(super) fn resolve_relative_path(
        &self,
        path_str: &str,
        source_file: &Path,
    ) -> Result<PathBuf, ResolverError> {
        let source_dir = source_file
            .parent()
            .ok_or_else(|| ResolverError::FileNotFound {
                path: format!(
                    "Cannot determine parent directory of {}",
                    source_file.display()
                ),
                span: None,
            })?;

        let mut file_path = source_dir.join(path_str);

        if file_path.extension().is_none() {
            file_path.set_extension("hw");
        }

        let canonical_path = file_path
            .canonicalize()
            .map_err(|_| ResolverError::FileNotFound {
                path: format!("{} (relative to {})", path_str, source_file.display()),
                span: None,
            })?;

        Ok(canonical_path)
    }

    /// Resolve a standard library path
    pub(super) fn resolve_stdlib_path(&self, name: &str) -> Result<PathBuf, ResolverError> {
        if embedded_stdlib::has_stdlib_module(name) {
            return Ok(PathBuf::from(format!("@std/{}", name)));
        }

        let file_path = self.stdlib_path.join(format!("{}.hw", name));

        if !file_path.exists() {
            return Err(ResolverError::FileNotFound {
                path: format!(
                    "@std/{} (not in embedded stdlib or at {})",
                    name,
                    file_path.display()
                ),
                span: None,
            });
        }

        Ok(file_path)
    }
}
