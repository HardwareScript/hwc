# Hardware Script - Changelog

All notable changes to this project will be documented in this file.

---

## [Unreleased] - v0.2 Planning

### 🔄 Planned Features

#### High Priority
- Multi-layer routing with automatic via generation
- Component library (resistors, capacitors, ICs)
- Import system for modular designs
- Proper error handling with line numbers
- Collision detection

#### Medium Priority
- Electrical validation (voltage/current checks)
- BOM generation (CSV format)
- Complete Gerber package (drill files, silkscreen)
- Substrate spanning syntax
- Advanced routing parameters (trace width, clearance)

#### Low Priority
- Mesh optimization (merge adjacent copper cells)
- Solder mask layers
- Component footprint library
- Design rule checking (DRC)
- Cost estimation

### 🎯 Goals

- Production-ready manufacturing outputs
- Complete component ecosystem
- Professional PCB design capabilities
- Comprehensive error messages
- Full electrical validation

---

## [0.1.7]

- **Analytic Trace Representation**: Shifted the trace representation to continuous mathematical lines/segments (`AnalyticTrace`), bypassing slow voxel-crawling and lowering memory requirements.
- **Cylindrical Via Representation**: Added plated-through-holes (PTH), circular via-drills, and annular rings ("donuts") according to profile rules.
- **Advanced Material Optics**: Added PBR parameters (`ior`, subsurface scattering, `clearcoat_roughness`, and anisotropy) to simulate realistic fiberglass weave (FR4) and polished coatings.
- **Pin Stitching**: Automatically connects footprint pins to internal routing layers.
- **Minkowski Obstacle Inflation**: Generates 2.5D obstacle AABBs on the fly to accelerate path collision checks.
- **Quadrant Partition Triangulation**: Prevents starburst rendering anomalies and normal inversion in circular mesh exports.
- **Syntax Stabilization**: Hardware Script syntax has achieved partial stabilization, establishing core features for layout and behavior while iterating toward final design goals.
- **Hardware-Script-Driven Testing**: Shifted the testing pipeline to be primarily driven by Hardware Script (`.hw`) test files, with a majority of compiler tests now authored directly in the language.

---

## [0.1.6]

- **Syntax Unification**:
  - Removed `define` keyword and quotes around type names (bare identifiers).
  - Adopted a single `=` for both assignment and comparison.
  - Replaced symbol-only operators with explicit keywords (`and`, `or`, `not`, `xor`).
  - Introduced a universal list parser `[]` for arrays, pin definitions, and configurations.
  - Lowercased `Reg()` to `reg()`.
  - Created the Boundary Law: `:` for static properties, `=` for logic.
- **Assembly Layout Features**:
  - **Relative Placement for Pours**: Positioned pours and contacts using edge and vector offsets (e.g., `M1.right + 2mm`).
  - **Parametric Loop Unrolling**: Support for `for` loops inside the physical space block to stamp geometries.
  - **Geometric Via Linker**: Automatic multi-layer via stack generation and power via arrays.
  - **Component Bounding-Box Obstacles**: Treated complex components as bounding-box "No-Fly Zones" to speed up pathfinding.
  - **Pin-to-Net Binding**: Direct assignment of nets inside unrolled component instantiation blocks.
  - **Spatial Topological Sorting**: Compiles and resolves relative placements in dependency order, removing textual file order restrictions.
- **Safety & Integrity Guards**:
  - **The Commit Gate**: Refuses to export GLB or DXF files if physical validation checks fail.
  - **Substrate Surface Detection**: Flags floating or buried components.
  - **Implicit Overlap Waivers**: Explicit `merge:` and `floating:` syntax to whitelist intentional physics-rule bypasses.
  - **Ohmic Bridge System**: Material-transition rules inserting silicide or solder interfaces at contacts based on profiles.
