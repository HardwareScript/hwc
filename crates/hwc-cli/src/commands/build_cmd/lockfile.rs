use crate::commands::build_cmd::BuildConfig;
use hwc_engine::HardwareSpace;
use miette::Result;
use std::path::Path;
use std::time::Instant;

/// Handle route lockfile loading and saving
pub fn handle_lockfile(
    input: &Path,
    space: &HardwareSpace,
    config: &BuildConfig,
    _start_time: Instant,
) -> Result<()> {
    let lockfile_path = input.with_extension("hw.routes.lock");

    // Load existing lockfile if enabled
    let loaded_lockfile = if !config.no_lockfile && !config.force_reroute && lockfile_path.exists()
    {
        if config.verbose {
            println!("📋 Loading route lockfile: {}", lockfile_path.display());
        }
        hwc_engine::geometry_router::RouteLockfile::load(&lockfile_path)
    } else {
        None
    };

    // Log lockfile status
    if config.verbose {
        if let Some(ref lockfile) = loaded_lockfile {
            println!("   ✅ Loaded {} locked routes", lockfile.routes.len());
        } else if config.no_lockfile {
            println!("   ⚠️  Lockfile disabled");
        } else if config.force_reroute {
            println!("   ⚠️  Lockfile ignored (force reroute)");
        } else {
            println!("   ℹ️  No existing lockfile found");
        }
    }

    // Generate new lockfile after successful routing
    if !config.no_lockfile {
        use hwc_engine::geometry_router::{GridMetadata, RouteLockfile};

        let grid_metadata = GridMetadata {
            dimensions: [space.grid.x_cols, space.grid.y_rows, space.grid.z_layers],
            resolution: [
                space.voxel_size.x_nm as f64 / 1_000_000.0,
                space.voxel_size.y_nm as f64 / 1_000_000.0,
                space.voxel_size.z_nm as f64 / 1_000_000.0,
            ],
        };

        let mut new_lockfile = RouteLockfile::new(space.name.clone(), grid_metadata);

        // TODO: Collect routes from voxel grid
        // For now, we create an empty lockfile to establish the workflow

        new_lockfile.sort_routes();

        if let Err(e) = new_lockfile.save(&lockfile_path) {
            if config.verbose {
                println!("   ⚠️  Failed to save lockfile: {}", e);
            }
        } else if config.verbose {
            println!("   ✅ Saved route lockfile: {}", lockfile_path.display());
        }
    }

    Ok(())
}
