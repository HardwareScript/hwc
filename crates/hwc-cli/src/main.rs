use clap::{Parser, Subcommand};
use compact_str::CompactString;
use miette::Result;
use std::path::PathBuf;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod commands;

#[derive(Parser)]
#[command(name = "hwc")]
#[command(about = "Hardware Script Language Compiler and Engine (Syntax Unification)", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile a .hw file to output formats
    Build {
        /// Input .hw file
        #[arg(value_name = "FILE")]
        input: PathBuf,

        /// Output directory
        #[arg(short, long, default_value = "build")]
        output: PathBuf,

        /// Export formats (glb, dxf, netlist, all)
        #[arg(short, long, value_delimiter = ',', default_value = "all")]
        formats: Vec<CompactString>,

        /// Build only a specific space (namespace) instead of all spaces
        #[arg(short, long)]
        space: Option<CompactString>,

        /// Skip design rule check (faster iteration, not recommended for production)
        #[arg(long)]
        skip_drc: bool,

        /// Skip physics validation (faster iteration, not recommended for production)
        #[arg(long)]
        skip_physics: bool,

        /// Skip connectivity check (faster iteration, not recommended for production)
        #[arg(long)]
        skip_connectivity_check: bool,

        /// Skip physical continuity check (P41/P42/P43 - for testing individual stages)
        #[arg(long)]
        skip_physical_continuity: bool,

        /// Skip route lockfile (disables incremental routing)
        #[arg(long)]
        no_lockfile: bool,

        /// Force complete reroute (ignore existing lockfile)
        #[arg(long)]
        force_reroute: bool,

        /// Force export even with validation errors (Task 5.3: Override Commit Gate for debugging)
        #[arg(long)]
        force_export: bool,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,

        /// Maximum errors to show (default: 20, professional standard)
        #[arg(long)]
        limit: Option<usize>,

        /// Show all errors (no limit, for SoC-scale designs)
        #[arg(long)]
        all: bool,

        /// Treat warnings as errors
        #[arg(long)]
        deny_warnings: bool,

        /// Debug net identity: trace LogicalNet → RouteSegments → PhysicalRegions decomposition
        #[arg(long)]
        debug_identity: bool,

        /// Run verification only (DRC, connectivity, stackup) without exporting
        #[arg(long)]
        verify_only: bool,
    },

    /// Run design rule check on existing build
    Drc {
        /// Input .hw file
        #[arg(value_name = "FILE")]
        input: PathBuf,

        /// Build directory to check
        #[arg(short, long, default_value = "build")]
        build_dir: PathBuf,
    },

    /// Run physics validation on existing build
    Physics {
        /// Input .hw file
        #[arg(value_name = "FILE")]
        input: PathBuf,

        /// Build directory to check
        #[arg(short, long, default_value = "build")]
        build_dir: PathBuf,

        /// Enable verbose output with detailed analysis
        #[arg(short, long)]
        verbose: bool,

        /// Use parallel validation (faster on multi-core systems)
        #[arg(short, long, default_value = "true")]
        parallel: bool,
    },

    /// Check syntax without building (current syntax validation)
    Check {
        /// Input .hw file
        #[arg(value_name = "FILE")]
        input: PathBuf,

        /// Enable foundry validation (syntax + physics/materials validation)
        #[arg(long)]
        foundry: bool,

        /// Maximum errors to show (default: 20, professional standard)
        #[arg(long)]
        limit: Option<usize>,

        /// Show all errors (no limit, for SoC-scale designs)
        #[arg(long)]
        all: bool,

        /// Show deduplication summary
        #[arg(short, long)]
        verbose: bool,

        /// Treat warnings as errors
        #[arg(long)]
        deny_warnings: bool,
    },

    /// Execute standalone compute scripts or main() (<2ms, zero physical synthesis)
    Run {
        /// Input .hw script file
        #[arg(value_name = "FILE")]
        input: PathBuf,

        /// Optional function name to execute
        #[arg(short, long = "fn")]
        r#fn: Option<CompactString>,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Quick interactive expression or comptime function evaluator (e.g. "4.0um / 1.41um * 350.0")
    Eval {
        /// Expression string (e.g. "4.0um / 1.41um * 350.0") or .hw file
        #[arg(value_name = "TARGET")]
        target: String,

        /// Optional function name to execute if evaluating a file
        #[arg(short, long = "fn")]
        r#fn: Option<CompactString>,

        /// Verbose evaluation output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Run layout synthesis testbenches and assertions (<100ms)
    Test {
        /// Input .hw file
        #[arg(value_name = "FILE")]
        input: PathBuf,

        /// Verbose test output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Initialize a new hardware project
    Init {
        /// Project name
        #[arg(value_name = "NAME")]
        name: CompactString,

        /// Project directory
        #[arg(short, long)]
        path: Option<PathBuf>,
    },

    /// Manage materials database
    Materials {
        #[command(subcommand)]
        action: MaterialsCommand,
    },

    /// Run physics simulation
    Simulate {
        /// Input .hw file
        #[arg(value_name = "FILE")]
        input: PathBuf,

        /// Simulation parameters file
        #[arg(short, long)]
        params: Option<PathBuf>,
    },

    /// Access Hardware Script documentation
    Doc {
        /// Subcommand (list, read, path)
        #[arg(value_name = "SUBCOMMAND")]
        subcommand: CompactString,

        /// Additional arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<CompactString>,
    },
}

#[derive(Subcommand)]
enum MaterialsCommand {
    /// List available materials
    List {
        /// Filter by category (conductor, insulator, semiconductor, resistive)
        #[arg(short, long)]
        category: Option<CompactString>,
    },

    /// Show material properties
    Info {
        /// Material name or symbol
        name: CompactString,
    },

    /// Add custom material
    Add {
        /// Path to material definition file
        file: PathBuf,
    },

    /// Export materials database
    Export {
        /// Output file
        #[arg(short, long, default_value = "materials.yaml")]
        output: PathBuf,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{:?}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Build {
            input,
            output,
            formats,
            space,
            skip_drc,
            skip_physics,
            skip_connectivity_check,
            skip_physical_continuity,
            no_lockfile,
            force_reroute,
            force_export,
            verbose,
            limit,
            all,
            deny_warnings,
            debug_identity,
            verify_only,
        } => commands::build::execute(
            input,
            output,
            formats,
            commands::build::BuildConfig {
                skip_drc,
                skip_physics,
                skip_connectivity_check,
                skip_physical_continuity,
                no_lockfile,
                force_reroute,
                force_export,
                verbose,
                limit,
                all,
                deny_warnings,
                space,
                debug_identity,
                verify_only,
            },
        ),
        Commands::Drc { input, build_dir } => commands::drc::execute(input, build_dir),
        Commands::Physics {
            input,
            build_dir,
            verbose,
            parallel,
        } => commands::physics::execute(input, build_dir, verbose, parallel),
        Commands::Check {
            input,
            foundry,
            limit,
            all,
            verbose,
            deny_warnings,
        } => commands::check::execute(input, foundry, limit, all, verbose, deny_warnings),
        Commands::Run { input, r#fn, verbose } => commands::run::execute(input, r#fn, verbose),
        Commands::Eval { target, r#fn, verbose } => commands::eval::execute(target, r#fn, verbose),
        Commands::Test { input, verbose } => commands::test_cmd::execute(input, verbose),
        Commands::Init { name, path } => commands::init::execute(name, path),
        Commands::Materials { action } => commands::materials::execute(action),
        Commands::Simulate { input, params } => commands::simulate::execute(input, params),
        Commands::Doc { subcommand, args } => {
            let mut all_args = vec![subcommand];
            all_args.extend(args);
            commands::doc::execute(&all_args).map_err(|e| miette::miette!("{}", e))
        }
    }
}
