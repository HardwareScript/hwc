# Hardware Script — Product Roadmap

**Mission**: Bring the npm/software workflow to hardware development.

**Vision**: Make hardware design as simple as writing code, with deterministic, code-first synthesis and a thriving package ecosystem.

---

## The 5 Critical Problems

To make hardware development feel like software development, we must solve these 5 core problems:

### 1️⃣ Hardware Description Language
**Goal**: Declarative, text-based hardware design with no GUI required.  
**Status**: ✅ Implemented in UHWSL v0.2.1 (Picometer-Precision Vector Language).

### 2️⃣ Component Knowledge Database
**Goal**: Universal component library with electrical limits, pins, footprints, and 3D models.  
**Status**: 🔄 In Progress — `@std` primitive library active, public HPM package registry in progress.

### 3️⃣ Physics/Electrical Validation
**Goal**: Compiler-level error checking for electrical, thermal, and physical rules.  
**Status**: ✅ Active — DRC, LVS, PIVB connectivity, Wheeler–Sakurai BEM parasitics, EM, thermal-rise checks.

### 4️⃣ Integrated Toolchain
**Goal**: Single pipeline from `.hw` source code to verified manufacturing outputs.  
**Status**: ✅ Active — `hwc` Rust compiler outputs Gerber X3, Excellon drill, GDSII, DXF, GLB, SPICE suites, and BOM.

### 5️⃣ Parametric Hardware Modules
**Goal**: Reusable hardware components and sub-assemblies packaged like npm dependencies.  
**Status**: 🔄 In Progress — Reusable `.hw` module imports active, parametric stdlib expanding.

---

## Version Roadmap

### ✅ v0.2.1 — Active Release (Current Version)

**Theme**: AST Arena Database Architecture & Continuous Vector Physical Synthesis

**Completed & Active Capabilities:**

*   **Compiler Core & AST Arena**:
    *   Database-driven architecture with Salsa-inspired query-based incremental execution
    *   AST Arena node allocation with zero-copy symbol interning
    *   Picometer-precision continuous vector database (64-bit integer pm, ±9,220 km addressable range)
    *   Binary zero-copy `.hsx` (`hw.lock`) exchange format via `rkyv` serialization
*   **Language & Grammar (UHWSL v0.2.1)**:
    *   Range syntax & vector slicing (`0..N` exclusive, `0..=N` inclusive, `bus[0..7]`)
    *   Dedicated `device` keyword for semiconductor structures with multi-pour terminal bindings
    *   Multi-line block declarations with optional brace grouping (`{ align: ... }`)
    *   Explicit access control (`export` keyword) for symbols and re-exports
    *   Comptime anchor arithmetic (`(Pad_A.center_x + Pad_B.center_x) / 2`)
*   **Physical Synthesis & Routing Engine**:
    *   Topological Line-Search Router using Axis-Aligned Slab Method over `geo-index` ($O(\log N)$ obstacle queries)
    *   Material-Specific Via Depth & Substrate Cutout Resolver
    *   Via array structures and multi-layer depth control
    *   Polygon copper pour generation with thermal relief boundaries
*   **Physics & Validation**:
    *   Full DRC (Design Rule Checking) & LVS (Layout-Versus-Schematic) verification
    *   PIVB (Physical Interconnect Verification & Bridging) net island checker
    *   Automated PDK Geometry Extraction ($AD, AS, PD, PS$)
    *   Wheeler–Sakurai BEM parasitic extraction (trace R/C/L/M)
    *   Crosstalk, electromigration, and IPC-2152 thermal-rise validation
*   **Manufacturing & Export Suite**:
    *   Gerber X3 package (copper layers, solder mask, silkscreen, board edge)
    *   Excellon drill files (plated/non-plated holes)
    *   Extended dual-table BOM (`.csv`) with volume breakdowns and material totals
    *   SPICE netlists (`.sp`) & automated multi-analysis testbenches (`circuit.sp`, `dc.sp`, `ac.sp`, `tran.sp`)
    *   GDSII silicon layout format (`.gds`)
    *   2D DXF mechanical drawings (`.dxf`)
    *   3D GLB meshes (`.glb`) with `earcut` triangulation

---

### 🔄 v0.2.2+ — Near-Term Roadmap

**Theme**: Auto-Routing Refinements & Public Ecosystem Launch

#### Physical Synthesis & Routing
- [ ] **Automatic BGA escape routing** — Fan-out pattern generation for high-density IC packages
- [ ] **Pattern system & length matching** — Meanders for differential pairs and high-speed timing buses
- [ ] **Miter pass** — Automatic 45° corner chamfering for impedance stability
- [ ] **Port-aware routing** — Outer bounding box edge docking without pad interior loops

#### Ecosystem & Tooling
- [ ] **HPM public registry** — Launch GitHub-backed community package index
- [ ] **Language Server Protocol (LSP)** — VS Code extension with auto-complete, diagnostics, and hover docs
- [ ] **CLI lockfile inspector** — `hwc lock inspect` tool for binary exchange file debugging

---

### 📋 v0.3 — Advanced EDA Features

**Theme**: Optimization, Signal Integrity, and Professional Tools

#### Advanced Routing & Simulation
- [ ] Advanced high-frequency coplanar waveguide and microstrip RF routing
- [ ] Integrated Ngspice / Xyce transient simulation solver runner
- [ ] 3D thermal finite-element modeling (FEM) integration

#### Developer Experience
- [ ] VS Code syntax highlighting and extension package
- [ ] Enhanced macro-placement floorplanning auto-solvers

---

### 🚀 v0.4+ — Scale Invariance & Parametric Sub-Assemblies

**Theme**: Hardware as Libraries at All Scales

#### Parametric Standard Library
- [ ] Parametric voltage regulators, motor drivers, and transceiver modules
- [ ] Auto-routing for parametric sub-assemblies (IC + passives + trace layout)
- [ ] Foundational silicon VLSI cell library integration

#### AI Integration
- [ ] Model Context Protocol (MCP) server for AI coding assistants
- [ ] Structured diagnostic JSON output for autonomous agent feedback loops

---

## Technology Stack

*   **Compiler Core**: Rust workspace (`hwc-cli`, `hwc-parser`, `hwc-compiler`, `hwc-engine`, `hwc-physics`, `hwc-export`)
*   **Parsing & Diagnostics**: `logos` lexer, `miette` error diagnostics
*   **Database & Indexing**: 64-bit picometer vector DB, hybrid `rstar` (dynamic) + `geo-index` (static layers)
*   **Solvers & Geometry**: `clarabel` IPM solver, DAG active-set legalizer, `clipper2` 2D copper welder, `earcut` triangulator
*   **Serialization**: `rkyv` zero-copy binary format (`hw.lock` / `.hsx`)
*   **Live Monitor (`hsm`)**: Tauri v2, SolidJS, Babylon.js (3D PBR), PixiJS (2D WebWorker), `uPlot` (SPICE waveforms)

---

## Success Metrics

### v0.2.1 (Active Release)
- ✅ Rust compiler workspace with AST Arena & continuous picometer vector database.
- ✅ Multi-format export (Gerber X3, Excellon, GDSII, GLB, SPICE netlist suite, DXF, BOM).
- ✅ Topological line-search routing & via depth/array controls.
- ✅ Physics validation (DRC, LVS, PIVB, parasitics, crosstalk, thermal/EM).

### v0.2.2+ (Target)
- 100+ GitHub stars.
- Public HPM component registry live.
- VS Code LSP extension published.
- 5+ community-designed boards manufactured using Hardware Script.

---

**Hardware Script** — Making hardware design as simple as writing code.
