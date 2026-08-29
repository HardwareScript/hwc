# Hardware Script — Product Roadmap

**Mission**: Bring the npm/software workflow to hardware development.

**Vision**: Make hardware design as simple as writing code, with deterministic, code-first generative synthesis and a thriving package ecosystem.

---

## The 5 Critical Problems

To make hardware development feel like software development, we must solve these 5 core problems:

### 1️⃣ Hardware Description Language
**Goal**: Declarative, text-based hardware design with no GUI required.  
**Status**: ✅ Implemented in UHWSL v0.3.0 (Turing-Complete Comptime Generative HDL).

### 2️⃣ Component Knowledge Database
**Goal**: Universal component library with electrical limits, pins, footprints, and 3D models.  
**Status**: 🔄 In Progress — `@std` primitive & PDK library active (`@std/pdk/sky130`), public HPM package registry in progress.

### 3️⃣ Physics/Electrical Validation
**Goal**: Compiler-level error checking for electrical, thermal, and physical rules.  
**Status**: ✅ Active — DRC, LVS, PIVB connectivity, Wheeler–Sakurai BEM parasitics, EM, thermal-rise checks.

### 4️⃣ Integrated Toolchain
**Goal**: Single pipeline from `.hw` source code to verified manufacturing outputs.  
**Status**: ✅ Active — `hwc` Rust compiler outputs Gerber X3, Excellon drill, GDSII, DXF, GLB, SPICE suites, and BOM.

### 5️⃣ Parametric Hardware Modules
**Goal**: Reusable hardware components and sub-assemblies packaged like npm dependencies.  
**Status**: 🔄 In Progress — Reusable `.hw` module imports active, `@std` PDK PCells (parametric `sky130_nmos`/`sky130_pmos`/`sky130_tap`/`sky130_cap_mim`) expanding.

---

## Version Roadmap

### ✅ v0.3.0 — Current Release (Milestone Completed)

**Theme**: Turing-Complete Comptime HDL & Data-Oriented Hierarchical Routing

**Completed & Active Capabilities:**

*   **Comptime Evaluation Engine (`hwc-eval`)**:
    *   Linear register-based **Bytecode Virtual Machine** (`VM`) with static activation records — 86-instruction ISA (`OpCode`/`Chunk` model in `hwc-compiler/src/eval/`)
    *   **128-bit integer picometer arithmetic** (`MeasurementValue { raw: i128, dimension }`) with strict dimensional algebra (Length×Length→Area, Voltage×Current→Power, …)
    *   Hermetic **sandbox**: `MAX_EVAL_STEPS = 10_000_000`, `MAX_RECURSION_DEPTH = 256` (Halting-Problem guard)
    *   `Value` model: primitives, `Point2D`/`Point3D`/`Vector2D`/`BoundingBox`, `Array`/`Tuple`/`StructInstance`/`EnumVariant`, and hardware handles (`NetHandle`/`SpaceHandle`/`DeviceHandle`)
*   **Language & Grammar (UHWSL v0.3.0)**:
    *   **Turing-complete surface**: `fn` (typed params, named args, defaults), `struct`, `enum`, native `match`
    *   Expression-oriented `if`/`match`, block tail expressions, `break`/`continue`, `while`
    *   Compound assignment (`+= -= *= /= %=`), bitwise & shift ops, `and`/`or`/`not` keywords (replacing `&& || !`)
    *   Arrays with `.len()`/`.push()`/`.pop()`/`.is_empty()` and `[start..end]` slices; tuple destructuring
    *   Dot `.` namespace/enum access (replacing `::`); `{}` string interpolation; unit converters `.to_float()`/`.to_int()`/`.to_pm()`/`.to_nm()`/`.to_um()`
    *   26 reserved keywords; brace-delimited blocks (no indentation sensitivity)
*   **Physical Synthesis & Routing (DOPHR)**:
    *   **Stage 1 — 3D Volumetric Tensor Global Routing**: `PathFinder` negotiated congestion over `VolumetricTensor3D` G-cells (14 bytes/cell SoA layout)
    *   **Stage 2 — Panel Track Assignment**: interval-graph coloring of track anchors (`routing/track_assign/panel.rs`)
    *   **Stage 3 — Guided Detailed Routing**: lock-free spatial 4-coloring + adaptive guide inflation (`MAX_RETRIES = 8`)
    *   Material-specific via depth & substrate cutout resolver; via arrays; polygon copper pour with thermal relief
