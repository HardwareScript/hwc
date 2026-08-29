# Hardware Script - Changelog

All notable changes to this project will be documented in this file.

---

## [0.3.0] - 2026-08-26 — Milestone Completed

**Theme**: Turing-Complete Comptime HDL & Data-Oriented Hierarchical Routing (DOPHR)

This is a breaking, generative-language release. The declarative relational-placement dialect (v0.2.x) was replaced by a brace-delimited, Turing-complete comptime language executed on a Linear Bytecode Virtual Machine.

### ✨ Comptime Evaluation Engine (`hwc-eval`)
- **Linear Bytecode Virtual Machine** (`eval/vm.rs`) — flat activation stack, static activation records, 86-instruction ISA (`eval/opcodes.rs`: `LoadConst`, `Add`/`Sub`/`Mul`/`Div`/`Mod`, `Eq`…`Ge`, `And`/`Or`/`Not`, bitwise & shift, `Jump`/`JumpIfTrue`/`JumpIfFalse`/`LoopStep`, `Call`/`Return`, `AllocArray`/`AllocStruct`/`GetField`/`SetField`/`GetIndex`/`CoercePoint2D`/`CoerceType`/`BuiltinCall`, `EmitPolygon`/`EmitContact`/`EmitDevice`/`EmitRoute`, `Assert`, compound-assign, `JumpForward`/`JumpBack`, array ops, tuple ops, `MeasToFloat`/`MeasToInt`).
- **AST→Bytecode compiler** (`eval/compiler/`) — functions, spaces, structs, and top-level scripts compile to `Chunk` streams.
- **128-bit picometer arithmetic** — `MeasurementValue { raw: i128, dimension }` with strict dimensional algebra (`Length×Length→Area`, `Voltage×Current→Power`, `Current×Resistance→Voltage`). Unit-mismatched math is a compile-time error.
- **Hermetic sandbox** — `MAX_EVAL_STEPS = 10_000_000`, `MAX_RECURSION_DEPTH = 256`.
- **`Value` model** — primitives, `Point2D`/`Point3D`/`Vector2D`/`BoundingBox`, `Array`/`Tuple`/`StructInstance`/`EnumVariant`, and hardware handles (`NetHandle`/`SpaceHandle`/`DeviceHandle`).
- **Built-ins** — `println`, `eprintln`, `dbg`, `assert`, `min`, `max`, `abs`, `sqrt`, `sin`, `cos`, `tan`, `rect_between`, `range`, `int`, `float`, `bbox_intersects`, `bbox_union`, `bbox_from_rect`.

### 🧠 Language & Grammar (UHWSL v0.3.0)
- **Turing-complete surface**: `fn` (typed params, named args, defaults), `struct`, `enum`, native `match`.
- Expression-oriented `if`/`match`, block tail expressions, `while`, `break`/`continue`.
- Compound assignment `+= -= *= /= %=`, bitwise & shift ops.
- `and` / `or` / `not` keywords replace `&& || !`.
- Arrays with `.len()`/`.push()`/`.pop()`/`.is_empty()` and `[start..end]` slices; tuple destructuring (`let (a, b) = ...`, wildcard `_`).
- Dot `.` namespace/enum access replaces `::`.
- `{}` string interpolation; unit converters `.to_float()`/`.to_int()`/`.to_pm()`/`.to_nm()`/`.to_um()`; `bbox.width()`/`height()`/`center()`.
- 26 reserved keywords; brace-delimited blocks (no indentation sensitivity).
- **Breaking removals**: `align with`/`center_x`/`center_y` relational anchors, `spanning layer: X to Y`, `resolution:`, `grid:`, `origin:`, `absolute:`, `device_nets`, `prefer`, `require`, `matrix`, `fill`, `by`, `chain_x`, `shared_gate`, `right_of`/`left_of`/`above`/`below`/`inside`, `::`.