- **Safe Memory Concurrency**: Swapped unsafe pointers for safe interior mutability (`Arc` + `RwLock`) in the voxel grid to support safe concurrent edits.
- **Visual API (Outline Logic)**: Support for clearcoat, metallic, and roughness values alongside `outline_opacity` to generate "ghost" or outline rendering modes.

---

## [0.1.5]

- **Optimized Performance Storage**:
  - **Magic Morton**: Loop-free, constant-time $O(1)$ bit-interleaving coordinate encoding and decoding.
  - **Virtual Spatial Page Table**: Replaced hash map voxel storage with a flat pointer directory array and 4x4x4 `VoxelChunks` to optimize memory.
  - **Bit-Parallel Occupancy**: Stores layer occupancy as `u64` bitmasks, checking 64 voxels simultaneously.
  - **Material Bit-Planes**: Dedicated fast-path bit-planes for conductors and insulators.
- **Leap-Frog Pathfinding**: Utilizes Signed Distance Fields (SDF) and Sphere Tracing to skip empty areas during A* searches.
- **Voxel Stamps**: Added pre-rasterized standard logic cell footprints for rapid layout stamping.
- **High-Speed Router Enhancements**: Included deterministic tie-breaking in pathfinding and binary collision skips.
- **Dual Coordinate System**: Implemented physics-grounded coordinates, separating absolute physical mass, logical layer indices for Z, and relative percentage (`%`) placement offsets.
- **SoC-Scale Performance Guards**:
  - **Chunk Net Summary**: Net presence Bloom filters inside `VoxelChunks`.
  - **Hierarchical Corridor Search**: Voxel-pyramid coarse grids to guide long-distance routes.
  - **Incremental DRC**: Tracks dirty chunks to recheck only modified board areas.
  - **Coarse Floorplanner**: Force-directed auto-placer grouping components by connectivity on the coarse grid.

---

## [0.1.4] (including v0.1.4.x)

- **Unified Parser & Single File Extension**: Replaced 10+ separate file types (`.hwmat`, `.hwp`, etc.) with a single `.hw` format and unified grammar parser.
- **Two-Pass Compilation**: Added a Symbol Table to register definitions in Pass 1 (Symbol Registration) and assemble the physical space in Pass 2 (Space Assembly).
- **Native SI Unit Parsing**: The lexer parses attached SI units (e.g., `254µm`, `4.7kΩ`, `100nF`) with support for keyboard ASCII aliases (`Ohm`, `uF`).
- **Standard Library Prelude**: Standard units and physical constants automatically load into the compiler context without explicit imports.
- **Comptime Module Flattening**: Added compile-time loop unrolling (inclusive ranges), conditional `if`/`else` evaluation, and variable array index math.
- **Logic Synthesis Block (logic:)**: Introduced behavioral synthesis inside modules, translating math operators (`+`, `-`, bitwise) to logic gates, control flow to multiplexer trees, and sequential registers to D-flip-flops.
- **Clock Domain & CDC Tracking**: Validates clock domains and flags unauthorized clock domain crossings.
- **Combinational Loop Detection**: Builds a logic dependency graph to detect and block invalid combinational feedback loops.
- **Progressive Alignment (LVS)**: Added LVS checking, comparing the physical extracted netlist against the logical schematic.
- **Parallel Domain Partitioning (v0.1.4.2)**: Added domain routing boundaries to parallelize routing across independent modules using Rayon.

---

## [0.1.3]

- **Tri-Fold Case Sensitivity Model**: Standardized lowercase keywords for the software domain, case-sensitive SI units for the physics domain, and case-sensitive identifier matching for the user domain.
- **Unified Origin Syntax**: Introduced 3D origin points combining XY and Z planes (e.g., `tl by t`, representing top-left by top-down Z).
- **Voxel Grid Core**: Implemented a 3D sparse spatial voxel grid using Morton Z-curve encoding for local memory caching.
- **NetlistArena**: Implemented an ECS-style entity-component-system storage layout using strongly-typed IDs (`ComponentId`, `PinId`, `NetId`) for $O(1)$ lookups.
- **3-Phase Routing Pipeline**: Introduced automatic routing consisting of:
  - **Phase 1 (Constraint Manager)**: Translated physical parameters (like IPC-2221 trace width) to geometry.
  - **Phase 2 (Geometry Router)**: Implemented A* pathfinding using a deterministic `VecDeque` tie-breaker and Manhattan routing.
  - **Phase 3 (Design Rule Checker)**: Validated final layouts against physical constraints (voltage drop, temperature rise).
