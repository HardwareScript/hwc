//! Embedded Standard Library (Runtime Parsing with Caching)

use compact_str::CompactString;
use hwc_parser::{Lexer, Parser, Program};
use rustc_hash::FxHashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// Runtime-loaded stdlib cache (lazy initialization)
static STDLIB_CACHE: std::sync::LazyLock<Mutex<FxHashMap<CompactString, Program>>> =
    std::sync::LazyLock::new(|| Mutex::new(FxHashMap::default()));

/// Get the stdlib directory path
fn get_stdlib_path() -> PathBuf {
    let candidates = vec![
        PathBuf::from("stdlib"),
        PathBuf::from("../stdlib"),
        PathBuf::from("../../stdlib"),
        PathBuf::from("../../../stdlib"),
    ];

    for path in candidates {
        if path.exists() {
            return path;
        }
    }

    PathBuf::from("stdlib")
}

/// Parse a stdlib module from disk
fn parse_stdlib_module(module_path: &str) -> Result<Program, String> {
    let stdlib_dir = get_stdlib_path();
    let file_path = stdlib_dir.join(format!("{}.hw", module_path));

    if !file_path.exists() {
        return Err(format!("Stdlib module not found: {}", module_path));
    }

    let source = std::fs::read_to_string(&file_path)
        .map_err(|e| format!("Failed to read stdlib file: {}", e))?;

    let lexer = Lexer::new(&source);
    let tokens = lexer
        .tokenize()
        .map_err(|e| format!("Lexer error in stdlib: {:?}", e))?;

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

    Ok(program)
}

/// Get parsed program from stdlib (with caching)
pub fn get_stdlib_program(path: &str) -> Option<Program> {
    let mut cache = STDLIB_CACHE.lock().unwrap();

    if let Some(cached) = cache.get(path) {
        return Some(cached.clone());
    }

    match parse_stdlib_module(path) {
        Ok(cached_prog) => {
            cache.insert(path.into(), cached_prog.clone());
            Some(cached_prog)
        }
        Err(_) => None,
    }
}

/// Check if a path exists in stdlib
pub fn has_stdlib_module(path: &str) -> bool {
    get_stdlib_program(path).is_some()
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