### 🛣️ Routing — DOPHR 3-Stage Guided Router (`hwc-engine::routing::dophr`)
- **Stage 1**: 3D Volumetric Tensor global routing (`VolumetricTensor3D`, 14 bytes/G-cell SoA) with PathFinder negotiated congestion and via-porosity capacity subtraction.
- **Stage 2**: Panel Track Assignment (`routing/track_assign/panel.rs`) via interval-graph coloring; track anchors become mandatory boundary conditions.
- **Stage 3**: Guided Detailed Routing (`detailed/guided_router.rs`) with lock-free spatial 4-coloring (`color_scheduler.rs`) and adaptive guide inflation (`MAX_RETRIES = 8`, `+1` G-cell).
- Closed-loop escapes via `EscapeEnvelope` (`eval/escape_contract.rs`).

### 📚 Standard Library (`@std`)
- `@std/primitives/{units,math}.hw`
- `@std/layout/{placement,via,passives,pcb}.hw`
- `@std/pdk/sky130/{rules,devices,nmos,pmos,tap,strap,pad,passives}.hw` — parametric PCells (`sky130_nmos` W=1.0um/L=150nm, `sky130_pmos` W=2.0um, `sky130_tap`, `sky130_res_high_po`, `sky130_cap_mim`) and `SKY130_RULES` (17-field rule set).

### 🖥️ Toolchain
- New CLI subcommands: `hwc run` (pure comptime compute, <5ms), `hwc eval "<expr>"` (quick expression evaluator), `hwc test` (layout synthesis testbenches + assertions).
- `hwc build` now runs `hwc-eval` → `EntityGraph` → DOPHR → exports; `hwc check` supports `--foundry` validation; `hwc doc` for in-tree documentation.

### ⚡ Performance (vs v0.2.x, 20k components)
- Comptime: 650 ms → 3.2 ms (~203×).
- Allocations: ~400k → ~4.
- RAM: 65 MB → 1.8 MB.
- Cold build: 21.6 s → 1.26 s.
- DOPHR: 5k-gate block <2s; 2M-gate SoC 12–35 min; 25M-gate die 1.5–3.5 hr.

### 🐛 Refactors
- Purged `placement_loop.rs`, `relational_resolver.rs`, `auto_via_inserter/`, `parametric_unroller/`, voxel grid, `sdf_router.rs`, `heuristic.rs`.
- Unidirectional crate dependencies: `hwc-cli` → `hwc-compiler` → {`hwc-parser`, `hwc-engine`} → {`hwc-physics`, `hwc-export`}.

---

## [0.2.2] — (Superseded by v0.3.0)

The v0.2.x planning milestone (auto-routing refinements, HPM registry, LSP) was subsumed into the v0.3.0 generative-language effort and the v0.3.1+ near-term roadmap. The v0.2.1 compiler remains the last of the declarative-markup line.

---

## [0.1.7]

- **Analytic Trace Representation**: Shifted the trace representation to continuous mathematical lines/segments (`AnalyticTrace`), bypassing slow voxel-crawling and lowering memory requirements.
- **Cylindrical Via Representation**: Added plated-through-holes (PTH), circular via-drills, and annular rings ("donuts") according to profile rules.
- **Advanced Material Optics**: Added PBR parameters (`ior`, subsurface scattering, `clearcoat_roughness`, and anisotropy) to simulate realistic fiberglass weave (FR4) and polished coatings.
- **Pin Stitching**: Automatically connects footprint pins to internal routing layers.
- **Minkowski Obstacle Inflation**: Generates 2.5D obstacle AABBs on the fly to accelerate path collision checks.
- **Quadrant Partition Triangulation**: Prevents starburst rendering anomalies and normal inversion in circular mesh exports.
- **Syntax Stabilization**: Hardware Script syntax achieved partial stabilization.
- **Hardware-Script-Driven Testing**: Testing pipeline driven primarily by Hardware Script (`.hw`) test files.