- **Custom Emitters**: Added direct exports for GDSII (silicon), Gerber X3 (PCB), OBJ/GLB (3D), and Blender scene scripts.
- **Trace Optimization**: Integrated Gerber D01 Draw (for traces) and D03 Flash (for pads) optimizations.

---

## [0.1.2]

- **Declarative Coordinate Syntax**: Introduced coordinate pair syntax `[x: N, y: N, z: N]`, allowing variables to be parsed in any order.
- **Coordinate System Reordering**: Swapped the internal coordinate structure to alphabetical XYZ order.
- **Origin Point Abstraction**: Supported 2D origin point tracking (TL - Top-Left, BL - Bottom-Left, TR - Top-Right, BR - Bottom-Right).
- **Manual Waypoint Routing**: Basic manual trace routing using Bresenham 3D line interpolation.

---

## [0.1.0] - 2026-03-13

### 🧪 Initial MVP Beta

First working proof-of-concept of the Hardware Script compiler (Beta), initially implemented in Python.

### ✅ Added

#### Language Features (Initial MVP)
- Space definition with dimensions and grid resolution
- Component placement with rotation support
- Waypoint-based routing with automatic line interpolation
- Comment support (`#` syntax)
- Coordinate system `[Z, X, Y]` for 3D tensor grid

#### Compiler Features (Initial MVP)
- Regex-based lexer with 10 token types
- Imperative parser generating AST
- 3D tensor grid engine using NumPy
- Bresenham line interpolation algorithm
- Physics calculation (trace resistance)

#### Export Formats (Initial MVP)
- **Gerber (GTL)** - PCB manufacturing format
- **Blender Python** - 3D simulation script
- **OBJ** - Universal 3D model format

#### Materials Database (Initial MVP)
- YAML-based material properties
- 8 materials (conductors, insulators, semiconductors)
- Properties: electrical, thermal, physical, mechanical
- Data from Materials Project API

#### Documentation
- Complete v0.1 documentation suite
- Getting Started tutorial
- Language specification
- Architecture guide
- Achievements report
- Quick reference card

### 📊 Metrics (Initial Python MVP)

- **Compiler**: 180 lines of Python
- **Compilation time**: < 10ms for typical boards
- **Memory usage**: < 1 MB for typical boards
- **Dependencies**: 2 (numpy, pyyaml)

### ✅ Verified (Initial Python MVP)

- Test case: 20mm × 20mm board with L-shaped trace
- Output: Valid Gerber, Blender script, OBJ model
- Physics: 0.0101 Ω trace resistance
- Visualization: Confirmed in online viewer and Blender

### 🔬 Proved

- ✅ Hardware can be described in plain text
- ✅ Text compiles to multiple industry formats
- ✅ Discrete 3D grid is practical for hardware design
- ✅ Physics integration works at compile time
- ✅ Deterministic output (same input = same output)

### ⚠️ Known Limitations

- Single layer only (Z=1 tested)
- No component library (placeholders only)
- No import system
- No collision detection
- No electrical validation
- No BOM generation
- No drill file export
- Minimal error handling
- Poor error messages

### 🐛 Known Issues

- Parser has no error recovery
- No line number tracking in errors
- Whitespace sensitivity
- Component pins not validated
- Route endpoints not checked

---

## Version History

### v0.1 (2026-03-13) - MVP Beta Series
**Status**: 🧪 Active Beta / Active Development (currently v0.1.7)  
**Focus**: Proof of concept to continuous-line physical representation  
**Achievement**: Iterative beta compiler with custom emitters and analytic routing.

