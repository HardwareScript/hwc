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

### ✅ v0.1.8 - Active Development (Q2 2026 – Present)

**Theme**: Vector-First Architecture & Advanced Physical Synthesis

**Completed:**
- Unified v0.1.6 syntax (bare identifiers, `[]` lists, `:` vs `=` boundary)
- Semantic layer abstraction (`on layer: metal1` replaces raw Z coordinates)
- Picometer-precision coordinate database (64-bit integer pm, ±9,220 km range)
- Native Rust compiler with 7+ specialized crates
- Symbol table with stackup, materials, profiles, components
- Two-pass compilation (resolution → IR generation)
- Logical netlist synthesis from `module` blocks
- Physical layout validation (LVS, continuity, DRC)
- PIVB solver for connectivity analysis
- Device binding validation (physical pours → logical terminals)
- Manual routing with explicit `path:` statements
- Pour generation with boundary definitions
- Via resolver infrastructure
- Bridge rule parsing and storage
- Standard library auto-loading (`@std/units.hw`)
- Export: SPICE (`.sp`), BOM (`.csv`), GLB (`.glb`), DXF (`.dxf`)
- Comprehensive test suite (ASIC designs, capacitors, traces)
- `miette`-powered error diagnostics with context

**In Active Development:**
- **Vector-first routing engine** — Transition from voxel grid to continuous coordinates
- **Zero-stamping scene graph** — ComponentStamp + FixedTransform2D instances
- **Hybrid spatial indexing** — `rstar` (dynamic macro-placement) + `geo-index` (static detailed routing)
- **Topological line-search router** — Axis-Aligned Slab Method for $O(\log N)$ obstacle queries
- **Multi-layer automatic routing** — Via/contact insertion with bridge rule application
- **Pattern-guided meander injection** — Closed-form polar decomposition for length matching
- **Convex legalization** — Hybrid `clarabel` (macro) + active-set/DAG (micro) solvers
- **G-cell-local unified sweep** — SIMD-accelerated DRC + same-net topology in single pass
- **Wheeler-Sakurai-Greenhouse BEM** — Analytic parasitic extraction (R/C/L/M)

---

### 🔄 v0.2 - Production Release (Target: Q3-Q4 2026)

**Theme**: Production-Ready Routing & Complete Export Pipeline

#### Physical Synthesis
- [ ] **Multi-layer auto-router** — Complete via/contact insertion with bridge rule application
- [ ] **Pattern system** — `pattern` and `strategy` definitions for meander injection
- [ ] **Miter pass** — Automatic 45° chamfering for impedance-stable corners
- [ ] **Port-aware routing** — Outer bounding box edge docking (no inside-routing)
- [ ] **Boundary track buffering** — G-cell interface port negotiation
- [ ] **Diagonal grid-snapping** — $L_{\text{snapped}} = \text{round}(N \cdot \text{pitch} / \sin(45°))$

#### Export & Manufacturing
- [ ] **Complete Gerber package** — All copper layers, silkscreen, solder mask, board edge
- [ ] **Excellon drill files** — Via drills, mounting holes, proper tooling
- [ ] **Pick-and-place** — CPL format with component positions and rotations
- [ ] **Enhanced BOM** — Manufacturer part numbers, datasheets, pricing
- [ ] **GDSII export** — Full silicon foundry format for ASIC manufacturing

#### Language & Syntax  
- [ ] **Formal UHWSL v1.0 spec** — Language specification freeze
- [ ] **Signal groups** — Differential pairs, impedance matching, timing constraints
- [ ] **Interface blocks** — Firmware bindings for hardware↔software API
- [ ] **Test blocks** — CI/CD physics assertions for automated validation

#### Tooling
- [ ] **HPM package registry** — Launch public GitHub-based component registry
- [ ] **HWSD documentation** — Auto-generate docs from `##` comments
- [ ] **Binary lockfile** — `rkyv` + `memmap2` zero-copy deserialization
- [ ] **CLI inspect tool** — `hwc lock inspect` for human-readable lockfile viewing

**Target**: First production PCBs manufactured from HardwareScript source code

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

**Join us**: [[GitHub Repository URL](https://github.com/HardwareScript/hwc)]