---

## [0.1.6]

- **Syntax Unification**: Removed `define` keyword; adopted single `=` for assignment and comparison; explicit keywords (`and`, `or`, `not`, `xor`); universal list parser `[]`.
- **Assembly Layout Features**: Relative placement for pours; parametric loop unrolling; geometric via linker; component bounding-box obstacles; pin-to-net binding; spatial topological sorting.
- **Safety & Integrity Guards**: Commit Gate; substrate surface detection; implicit overlap waivers; ohmic bridge system.
- **Safe Memory Concurrency**: Swapped unsafe pointers for `Arc` + `RwLock`.

---

## [0.1.5]

- **Optimized Performance Storage**: Magic Morton encoding; virtual spatial page table; bit-parallel occupancy.
- **Leap-Frog Pathfinding**: Signed Distance Fields (SDF) and Sphere Tracing.
- **Voxel Stamps**: Pre-rasterized logic cell footprints.
- **High-Speed Router Enhancements**: Deterministic tie-breaking, binary collision skips.
- **Dual Coordinate System**: Physics-grounded coordinates.
- **SoC-Scale Performance Guards**: Chunk net summary, hierarchical corridor search, incremental DRC, coarse floorplanner.

---

## [0.1.4]

- **Unified Parser & Single File Extension**: Replaced 10+ file types with single `.hw` format.
- **Two-Pass Compilation**: Symbol Table + space assembly.
- **Native SI Unit Parsing**: Lexer parses attached SI units.
- **Standard Library Prelude**: Units and constants auto-loaded.
- **Comptime Module Flattening**: Loop unrolling, `if`/`else`, index math.
- **Logic Synthesis Block (`logic:`)**: Behavioral synthesis to gates.
- **Clock Domain & CDC Tracking**; **Combinational Loop Detection**; **Progressive Alignment (LVS)**; **Parallel Domain Partitioning**.

---

## [0.1.3]

- **Tri-Fold Case Sensitivity Model**; **Unified Origin Syntax**; **Voxel Grid Core** (Morton Z-curve); **NetlistArena** (ECS-style IDs).
- **3-Phase Routing Pipeline**: Constraint Manager → Geometry Router (A*) → Design Rule Checker.
- **Custom Emitters**: GDSII, Gerber X3, OBJ/GLB, Blender scripts.
- **Trace Optimization**: Gerber D01/D03.

---

## [0.1.2]

- **Declarative Coordinate Syntax** `[x: N, y: N, z: N]`; **Coordinate System Reordering**; **Origin Point Abstraction**; **Manual Waypoint Routing** (Bresenham 3D).

---

## [0.1.0] - 2026-03-13

### 🧪 Initial MVP Beta

First working proof-of-concept of the Hardware Script compiler (Beta), initially implemented in Python.

- Space definition with dimensions and grid resolution; component placement with rotation; waypoint routing; comment support.
- Regex lexer; imperative parser; 3D tensor grid engine (NumPy); Bresenham interpolation; physics (trace resistance).
- Gerber (GTL), Blender Python, OBJ exports; YAML materials database (8 materials).
- Deterministic output (same input = same output).

---

## Version History

### v0.1 (2026-03-13) - MVP Beta Series
**Status**: 🧪 Historical (v0.1.7) — proof of concept to continuous-line physical representation.

### v0.2 (2026) - Declarative Markup Line
**Status**: 🏁 Superseded — v0.2.1 was the last declarative relational-placement release.

### v0.3 (2026-08-26) - Generative Comptime HDL
**Status**: ✅ Current Milestone — Turing-complete comptime evaluation (`hwc-eval`), 128-bit picometer arithmetic, DOPHR 3-stage routing.

---

## Breaking Changes

### v0.2.x → v0.3.0

