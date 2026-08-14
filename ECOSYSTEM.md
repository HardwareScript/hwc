# Hardware Script — Ecosystem Architecture

**Unified Hardware Engineering Toolchain & Standard (v0.2.1)**

---

## Overview

Hardware Script is a complete toolchain and formal language standard for text-based physical hardware synthesis. Inspired by modern software engineering paradigms (Rust, Cargo, TypeScript), Hardware Script unifies logical schematics, physical layer layout, material properties, PDK stackups, physics verification, and manufacturing export into a deterministic, Git-friendly workflow.

---

## 1. Universal Hardware Scripting Language (UHWSL)

**UHWSL** is the formal specification defining the syntax, grammar, and physical database semantics of `.hw` source code.

*   **Formal Specification**: UHWSL v0.2.1 (Picometer-Precision Vector Database Specification)
*   **Source File Extension**: `.hw`
*   **Primary Execution Engine**: `hwc` (Rust Compiler Workspace)

---

## 2. Unified 3-File Architecture

Hardware Script eliminates file-format fragmentation by consolidating design input into three canonical files, plus a single compiled binary exchange format:

| File | Purpose | Description |
|------|---------|-------------|
| `hw.toml` | Project Manifest | Declares project metadata, manufacturing targets, active PDK profiles, and HPM package dependencies. |
| `hw.lock` | Version & Route Lockfile | Cryptographically hashes dependencies and caches physical route geometries using `rkyv` zero-copy binary serialization for sub-millisecond incremental builds. |
| `.hw` | Universal Source | Human-readable source defining materials, profiles, devices, subcircuits, modules, spaces, and testbenches. |
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
*   `pour`: Conductor, semiconductor, or mask polygons bound to layers and nets.
*   `contact`: Layer-spanning vias with material-specific penetration depth control.
*   `route`: Interconnect traces routed by the topological line-search engine.
*   `material`: Atomic, electrical, thermal, and optical properties of physical elements and alloys.
*   `profile`: Stackup layer definitions, PDK clearance constraints, trace widths, and via rules.

#### 3. Verification & Testing
*   `test`: SPICE testbench declarations (DC operating point, AC frequency response, transient analysis).
*   `region`: Placement floorplanning partitions and DRC parallelization zones.

---

### Reusable Sub-Assembly Libraries (`.hw` Packages)

Hardware Script allows entire routed sub-circuits (e.g. voltage regulators, RF front-ends, sensor modules) to be packaged as reusable `.hw` libraries.

#### Example: Power Regulator Library (`@power/5v-regulator.hw`)

```hw
import * from @std/primitives/units

export module Regulator5V:
    pins: [input VIN, output VOUT, ground GND]
    route VIN to VOUT

export space Regulator5V_Space implements Regulator5V:
    dimensions: 10mm by 10mm
    resolution: 1nm
    profile: StandardPCB

    nets:
        VIN:  { classification: power, potential: 12.0V, current: 1.5A }
        VOUT: { classification: power, potential: 5.0V,  current: 1.5A }
        GND:  { classification: ground, potential: 0.0V,  current: 1.5A }

    # Component placements and interconnects defined parametrically
```

#### Usage in System Layout (`main.hw`)

```hw
import Regulator5V from "@power/5v-regulator"

space MainBoard:
    dimensions: 100mm by 100mm
    resolution: 1nm
    profile: StandardPCB

    # Instantiate reusable sub-assembly
    add Regulator5V_Space named PowerSupply at [x: 50mm, y: 50mm]
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

The **Hardware Script Compiler** (`hwc`) is written entirely in high-performance Rust (`logos`, `miette`, `rayon`, `rkyv`, `rstar`, `geo-index`, `clarabel`, `clipper2`).

Like professional software compilers (`rustc`, `gcc`), `hwc` provides detailed diagnostic error feedback with source line snippets, exact physical measurement deltas, and fix guidance directly to terminal stdout or stderr.

#### Sub-Crates
*   **`hwc-cli`**: Command-line interface and target orchestration.
*   **`hwc-parser`**: Logos DFA lexer and recursive AST parser with inline SI unit tokenization (`1.41um`, `400nm`, `1.8V`).
*   **`hwc-compiler`**: Two-pass semantic analyzer, symbol table manager, and Salsa-inspired incremental query engine.
*   **`hwc-engine`**: 64-bit picometer vector database, Topological Line-Search Router (Axis-Aligned Slab Method), and `clarabel` / DAG legalization engine.
*   **`hwc-physics`**: DRC, LVS, PIVB connectivity, Wheeler–Sakurai BEM parasitic extraction, crosstalk, electromigration, and IPC-2152 thermal checks.
*   **`hwc-export`**: Multi-format emitters for Gerber X3, Excellon drill, GDSII, SPICE netlists (`circuit.sp`, `dc.sp`, `ac.sp`, `tran.sp`), DXF 2D drawings, GLB 3D meshes, and BOM CSV files.

#### CLI Commands

```bash
# Compile design to binary exchange format (.hsx)
hwc build main.hw

# Check syntax and physics rules without generating outputs
hwc check main.hw

# Run Design Rule Check (DRC) on build outputs
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
| **Compiler** | `rustc` | `tsc` | `hwc` |
| **Package Manager** | `cargo` | `npm` / `pnpm` | `hpm` |
| **Live Monitor** | — | Vite / HMR | `hsm` |
| **Lockfile** | `Cargo.lock` | `package-lock.json` | `hw.lock` (`.hsx`) |

---

## Summary

The Hardware Script ecosystem delivers a deterministic, software-grade environment for hardware design:

*   **`UHWSL`**: The formal picometer vector language specification.
*   **`hw.toml` / `hw.lock` / `.hw`**: The unified 3-File Architecture.
*   **`.hsx`**: Zero-copy binary exchange payload.
*   **`hwc`**: Rust compiler workspace with built-in physics engines.
*   **`hsm`**: 50ms live preview monitor with 2D, 3D, and SPICE viewports.
*   **`hpm`**: Git-backed hardware package manager.
