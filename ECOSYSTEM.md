# Hardware Script — Ecosystem Architecture

**Unified Hardware Engineering Toolchain & Standard (v0.3.0)**

---

## Overview

Hardware Script is a complete toolchain and formal language standard for text-based, **generative** physical hardware synthesis. Inspired by modern software engineering paradigms (Rust, Cargo, TypeScript), Hardware Script unifies logical schematics, physical layer layout, material properties, PDK stackups, compile-time generative logic, physics verification, and manufacturing export into a deterministic, Git-friendly workflow. In v0.3.0 the language is **Turing-complete at compile time**: layout is computed by a Linear Bytecode Virtual Machine (`hwc-eval`) before any synthesis or routing begins.

---

## 1. Universal Hardware Scripting Language (UHWSL)

**UHWSL** is the formal specification defining the syntax, grammar, and physical database semantics of `.hw` source code.

*   **Formal Specification**: UHWSL v0.3.0 (Turing-Complete Comptime Generative HDL, Picometer-Precision Vector Database)
*   **Source File Extension**: `.hw`
*   **Primary Execution Engine**: `hwc` (Rust Compiler Workspace) — `hwc-eval` Bytecode VM + `hwc-engine` synthesis

---

## 2. Unified 3-File Architecture

Hardware Script eliminates file-format fragmentation by consolidating design input into three canonical files, plus a single compiled binary exchange format:

| File | Purpose | Description |
|------|---------|-------------|
| `hw.toml` | Project Manifest | Declares project metadata, manufacturing targets, active PDK profiles, and HPM package dependencies. |
| `hw.lock` | Version & Route Lockfile | Cryptographically hashes dependencies and caches physical route geometries using `rkyv` zero-copy binary serialization for sub-millisecond incremental builds. |
| `.hw` | Universal Source | Human-readable source defining materials, profiles, devices, subcircuits, modules, structs, enums, comptime `fn`, spaces, and testbenches. |
| `.hsx` | Compiled Exchange Binary | Binary payload emitted by `hwc` containing 2D vector shapes, 3D meshes, logical netlist graphs, and diagnostic states for `hsm` hot-reload. |

---

### `.hw` — Universal Source Declarations

In Hardware Script, there are no proprietary binary files for footprints, schematics, or stackups. Everything is authored as clean, top-level blocks inside `.hw` source files:

#### 1. Logical Layer (Electrical Contracts)
*   `module`: Abstract electrical netlist contracts with pin interfaces and logical routes.
*   `subcircuit`: PDK-provided SPICE equivalent circuits (e.g. SKY130 resistor or transistor models).

#### 2. Physical Layer (Absolute Reality)
*   `space`: The concrete 2D/3D physical layout, implementing a `module`.
*   `device`: Multi-terminal semiconductor component definitions (resistors, capacitors, MOSFETs) with terminal-to-material contracts.
*   `pour`/`contact` geometry: emitted at comptime via `space.add_polygon` / `space.add_contact` / `space.add_device` (compiled to `EmitPolygon`/`EmitContact`/`EmitDevice` VM opcodes).
*   `route`: Interconnect traces routed by the DOPHR engine.
*   `material`: Atomic, electrical, thermal, and optical properties of physical elements and alloys.
*   `profile`: Stackup layer definitions, PDK clearance constraints, trace widths, and via rules.

#### 3. Generative Layer (Compile-Time Computation)
*   `fn`: Turing-complete functions — parametric PCells, via arrays, layout generators (typed params, named args, defaults).
*   `struct` / `enum`: User-defined value types for layout metadata.
*   `let` / `for` / `if` / `match` / `while`: Control flow evaluated by the `hwc-eval` VM.

#### 4. Verification & Testing
*   `test`: SPICE testbench declarations (DC operating point, AC frequency response, transient analysis).
*   `assert`: Compile-time assertions executed by the comptime VM.
*   `region`: Placement floorplanning partitions and DRC parallelization zones.

---

### Reusable Sub-Assembly Libraries (`.hw` Packages)

Hardware Script allows entire routed sub-circuits (e.g. voltage regulators, RF front-ends, sensor modules) to be packaged as reusable `.hw` libraries — including the generative functions that stamp their geometry.