**Syntax Changes** (breaking):
- Relational anchors (`align:`, `center_x`, `center_y`, `right_of`, …) removed.
- `spanning layer: X to Y` → `space.add_contact(from:, to:, …)`.
- `&& || !` → `and or not`; `Enum::Variant` → `Enum.Variant`.
- `resolution:`, `grid:`, `origin:`, `absolute:` removed.

**API Changes**:
- `program_to_space` replaced by `evaluate_program` + `SpaceEmitter`.
- Geometry now emitted via `space.add_polygon` / `space.add_contact` / `space.add_device` (VM `Emit*` opcodes).

**File Format**:
- `.hw` files require migration to the brace-delimited comptime grammar.
- `hw.lock` / `.hsx` binary format retained.

---

## License

**AGPLv3** - Free for open-source use, commercial licensing available for proprietary use.

**Dual Licensing Model**:
- Open source projects: Free under AGPLv3
- Commercial/proprietary use: Separate commercial license required

See [LICENSE](LICENSE) for full details.

---

## Performance

### Rust Compiler Benchmarks (v0.3.0)

| Workload | Comptime | RAM | Notes |
|----------|----------|-----|-------|
| Pure compute script | < 5 ms | < 2 MB | `hwc run` / `hwc eval` (zero meshing) |
| 20k-component board | 1.26 s | 1.8 MB | vs 21.6 s / 65 MB in v0.2.x (~203× faster comptime) |
| 5k-gate block (DOPHR) | < 2 s | — | global → panel → detailed |
| 2M-gate SoC (DOPHR) | 12–35 min | — | negotiated-congestion routing |
| 25M-gate die (DOPHR) | 1.5–3.5 hr | — | 21.0 MB tensor for 500×500×6 G-cells |

**Hardware**: Standard development machine
**Compiler**: Rust (Cargo Workspace)
**Testing Framework**: Direct Hardware Script execution via workspace integration tests

---

## Contributors

### v0.3.0 Development Team

- Comptime evaluation engine (`hwc-eval`) & bytecode VM
- DOPHR 3-stage router
- Materials database & `@std` PDK library
- Documentation and examples
- Testing and verification

---

## Acknowledgments

### Data Sources
- **Materials Project** - Material properties (mp-XXXXXXX IDs)
- **Engineering Handbooks** - Physical constants
- **Manufacturer Datasheets** - Component specifications

### Tools
- **Rust** - Main compiler implementation language (`logos`, `miette`, `rayon`, `rkyv`, `clipper2`, `clarabel`)
- **Babylon.js Sandbox** - 3D model (GLB/OBJ) verification
- **LibreCAD** - 2D CAD and DXF verification
- **Gerbv Viewer** - Gerber and drill file verification
- **Python / NumPy / PyYAML** - Initial MVP prototyping (v0.1.0)

---

## Links

- **Documentation**: [Docs/v0.3.0/INDEX.md](Docs/v0.3.0/INDEX.md)
- **Getting Started**: [Docs/v0.3.0/Canonical-Language-Grammar.md](Docs/v0.3.0/Canonical-Language-Grammar.md)
- **Comptime Engine**: [Docs/v0.3.0/Comptime-Evaluation-Engine.md](Docs/v0.3.0/Comptime-Evaluation-Engine.md)
- **Architecture**: [Docs/v0.3.0/Authoritative-Architecture-Reference.md](Docs/v0.3.0/Authoritative-Architecture-Reference.md)
- **Routing**: [Docs/v0.3.0/3-Stage-Guided-Routing-Specification.md](Docs/v0.3.0/3-Stage-Guided-Routing-Specification.md)
- **Compiler CLI**: [hwc/crates/hwc-cli](hwc/crates/hwc-cli)
- **Parser & Lexer**: [hwc/crates/hwc-parser](hwc/crates/hwc-parser)
- **Integration Tests (Hardware Script)**: [tests](tests)

---

**Format**: Based on [Keep a Changelog](https://keepachangelog.com/)
**Versioning**: [Semantic Versioning](https://semver.org/)