### v0.2 (Planned Q2 2026) - First Production-Ready Release
**Status**: 🔄 Planning  
**Focus**: Multi-layer support, component library, and API stabilization  
**Goal**: First stable production-ready compiler

### v0.3 (Future) - Advanced Features
**Status**: 💭 Concept  
**Focus**: Optimization and advanced routing  
**Goal**: Professional-grade EDA capabilities

---

## Development Milestones

### Phase 1: Research (Completed)
- ✅ Language design
- ✅ Architecture planning
- ✅ Materials research
- ✅ Feasibility study

### Phase 2: MVP Implementation (Completed)
- ✅ Lexer and parser
- ✅ Grid engine
- ✅ Export formats
- ✅ Materials database
- ✅ Test case verification

### Phase 3: Documentation (Completed)
- ✅ User documentation
- ✅ Developer documentation
- ✅ Tutorial and examples
- ✅ Architecture guide

### Phase 4: Multi-Layer Support (Next)
- 🔄 Via generation
- 🔄 Layer management
- 🔄 Z-axis routing
- 🔄 Plane support

### Phase 5: Component Library (Future)
- 📋 Standard components
- 📋 Footprint definitions
- 📋 Pin mappings
- 📋 3D models

---

## Breaking Changes

### v0.1 → v0.2 (Planned)

**Syntax Changes**:
- None planned (backward compatible)

**API Changes**:
- Internal AST structure will change
- Export functions may be refactored

**File Format**:
- .hw files remain compatible
- Materials database may expand

---

## Deprecations

None yet (v0.1 is first release)

---

## Security

No security issues identified in v0.1.

**Note**: Hardware Script executes user-provided .hw files. Always review code before compilation.

---

## License

**AGPLv3** - Free for open-source use, commercial licensing available for proprietary use.

**Dual Licensing Model**:
- Open source projects: Free under AGPLv3
- Commercial/proprietary use: Separate commercial license required

See [LICENSE](LICENSE) for full details.

---

## Performance

### Rust Compiler Benchmarks (v0.1.7)

| Grid Size | Cells | Memory | Compile Time |
|-----------|-------|--------|--------------|
| 20×20×2 | 800 | < 100 KB | < 1ms |
| 100×100×2 | 20K | < 1 MB | < 5ms |
| 500×500×4 | 1M | < 15 MB | < 50ms |

**Hardware**: Standard development machine  
**Compiler**: Rust (Cargo Workspace)  
**Testing Framework**: Direct Hardware Script execution via workspace integration tests

---

## Contributors

### v0.1 Development Team

- Core architecture and implementation
- Materials database research
- Documentation and examples
- Testing and verification

---

## Acknowledgments

### Data Sources
- **Materials Project** - Material properties (mp-XXXXXXX IDs)
- **Engineering Handbooks** - Physical constants
- **Manufacturer Datasheets** - Component specifications

### Tools
- **Rust** - Main compiler implementation language (logos, miette, rayon)
- **Babylon.js Sandbox** - 3D model (GLB/OBJ) verification
- **LibreCAD** - 2D CAD and DXF verification
- **Gerbv Viewer** - Gerber and drill file verification
- **Python / NumPy / PyYAML** - Initial MVP prototyping (v0.1.0)

---

## Links

- **Documentation**: [Docs/v0.1/INDEX.md](Docs/v0.1/INDEX.md)
- **Getting Started**: [Docs/v0.1/GETTING-STARTED.md](Docs/v0.1/GETTING-STARTED.md)
- **Compiler CLI**: [hwc/crates/hwc-cli](hwc/crates/hwc-cli)
- **Parser & Lexer**: [hwc/crates/hwc-parser](hwc/crates/hwc-parser)
- **Integration Tests (Hardware Script)**: [tests](tests)

---

**Format**: Based on [Keep a Changelog](https://keepachangelog.com/)  
**Versioning**: [Semantic Versioning](https://semver.org/)
