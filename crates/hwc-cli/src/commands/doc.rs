use anyhow::{Context, Result};
use compact_str::CompactString;
use std::fs;
use std::path::PathBuf;

/// Documentation command handler
/// Provides access to globally installed Hardware Script documentation
pub fn execute(args: &[CompactString]) -> Result<()> {
    if args.is_empty() {
        print_usage();
        return Ok(());
    }

    match args[0].as_str() {
        "list" => list_docs(),
        "read" => {
            if args.len() < 2 {
                eprintln!("Error: Missing document name");
                eprintln!("Usage: hpm doc read <document-name>");
                eprintln!("Run 'hpm doc list' to see available documents");
                std::process::exit(1);
            }
            read_doc(&args[1])
        }
        "path" => show_docs_path(),
        _ => {
            eprintln!("Error: Unknown subcommand '{}'", args[0]);
            print_usage();
            std::process::exit(1);
        }
    }
}

fn print_usage() {
    println!("Hardware Script Documentation");
    println!();
    println!("USAGE:");
    println!("    hpm doc <SUBCOMMAND>");
    println!();
    println!("SUBCOMMANDS:");
    println!("    list              List all available documentation");
    println!("    read <name>       Read a specific documentation file");
    println!("    path              Show the global documentation directory path");
    println!();
    println!("EXAMPLES:");
    println!("    hpm doc list");
    println!("    hpm doc read language-spec");
    println!("    hpm doc read ecosystem");
}

fn get_docs_dir() -> Result<PathBuf> {
    // Try multiple locations in order of preference

    // 1. Environment variable override (for testing/development)
    if let Ok(custom_path) = std::env::var("HW_DOCS_PATH") {
        let path = PathBuf::from(custom_path);
        if path.exists() {
            return Ok(path);
        }
    }

    // 2. User's home directory (~/.hw/docs/)
    if let Some(home) = dirs::home_dir() {
        let user_docs = home.join(".hw").join("docs");
        if user_docs.exists() {
            return Ok(user_docs);
        }
    }

    // 3. System-wide installation (/usr/local/share/hw/docs/ on Unix)
    #[cfg(unix)]
    {
        let system_docs = PathBuf::from("/usr/local/share/hw/docs");
        if system_docs.exists() {
            return Ok(system_docs);
        }
    }

    // 4. Windows system installation (C:\Program Files\Hardware Script\docs\)
    #[cfg(windows)]
    {
        let system_docs = PathBuf::from("C:\\Program Files\\Hardware Script\\docs");
        if system_docs.exists() {
            return Ok(system_docs);
        }
    }

    // 5. Fallback to development directory (for contributors)
    let dev_docs = PathBuf::from("Docs/v0.1.3");
    if dev_docs.exists() {
        return Ok(dev_docs);
    }

    anyhow::bail!(
        "Documentation not found. Please install Hardware Script properly.\n\
         Expected location: ~/.hw/docs/\n\
         Run 'hpm install' to set up documentation."
    )
}

fn list_docs() -> Result<()> {
    let docs_dir = get_docs_dir()?;

    println!("Available Documentation:");
    println!();
    println!("Location: {}", docs_dir.display());
    println!();

    // Define the canonical documentation files
    let docs = vec![
        (
            "vision",
            "The Vision & Ideology",
            "Project philosophy and goals",
        ),
        (
            "language-spec",
            "Language Specification",
            "Complete syntax reference and examples",
        ),
        (
            "ecosystem",
            "Ecosystem & File Extensions",
            "Project structure and file types",
        ),
        (
            "compiler-internals",
            "Compiler Internals",
            "Architecture and IR pipeline",
        ),
        (
            "routing-physics",
            "Routing & Physics",
            "Algorithms and constraints",
        ),
    ];

    for (name, title, description) in docs {
        let file_path = docs_dir.join(format!("{}.md", name.to_uppercase()));
        let exists = file_path.exists() || docs_dir.join(format!("{}.md", name)).exists();

        let status = if exists { "✓" } else { "✗" };
        println!("  {} {}", status, name);
        println!("      {}", title);
        println!("      {}", description);
        println!();
    }

    println!("USAGE:");
    println!("    hpm doc read <name>");
    println!();
    println!("EXAMPLES:");
    println!("    hpm doc read language-spec");
    println!("    hpm doc read ecosystem");

    Ok(())
}

fn read_doc(doc_name: &str) -> Result<()> {
    let docs_dir = get_docs_dir()?;

    // Map friendly names to actual file names
    let file_name = match doc_name {
        "language-spec" => "LANGUAGE-SPEC.md",
        "ecosystem" => "ECOSYSTEM.md",
        "routing-physics" => "ROUTING-AND-PHYSICS.md",
        "compiler-internals" => "COMPILER-INTERNALS.md",
        "vision" => "VISION.md",
        "exports" => "EXPORTS-AND-ASSETS.md",
        _ => {
            // Try as-is with .md extension
            &format!("{}.md", doc_name.to_uppercase())
        }
    };

    let doc_path = docs_dir.join(file_name);

    if !doc_path.exists() {
        eprintln!("Error: Documentation '{}' not found", doc_name);
        eprintln!("Expected path: {}", doc_path.display());
        eprintln!();
        eprintln!("Run 'hpm doc list' to see available documents");
        std::process::exit(1);
    }

    // Read and print the documentation directly to stdout
    // This allows LLMs to capture it in their context window
    let content = fs::read_to_string(&doc_path)
        .with_context(|| format!("Failed to read documentation file: {}", doc_path.display()))?;

    println!("{}", content);

    Ok(())
}

fn show_docs_path() -> Result<()> {
    let docs_dir = get_docs_dir()?;
    println!("{}", docs_dir.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_get_docs_dir_with_env_override() {
        let temp_dir = env::temp_dir().join("hw_test_docs");
        fs::create_dir_all(&temp_dir).unwrap();

        env::set_var("HW_DOCS_PATH", temp_dir.to_str().unwrap());

        let result = get_docs_dir();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), temp_dir);

        env::remove_var("HW_DOCS_PATH");
        fs::remove_dir_all(temp_dir).ok();
    }

    #[test]
    fn test_doc_name_mapping() {
        let mappings = vec![
            ("language-spec", "LANGUAGE-SPEC.md"),
            ("ecosystem", "ECOSYSTEM.md"),
            ("routing-physics", "ROUTING-AND-PHYSICS.md"),
        ];

        for (input, expected) in mappings {
            let result = match input {
                "language-spec" => "LANGUAGE-SPEC.md",
                "ecosystem" => "ECOSYSTEM.md",
                "routing-physics" => "ROUTING-AND-PHYSICS.md",
                _ => panic!("Unexpected input"),
            };
            assert_eq!(result, expected);
        }
    }
}