#### Example: Power Regulator Library (`@power/5v-regulator.hw`)

```hw
import * from @std/primitives/units

export module Regulator5V {
    pins: [input VIN, output VOUT, ground GND]
}

export fn build_regulator(name: String, at: Point2D) -> Regulator5V_Space {
    # comptime generator stamps the converter geometry deterministically
    space.add_polygon(layer: "metal1", rect: [at.x, at.y, 10mm, 10mm], net: VOUT, name: name)
    # ... additional pours, contacts, and routes
}
```

#### Usage in System Layout (`main.hw`)

```hw
import Regulator5V from "@power/5v-regulator"

space MainBoard {
    dimensions: [100mm, 100mm]
    profile: StandardPCB

    # Instantiate reusable sub-assembly
    add Regulator5V_Space named PowerSupply at [x: 50mm, y: 50mm]
}
```

---

## 3. The Core Toolchain Components

```
                      ┌───────────────────────────────┐
                      │          UHWSL (.hw)          │
                      │   Universal Hardware Source   │
                      └───────────────┬───────────────┘
                                      │
                                      ▼
                      ┌───────────────────────────────┐
                      │          hwc Compiler         │
                      │    (Rust Multi-Crate Engine)  │
                      │  ┌─────────────────────────┐  │
                      │  │  hwc-eval (Bytecode VM) │  │
                      │  └─────────────────────────┘  │
                      │  ┌─────────────────────────┐  │
                      │  │  hwc-engine (DOPHR +    │  │
                      │  │   EntityGraph, physics)  │  │
                      │  └─────────────────────────┘  │
                      └───────────────┬───────────────┘
                                      │
                       ┌──────────────┴──────────────┐
                       │                             │
                       ▼                             ▼
              ┌─────────────────┐           ┌─────────────────┐
              │       hsm       │           │       hpm       │
              │  Live Monitor   │           │ Package Manager │
              │ (Tauri / 3D/2D) │           │ (Soft IP / PDKs)│
              └─────────────────┘           └─────────────────┘
```

---

### 3.1 `hwc` — Core Compiler

The **Hardware Script Compiler** (`hwc`) is written entirely in high-performance Rust. It is a two-phase system:

1. **`hwc-eval` — Comptime Evaluation Engine**: a Linear Bytecode Virtual Machine (`eval/vm.rs`) that compiles functions/spaces into `Chunk` bytecode and emits geometry into the `EntityGraph` using 128-bit picometer arithmetic, under a hermetic sandbox.
2. **`hwc-engine` — Synthesis & Routing**: builds the `EntityGraph`, runs the DOPHR 3-stage router, and performs physics checks before export.

Like professional software compilers (`rustc`, `gcc`), `hwc` provides detailed diagnostic error feedback with source line snippets, exact physical measurement deltas, and fix guidance directly to terminal stdout or stderr.

#### Sub-Crates
*   **`hwc-cli`**: Command-line interface and target orchestration (`build`, `run`, `eval`, `test`, `check`, `drc`, `physics`, `simulate`, `doc`, `init`, `materials`).
*   **`hwc-parser`**: Logos DFA lexer and Pratt parser (8-level precedence) with inline SI unit tokenization (`1.41um`, `400nm`, `1.8V`) and arena AST.
*   **`hwc-compiler`**: `hwc-eval` (value, vm, opcodes, compiler, builtins, emitter, sandbox), module resolver, symbol table, and the v0.3.0 pipeline bridge (`pipeline/`).
*   **`hwc-engine`**: `EntityGraph`, 128-bit picometer vector database, DOPHR 3-stage router (`routing/dophr.rs`), and `clarabel` / DAG legalization engine.
*   **`hwc-physics`**: DRC, LVS, PIVB connectivity, Wheeler–Sakurai BEM parasitic extraction, crosstalk, electromigration, and IPC-2152 thermal checks.
*   **`hwc-export`**: Multi-format emitters for Gerber X3, Excellon drill, GDSII, SPICE netlists (`circuit.sp`, `dc.sp`, `ac.sp`, `tran.sp`), DXF 2D drawings, GLB 3D meshes, and BOM CSV files.

