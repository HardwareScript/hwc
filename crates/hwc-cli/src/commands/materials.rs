use crate::MaterialsCommand;
use miette::Result;

pub fn execute(action: MaterialsCommand) -> Result<()> {
    match action {
        MaterialsCommand::List { category: _ } => list_materials(),
        MaterialsCommand::Info { name: _ } => show_material_info(),
        MaterialsCommand::Add { file: _ } => add_custom_material(),
        MaterialsCommand::Export { output: _ } => export_database(),
    }
}

fn list_materials() -> Result<()> {
    println!("📚 Materials in v0.1.4\n");
    println!("Materials are now defined directly in .hw files using 'define material' syntax.");
    println!("\nExample:");
    println!("  define material \"Copper\":");
    println!("      category: Conductor");
    println!("      properties:");
    println!("          resistivity: 1.68e-8Ω·m");
    println!("          density: 8960kg/m³");
    println!("\nSee: hwc/data/standard-materials.hw for examples");
    Ok(())
}

fn show_material_info() -> Result<()> {
    println!("📋 Material Info in v0.1.4\n");
    println!("Materials are now defined in .hw files.");
    println!("Check your project's .hw files or hwc/data/standard-materials.hw");
    Ok(())
}

fn add_custom_material() -> Result<()> {
    println!("➕ Custom Materials in v0.1.4\n");
    println!("Define materials directly in your .hw files:");
    println!("\n  define material \"MyMaterial\":");
    println!("      category: Conductor");
    println!("      properties:");
    println!("          resistivity: 1.0e-8Ω·m");
    Ok(())
}

fn export_database() -> Result<()> {
    println!("✅ Export in v0.1.4\n");
    println!("Materials are already in .hw format - no export needed!");
    println!("See: hwc/data/standard-materials.hw");
    Ok(())
}
