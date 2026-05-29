# Hardware Script - Product Roadmap

**Mission**: Bring the npm/software workflow to hardware development.

**Vision**: Make hardware design as simple as writing code, with AI-native tooling and a thriving package ecosystem.

**The "Matrix Moment"**: Hardware Script's discrete 3D tensor grid unlocks capabilities impossible in traditional tools. Read [VISION.md](VISION.md) for the full story.

---

## The 5 Critical Problems

To make hardware development feel like software development, we must solve these 5 core problems:

### 1️⃣ Hardware Description Language
**Goal**: Text-based hardware design with no GUI required.  
**Status**: ✅ Implemented and stabilizing (v0.1.x)

### 2️⃣ Component Knowledge Database
**Goal**: Universal component library with electrical limits, pins, footprints, and 3D models.  
**Status**: 🔄 In Progress — Standard library in compiler, full public registry planned for v0.2.

### 3️⃣ Physics/Electrical Validation
**Goal**: Compiler-level error checking for electrical and physical violations.  
**Status**: ✅ Core validation active (DRC, LVS, voltage drop, thermal), expanding in v0.2.

### 4️⃣ Integrated Toolchain
**Goal**: Single pipeline from code to manufacturing files.  
**Status**: ✅ Active in v0.1.x — `hwc` Rust compiler outputs Gerber X3, GDSII, DXF, OBJ, GLB, SPICE.

### 5️⃣ Parametric Hardware Modules
**Goal**: Reusable hardware components like npm packages.  
**Status**: 🔄 Partial — module system and HPM registry infrastructure active, parametric stdlib expanding.

---

## Version Roadmap

### ✅ v0.1.x - Active Beta (March 2026 – Present)

**Theme**: Foundation, Engine, and Language Stabilization

**Completed**:
- Unified 3-File Architecture (`hw.toml`, `hw.lock`, `.hw` source).
- Compiled exchange binary format (`.hsx`) — memory-mapped, zero-copy.
- Rust rewrite: `hwc` compiler workspace (`hwc-cli`, `hwc-parser`, `hwc-engine`, `hwc-physics`, `hwc-export`).
- Unified grammar (`v0.1.6`): Dropped `define` keyword, bare identifiers, universal `[]` lists, `:` for structure, `=` for logic.
- v0.1.7 Z-Axis Abstraction Fix: Physical layer names (`layer: l1`) and physical units (`z: 1.5mm`) replacing raw voxel indices.
- `NetlistArena` ECS entity-component storage for $O(1)$ lookups.
- 3D sparse voxel grid with Morton Z-curve encoding.
- 3-Phase automatic routing pipeline (Constraint Manager, A* Geometry Router, DRC).
- Analytic trace representation (`AnalyticTrace`) — continuous mathematical lines, no voxel-crawling.
- Cylindrical via representation (PTH, annular rings).
- Logic synthesis block (`logic:`): operators → gates, control flow → mux trees, D-flip-flops.
- Clock domain and CDC tracking.
- LVS physical-vs-logical netlist comparison.
- Parallel domain partitioning with Rayon.
- Gerber X3, GDSII, DXF, OBJ, GLB, and Blender scene script emitters.
- Hardware Script Monitor (`hsm`): Tauri v2 + SolidJS + Babylon.js + PixiJS + uPlot live preview companion.
- Standard library prelude — SI units and physical constants auto-loaded.
- Comptime loop unrolling, conditional evaluation, and array index math.
- Testing primarily driven by `.hw` integration test files.

---

### 🔄 v0.2 - First Production Release (Target: Q2 2026)

**Theme**: Stabilization, Ecosystem, and Multi-Layer Production

#### Language & Syntax
- [ ] Final syntax freeze and formal UHWSL v1.0 spec.
- [ ] Full `signal_group` block (differential pairs, impedance, timing).
- [ ] Complete `test` block CI/CD assertions.
- [ ] Stable `interface` bindings (hardware ↔ firmware pin API).

#### Component System
- [ ] Formal parametric component generics (`component Resistor (val: Resistance, tol: Ratio):`).
- [ ] Standard component library: resistors, capacitors, inductors, transistors, common ICs.
- [ ] Footprint validation — trace-to-pad and pad-to-pad spacing.
- [ ] 3D `.glb` mesh attachment for all standard components.

#### Package Registry (hpm)
- [ ] Launch `hardwarescript-registry` community GitHub repository.
- [ ] Community contribution flow via Pull Request.
- [ ] Package versioning and `hw.lock` dependency resolution.
- [ ] `hpm publish` and `hpm install` working against live registry.

#### Documentation System (hwsd)
- [ ] Parse `##` documentation comments from `.hw` source.
- [ ] Generate HTML documentation (like rustdoc/HexDocs).
- [ ] LLM-friendly JSON output format.

#### Export Improvements
- [ ] Complete Gerber package: all copper, silkscreen, solder mask, and board-edge layers.
- [ ] Excellon drill file export.
- [ ] BOM generation (CSV format).
- [ ] Pick-and-place file (CPL format).

