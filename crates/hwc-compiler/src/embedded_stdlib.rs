//! Embedded Standard Library (Runtime Parsing with Caching)
//!
//! The HardwareScript standard library is parsed on-demand at runtime
//! and cached in memory for subsequent uses.
//!
//! Architecture:
//! - First access: Parse .hw file from disk (~50-200ms)
//! - Subsequent access: Return cached AST (~0.1ms)
//! - Thread-safe lazy loading with std::sync::LazyLock
//! - Each cached module has its own arena that lives for 'static
//! - No build-time pre-compilation (faster builds, simpler architecture)

use compact_str::CompactString;
use hwc_parser::{Definition, Lexer, Parser};
use rustc_hash::FxHashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// Cached stdlib module
/// 
/// CRITICAL: Must store BOTH definitions AND arena together.
/// Definitions contain arena IDs that reference data in the arena.
/// Storing definitions without arena causes index-out-of-bounds panics.
struct CachedModule {
    definitions: Vec<Definition>,
    arena: hwc_parser::ast::arena::AstArena,
}

/// Runtime-loaded stdlib cache (lazy initialization)
static STDLIB_CACHE: std::sync::LazyLock<Mutex<FxHashMap<CompactString, CachedModule>>> =
    std::sync::LazyLock::new(|| Mutex::new(FxHashMap::default()));

/// Get the stdlib directory path
fn get_stdlib_path() -> PathBuf {
    // Try multiple possible locations
    let candidates = vec![
        PathBuf::from("stdlib"),          // When running from hwc/
        PathBuf::from("../stdlib"),       // When running from hwc/target/
        PathBuf::from("../../stdlib"),    // When running from hwc/target/debug/
        PathBuf::from("../../../stdlib"), // When running from hwc/target/debug/deps/
    ];

    for path in candidates {
        if path.exists() {
            return path;
        }
    }

    // Fallback: assume stdlib is in current directory
    PathBuf::from("stdlib")
}

/// Parse a stdlib module from disk
fn parse_stdlib_module(module_path: &str) -> Result<CachedModule, String> {
    let stdlib_dir = get_stdlib_path();
    let file_path = stdlib_dir.join(format!("{}.hw", module_path));

    if !file_path.exists() {
        return Err(format!("Stdlib module not found: {}", module_path));
    }

    let source = std::fs::read_to_string(&file_path)
        .map_err(|e| format!("Failed to read stdlib file: {}", e))?;

    // Lex
    let lexer = Lexer::new(&source);
    let tokens = lexer
        .tokenize()
        .map_err(|e| format!("Lexer error in stdlib: {:?}", e))?;

    // Parse
    let collector = hwc_diagnostics::DiagnosticCollector::new_with_file(
        &source,
        &file_path.to_string_lossy(),
        20,
    );
    let mut parser = Parser::new(tokens);
    let program = parser.parse(&collector);

    if collector.has_errors() {
        return Err(format!("Parse errors in stdlib: {}", collector.summary()));
    }

    Ok(CachedModule {
        definitions: program.definitions,
        arena: program.arena,
    })
}

/// Get parsed definitions from stdlib (with caching)
///
/// This function:
/// 1. Checks the cache first
/// 2. If not cached, parses from disk and caches the result with its own arena
/// 3. Returns references to the cached AST
///
/// Note: Stdlib caching is safe because stdlib files don't change during development.
/// User code is NOT cached to avoid stale state issues.
/// Each cached stdlib module has its own arena that lives for 'static.
///
/// Performance:
/// - First access: ~50-200ms (parsing from disk)
/// - Subsequent access: ~0.1ms (cache lookup)
///
/// Returns: (definitions, arena) tuple. The arena must be kept alive as long as
/// the definitions are being used, since Definition variants contain IDs that
/// index into the arena.
pub fn get_stdlib_definitions(path: &str) -> Option<(Vec<Definition>, hwc_parser::ast::arena::AstArena)> {
    let mut cache = STDLIB_CACHE.lock().unwrap();

    // Check cache first
    if let Some(cached) = cache.get(path) {
        return Some((cached.definitions.clone(), cached.arena.clone()));
    }

    // Not in cache - parse from disk
    match parse_stdlib_module(path) {
        Ok(cached_module) => {
            let defs = cached_module.definitions.clone();
            let arena = cached_module.arena.clone();
            cache.insert(path.into(), cached_module);
            Some((defs, arena))
        }
        Err(_) => {
//             eprintln!("[STDLIB ERROR] Failed to load {}: {}", path, e);
            None
        }
    }
}

/// Check if a path exists in stdlib
pub fn has_stdlib_module(path: &str) -> bool {
    // Try to load it - if successful, it exists
    get_stdlib_definitions(path).is_some()
}

/// Get list of all available stdlib modules
pub fn list_stdlib_modules() -> Vec<CompactString> {
    let stdlib_dir = get_stdlib_path();
    let mut modules = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&stdlib_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "hw") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    modules.push(name.into());
                }
            } else if path.is_dir() {
                // Recursively list subdirectories
                if let Some(dir_name) = path.file_name().and_then(|s| s.to_str()) {
                    list_modules_recursive(&path, dir_name, &mut modules);
                }
            }
        }
    }

    modules
}

fn list_modules_recursive(dir: &PathBuf, base_path: &str, modules: &mut Vec<CompactString>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "hw") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    modules.push(format!("{}/{}", base_path, name).into());
                }
            } else if path.is_dir() {
                if let Some(dir_name) = path.file_name().and_then(|s| s.to_str()) {
                    let new_base = format!("{}/{}", base_path, dir_name);
                    list_modules_recursive(&path, &new_base, modules);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_stdlib_path_returns_pathbuf() {
        // Simple test: verify the function returns a PathBuf without hanging
        let path = get_stdlib_path();
        assert!(path.to_str().is_some(), "Path should be valid UTF-8");
        // Don't check if it exists - that's environment-dependent
    }

    #[test]
    fn test_parse_stdlib_module_handles_missing_file() {
        // Test that parsing a nonexistent module returns an error quickly
        let result = parse_stdlib_module("definitely/does/not/exist");
        assert!(
            result.is_err(),
            "Should return error for nonexistent module"
        );
    }

    #[test]
    fn test_list_stdlib_modules_does_not_panic() {
        // Test that listing modules doesn't panic even if stdlib doesn't exist
        let modules = list_stdlib_modules();
        // We don't assert on the result - just verify it completes
        let _ = modules.len();
    }
}