#### CLI Commands

```bash
# Compile design to binary exchange format (.hsx) + manufacturing outputs
hwc build main.hw

# Pure comptime compute (no meshing, <5ms)
hwc run main.hw
hwc eval "4.0um / 1.41um * 350.0"

# Check syntax and physics rules without generating outputs
hwc check main.hw

# Run layout synthesis testbenches and assertions
hwc test main.hw

# Run design rule check (DRC) on build outputs
hwc drc main.hw

# Run physics validation pass (EM, thermal, parasitic extraction)
hwc physics main.hw

# Run circuit simulation
hwc simulate main.hw

# Export production manufacturing targets
hwc build main.hw --target pcb     # Gerber X3, Excellon drill, DXF outline
hwc build main.hw --target spice   # SPICE netlist suite (circuit.sp, dc.sp, ac.sp, tran.sp)
hwc build main.hw --target gds     # Silicon IC GDSII stream
hwc build main.hw --target viz     # 3D assets (.glb, .obj)

# Inspect binary lockfile
hwc lock inspect build/main.hsx
```

---

### 3.2 `hsm` — Hardware Script Monitor

`hsm` is the companion GUI application for real-time design inspection and live hot-reloading (under 50ms refresh upon recompilation).

#### Technical Stack & Viewports
*   **App Shell**: Tauri v2 + SolidJS
*   **3D Viewport**: Babylon.js renderer with physically-based rendering (PBR) for board materials, vias, trace depth, and component geometry.
*   **2D Vector Viewport**: PixiJS WebWorker pipeline utilizing `OffscreenCanvas` for smooth 60 FPS pan/zoom over 1M+ trace segments.
*   **DXF & Mechanical Viewport**: Three.js orthographic renderer for mechanical boundary verification.
*   **SPICE Waveform Viewer**: `uPlot` engine capable of rendering 10M-point transient and AC frequency response waveforms in under 1ms.

---

### 3.3 `hpm` — Hardware Package Manager

`hpm` manages component footprints, Soft IP sub-assemblies, PDK profiles, and material libraries.

#### Key Workflows

```bash
# Initialize project workspace (creates hw.toml, main.hw)
hpm init

# Install discrete component packages
hpm install passive/resistor_0805
hpm install ics/esp32_c3

# Install Soft IP sub-assembly libraries
hpm install @power/5v-regulator
hpm install @rf/balun_filter

# Search community registry
hpm search regulator

# Publish package to registry
hpm publish @power/5v-regulator

# Update project dependencies
hpm update
```

#### Package Registry Infrastructure
`hpm` uses a lightweight, Git-backed registry model (similar to Homebrew or Cargo index), hosting package manifests on GitHub with zero infrastructure overhead.

---

## 4. Ecosystem Tooling Comparison

Hardware Script provides a unified toolchain equivalent to modern software stacks:

| Domain | Rust | TypeScript / Node.js | Hardware Script |
|--------|------|----------------------|-----------------|
| **Specification** | Rust Reference | ECMAScript | UHWSL |
| **Source File** | `.rs` | `.ts` / `.js` | `.hw` |
| **Compiler** | `rustc` | `tsc` | `hwc` (+ `hwc-eval` Bytecode VM) |
| **Package Manager** | `cargo` | `npm` / `pnpm` | `hpm` |
| **Live Monitor** | — | Vite / HMR | `hsm` |
| **Lockfile** | `Cargo.lock` | `package-lock.json` | `hw.lock` (`.hsx`) |

---

## Summary

The Hardware Script ecosystem delivers a deterministic, software-grade environment for hardware design:

*   **`UHWSL`**: The formal picometer-vector, Turing-complete comptime language specification.
*   **`hw.toml` / `hw.lock` / `.hw`**: The unified 3-File Architecture.
*   **`.hsx`**: Zero-copy binary exchange payload.
*   **`hwc`**: Rust compiler workspace with built-in `hwc-eval` Bytecode VM and DOPHR 3-stage router.
*   **`hsm`**: 50ms live preview monitor with 2D, 3D, and SPICE viewports.
*   **`hpm`**: Git-backed hardware package manager.