**Target**: First stable, production-ready PCB workflows for real boards.

---

### 📋 v0.3 - Advanced Features (Q3-Q4 2026)

**Theme**: Optimization, Professional Tools, and Scale

#### Advanced Routing
- [ ] Automatic BGA escape routing.
- [ ] Length matching for high-speed signal pairs.
- [ ] Coplanar waveguide and RF antenna routing.

#### Physics & Simulation
- [ ] Full SPICE waveform simulation integration (Ngspice).
- [ ] Parasitic extraction for high-frequency designs.
- [ ] Thermal finite-element modeling.

#### Developer Experience
- [ ] Language Server Protocol (LSP) for IDE integration.
- [ ] Full VS Code syntax highlighting and auto-complete.
- [ ] `hwcf` — Hardware Script Formatter.
- [ ] `hwcl` — Hardware Script Linter.

**Target**: Professional-grade EDA capabilities matching industry tools.

---

### 💭 v0.4+ - Scale Invariance & Parametric Modules (2027+)

**Theme**: The Holy Grail — Hardware as Libraries at All Scales

#### Parametric Standard Library
- [ ] Parametric voltage regulators, motor drivers, communication interfaces.
- [ ] Auto-routing for parametric modules (compiler generates IC + passives + routing).
- [ ] Silicon VLSI design support.
- [ ] GDSII export for commercial foundries.

#### AI Integration
- [ ] MCP (Model Context Protocol) server for AI agents.
- [ ] Structured JSON error output for LLM feedback loops.
- [ ] OpenAPI endpoint for component database queries.

**Target**: Hardware design as simple as using npm packages, at every scale.

---

## Technology Stack

### Current (v0.1.x)
- **Compiler**: Rust (Cargo workspace)
- **Lexer**: `logos` crate
- **Error Engine**: `miette` crate
- **Parallelism**: `rayon` crate
- **Grid Encoding**: Morton Z-curve (bit-interleaving)
- **Storage**: Flat VoxelChunk pointer directory arrays (Virtual Spatial Page Table)
- **Dependencies (compiler)**: 4 core crates (`logos`, `miette`, `thiserror`, `rayon`)
- **Monitor**: Tauri v2, SolidJS, Babylon.js, PixiJS, uPlot, `dxf-viewer`

### v0.1.0 (Historical Reference)
- **Language**: Python 3.x
- **Parser**: Regex-based
- **Grid**: NumPy arrays
- **Dependencies**: `numpy`, `pyyaml`

---

## Infrastructure

### Package Registry Strategy

**Model**: GitHub-based (like Homebrew or Go modules).

**Cost**: $0 (GitHub hosts everything).

**Architecture**:
```
hardwarescript-registry/
├── registry.yaml          # Maps package names to GitHub URLs
├── packages/
│   ├── power/
│   │   └── voltage_regulator.yaml
│   ├── sensors/
│   │   └── temperature.yaml
│   └── comms/
│       └── uart.yaml
```

**Publishing Flow**:
1. Developer creates a component package (GitHub repo containing `.hw` source files).
2. Developer submits a PR to `hardwarescript-registry`.
3. Community reviews.
4. Merge = package published.

---

## Business Model

### Open Source (AGPLv3)
- Free for hobbyists, students, and open-source projects.
- Community-driven development.
- Public GitHub repository.

### Commercial Licensing
- For corporations that cannot open-source their designs.
- Removes AGPLv3 restrictions.
- Priority support, SLA guarantees, and legal warranties.

**Revenue Model**: Dual licensing (like MongoDB, Qt, Sidekiq).

---

## Success Metrics

### v0.1 (Active)
- ✅ Unified Rust compiler workspace.
- ✅ Multi-format export (Gerber, GDSII, OBJ, GLB, SPICE, DXF).
- ✅ Deterministic builds.
- ✅ Hardware-Script-driven test suite.

### v0.2 (Target)
- 100+ GitHub stars.
- 10+ community contributors.
- 50+ components in registry.
- 5+ real boards designed and manufactured using Hardware Script.

### v0.3 (Target)
- 1,000+ GitHub stars.
- Active community forum.
- 200+ components in registry.
- First commercial license sale.

### v0.4+ (Vision)
- 10,000+ users.
- Industry adoption.
- Educational institutions using it.
- Hardware-as-code becomes standard practice.

---

## Call to Action

### For Users
- Try the compiler: `hwc build your_board.hw`.
- Use `hsm` to preview your designs live.
- Report bugs and issues on GitHub.

### For Contributors
- Build component packages and publish to the registry.
- Improve documentation.
- Add materials to the standard materials database.
- Write integration tests in Hardware Script.

### For Companies
- Evaluate for internal use.
- Contact for commercial licensing.
- Sponsor development.

---

**Hardware Script** - Making hardware design as simple as writing code.

**Join us**: [GitHub Repository URL]
