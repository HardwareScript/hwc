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

### ✅ v0.2.1 - Active Release (Current Version)

**Theme**: Database-Driven AST Arena Architecture & High-Performance Physical Synthesis

**Completed & Active Capabilities:**
- **Compiler Core & AST Arena**:
  - Database-driven architecture with Salsa-inspired query-based incremental execution
  - AST Arena allocation with zero-copy symbol interning and fast node indexing
  - Picometer-precision coordinate database (64-bit integer pm, ±9,220 km addressable range)
  - Memory-mapped zero-copy `.hsx` binary exchange format
- **Language & Syntax Specifications (UHWSL v0.2.1)**:
  - Range syntax & signal indexing (`bus[0..7]`, `pin[1..4]`)
  - Dedicated `device` keyword for multi-gate ICs and precise footprint pin bindings
  - Multi-line block declarations for spaces, modules, and components
  - Explicit symbol export (`export module`, `export component`)
  - Relational placement determinism (`named B at 5mm right of A`)
- **Clippy-Level Error Intelligence**:
  - `hwsd`-powered diagnostic engine with inline code context, structured error codes, actionable fix hints, and JSON output formatting for LLM feedback loops
  - Compile-time parasitic error intelligence (high resistance, crosstalk bounds, impedance mismatch)
- **Physical Synthesis & Routing Engine**:
  - Topological obstacle-aware router using Axis-Aligned Slab Method for $O(\log N)$ obstacle queries
  - Connection interface routing & spatial synthesis abstraction
  - Material-Specific Via Depth & Dielectric Substrate Cutout Resolver (calculates upper/lower layer conductor penetrations vs dielectric Boolean mesh cutouts)
  - Via array specifications (multi-via structures for high-current traces)
  - Via depth control (blind and buried via depth limits across substrate stackup layers)
  - Polygon copper pour generation with thermal relief boundaries
- **Physics & Validation**:
  - Full DRC (Design Rule Checking) & LVS (Layout-Versus-Schematic) verification
  - Automated Silicon PDK Geometry Extraction ($AD, AS, PD, PS$ calculation from physical pours)
  - Crosstalk analysis engine for coupled parallel trace lines
  - Electromigration & Thermal current-density checks
  - Wheeler-Sakurai BEM parasitic extraction (R/C/L/M)
- **Manufacturing & Export Suite**:
  - Complete Gerber X3 package (copper layers, silkscreen, solder mask, board edge)
  - Excellon drill files (plated/non-plated via drills & mounting holes)
  - Extended BOM (`.csv`) with manufacturer part numbers, material volumetric breakdowns, pricing, and tolerances
  - SPICE netlists (`.sp`) & automated multi-analysis testbench suite (`circuit.sp`, `dc.sp`, `ac.sp`, `tran.sp`), DXF CAD drawings (`.dxf`), GLB 3D models (`.glb`), GDSII silicon layout format (`.gds`)

---

### 🔄 v0.2.2+ - Near-Term Roadmap Targets

**Theme**: Auto-Routing Refinements & Public Ecosystem Integration

#### Physical Synthesis & Routing
- [ ] **Automatic BGA escape routing** — Fan-out patterns for high-density IC packages
- [ ] **Pattern system & length matching** — Meander injection for differential pairs and timing constraints
- [ ] **Miter pass** — Automatic 45° chamfering for impedance-stable trace corners
- [ ] **Port-aware routing** — Outer bounding box edge docking (no inside-routing)

#### Ecosystem & Tooling
- [ ] **HPM public registry** — Launch community package registry on GitHub
- [ ] **Language Server Protocol (LSP)** — Real-time auto-complete, diagnostics, and hover docs in VS Code
- [ ] **HWSD auto-documentation** — Generate full HTML doc sites from `##` doc comments
- [ ] **CLI inspect tool** — `hwc lock inspect` for human-readable lockfile viewing

**Target**: Production-ready PCB manufacturing pipeline and published HPM component registry

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

###  v0.4+ - Scale Invariance & Parametric Modules (2027+)

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





## Success Metrics

### v0.2.1 (Active Release)
- ✅ Unified Rust compiler workspace with AST Arena & Query engine.
- ✅ Multi-format export (Gerber X3, Excellon, GDSII, OBJ, GLB, SPICE, DXF, BOM).
- ✅ Topological obstacle-aware routing & via depth/array controls.
- ✅ Clippy-level error diagnostics & physics checks (DRC, LVS, crosstalk, thermal/EM).
- ✅ Deterministic compilation & Hardware-Script-driven test suite.

### v0.2.2+ (Target)
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

**Join us**: [[GitHub Repository URL](https://github.com/HardwareScript/hwc)]