*   **Physics & Validation**:
    *   Full DRC (G-Cell SIMD, AVX-512), LVS, PIVB connectivity
    *   Wheeler–Sakurai BEM parasitic extraction (trace R/C/L/M); crosstalk, EM, IPC-2152 thermal-rise
*   **Manufacturing & Export Suite**:
    *   Gerber X3, Excellon drill, GDSII, DXF, GLB, SPICE suite (`circuit.sp`, `dc.sp`, `ac.sp`, `tran.sp`), dual-table BOM CSV
*   **Standard Library (`@std`)**:
    *   `@std/primitives/{units,math}`, `@std/layout/{placement,via,passives,pcb}`
    *   `@std/pdk/sky130/{rules,devices,nmos,pmos,tap,strap,pad,passives}` — PDK PCells & `SKY130_RULES`

---

### 🔄 v0.3.1+ — Near-Term Roadmap

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

### 📋 v0.4 — Advanced EDA Features

**Theme**: Optimization, Signal Integrity, and Professional Tools

#### Advanced Routing & Simulation
- [ ] Integrated Ngspice / Xyce transient simulation solver runner
- [ ] 3D thermal finite-element modeling (FEM) integration
- [ ] Advanced high-frequency coplanar waveguide and microstrip RF routing

#### Developer Experience
- [ ] VS Code syntax highlighting and extension package
- [ ] Enhanced macro-placement floorplanning auto-solvers
- [ ] Machine-readable diagnostic JSON for tooling and CI integration

---

### 🚀 v0.5+ — Scale Invariance & Parametric Sub-Assemblies

**Theme**: Hardware as Libraries at All Scales

#### Parametric Standard Library
- [ ] Parametric voltage regulators, motor drivers, and transceiver modules
- [ ] Auto-routing for parametric sub-assemblies (IC + passives + trace layout)
- [ ] Foundational silicon VLSI cell library integration

---

## Technology Stack

*   **Compiler Core**: Rust workspace (`hwc-cli`, `hwc-parser`, `hwc-compiler`, `hwc-engine`, `hwc-physics`, `hwc-export`, `hwc-materials`, `hwc-stdlib`, `hwc-types`, `hwc-diagnostics`)
*   **Comptime Engine**: `hwc-eval` — Linear Bytecode VM (`eval/vm.rs`), AST→bytecode compiler (`eval/compiler/`), `rkyv`-style `Chunk` constants; 128-bit picometer `Value` model
*   **Routing**: `hwc-engine::routing::dophr` — 3-stage DOPHR (volumetric tensor global → panel track assignment → guided detailed + 4-coloring)
*   **Parsing & Diagnostics**: `logos` lexer, `miette` error diagnostics, Pratt 8-level precedence, arena AST
*   **Database & Indexing**: 64-bit picometer vector DB, hybrid `rstar` (dynamic) + `geo-index` (static layers)
*   **Solvers & Geometry**: `clarabel` IPM solver, DAG active-set legalizer, `clipper2` 2D copper welder, `earcut` triangulator
*   **Serialization**: `rkyv` zero-copy binary format (`hw.lock` / `.hsx`)
*   **Live Monitor (`hsm`)**: Tauri v2, SolidJS, Babylon.js (3D PBR), PixiJS (2D WebWorker), `uPlot` (SPICE waveforms)

---

## Success Metrics

### v0.3.0 (Current Release)
- ✅ Rust compiler workspace with `hwc-eval` Linear Bytecode VM & 128-bit picometer arithmetic.
- ✅ Turing-complete comptime surface (`fn`/`struct`/`enum`/`match`/loops/arrays).
- ✅ DOPHR 3-stage router (global → panel → detailed + 4-coloring).
- ✅ Multi-format export (Gerber X3, Excellon, GDSII, GLB, SPICE netlist suite, DXF, BOM).
- ✅ Physics validation (DRC, LVS, PIVB, parasitics, crosstalk, thermal/EM).
- ✅ `@std` PDK modules for SKY130.

### v0.3.1+ (Target)
- 100+ GitHub stars.
- Public HPM component registry live.
- VS Code LSP extension published.
- 5+ community-designed boards manufactured using Hardware Script.

---

**Hardware Script** — Making hardware design as simple as writing code.
