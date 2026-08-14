# Hardware Script

**Text-Based Hardware Design Language**

[![Status](https://img.shields.io/badge/status-v0.2.1-orange)]()
[![Rust](https://img.shields.io/badge/compiler-Rust-orange)]()
[![License](https://img.shields.io/badge/license-AGPLv3-blue)]()

---

## What is Hardware Script?

Hardware Script (`.hw`) is a text-based hardware design language that compiles to industry-standard formats. The goal is to design PCBs, silicon chips, and electronic systems from human-readable, Git-friendly text.

**The Vision**: Bring the npm/software workflow to hardware. Write hardware like code, compile it deterministically, and manufacture real boards from a single source of truth.

**Current Reality (v0.2.1)**: Powered by an AST Arena database-driven compiler, `hwc` supports high-performance physical synthesis, topological obstacle-aware routing, via depth & array controls, Clippy-level error intelligence, range syntax, device definitions, BOM export, and comprehensive physics checks (DRC, LVS, crosstalk, thermal/EM, parasitic extraction).

```hw
space Simple_Resistor_Space implements SimpleResistor:
    dimensions: 20.0um by 10.0um
    profile: Resistor_3D

    # 1. Single Anchor: Resistor Body
    add pour(Polysilicon) named Resistor_Body on layer: polyres:
        device: R1.A, R1.B
        dimensions: 4.0um by 1.41um
        at: [x: 10.0um, y: 5.0um]

    # 2. Relational Contact (Anchored to Body Edge)
    add pour(Titanium_Silicide) named Contact_A_LI on layer: li1:
        device: R1.A
        net: In
        dimensions: 400nm by 1.41um
        align: center_x with Resistor_Body.left + 200nm
        align: center_y with Resistor_Body

    # 3. Comptime Via Array Loop (polyres → li1)
    for i in 0..3:
        add contact(Tungsten) named Via_A_Poly_{i} spanning layer: polyres to li1:
            diameter: 170nm
            align: center_x with Contact_A_LI
            align: center_y with Contact_A_LI.center_y + (i - 1) * 400nm
            net: In

# SPICE Testbench (Auto-generates circuit.sp, dc.sp, ac.sp, tran.sp)
test Simple_Resistor_AC_Test for Simple_Resistor_Space:
    ac: { sweep: dec, points: 20, freq: 100Hz..100MHz }
    tran: { step: 10ps, stop: 200ns }
```

**Compiles to**:
- ✅ SPICE netlist (`.sp`) — Analog simulation & node verification
- ✅ BOM (Bill of Materials) (`.csv`) — Manufacturer part numbers, pricing, tolerances
- ✅ GLB / OBJ — 3D scene geometry visualization
- ✅ DXF (2D CAD) — Board outlines & mechanical boundaries
- ✅ Gerber X3 & Excellon — Industry manufacturing packages
- ✅ GDSII — Silicon IC layout output

| 2D Layout View (PixiJS Engine) | 3D Rendered View (Babylon.js Engine) |
| :---: | :---: |
| ![2D Resistor View](assets/resistor_2d_view.png) | ![3D Resistor View](assets/resistor_3d_view.png) |

> 💡 **Full Example**: View the complete source file [`tests/Resistor-Basics/simple_resistor_test.hw`](tests/Resistor-Basics/simple_resistor_test.hw)


---

## Why Hardware Script?

### The Problem

Traditional EDA tools (KiCad, Altium, Eagle) were built for clicking GUIs. This creates several problems:

1. **Poor version control** — Binary and XML files don't diff or merge well in Git.
2. **No programmatic access** — You can't script, template, or parameterize a GUI.
3. **Slow iteration** — Manual placement and routing takes hours.
4. **Tool lock-in** — Binary formats make sharing and collaboration difficult.

### The Solution

Hardware Script treats hardware like software:

- ✅ **Plain text** — Write your design by hand, just like any source file.
- ✅ **Deterministic compilation** — Same input = same output, every time.
- ✅ **Physics validation** — Catch electrical errors at compile time.
- ✅ **Multi-format export** — Gerber, DXF, GLB, SPICE from one source.
- ✅ **Package ecosystem** — Reusable components like npm packages.
- ✅ **Optional LLM assistance** — Because designs are plain text, you can paste a `.hw` file into any LLM and ask it to generate or modify hardware for you.

---

## Quick Start

### Create a Design

Create `my_board.hw`:

```hw
space FirstBoard:
    dimensions: 20mm by 20mm by 2.0mm
    resolution: 1nm
    origin: tl by t

    route A to B:
        path:
            - [x: 5mm, y: 5mm, layer: l1]
            - [x: 15mm, y: 5mm, layer: l1]
            - [x: 15mm, y: 15mm, layer: l1]
```

### Compile

```bash
hwc build my_board.hw
```

### Preview Live

```bash
hsm build/my_board.hsx
```

**Hardware Script Monitor** (`hsm`) opens and hot-reloads your board in under 50ms whenever you recompile.

### Generate Manufacturing Files

```bash
hwc build my_board.hw --target pcb   # Gerber + drill files
hwc build my_board.hw --target viz   # OBJ + GLB 3D models
hwc build my_board.hw --target spice # SPICE netlist
```

---

## The Vision: npm for Hardware

Imagine if hardware development worked like software:

```bash
# Install a component package
hpm install @power/5v-regulator

# Use it in your design
import Regulator5V from "@power/5v-regulator"
```

```hw
space MyRobot:
    dimensions: 100mm by 100mm by 2.0mm
    resolution: 1nm
    origin: tl by t

    add Regulator5V named PowerSupply at [x: 50mm, y: 50mm] on layer: l1

    route Battery.Plus to PowerSupply.VIN
    route PowerSupply.VOUT to ESP32.VIN
```

**That's where we are in v0.2.1.** The AST Arena database-driven architecture powers full physical synthesis and advanced routing engine features.

### The "Matrix Moment"

Hardware Script **v0.2.1** uses an **AST Arena database-driven architecture** with picometer precision. This architectural evolution unlocks capabilities impossible in traditional tools:

- **AST Arena Database** — Zero-copy interning, arena-allocated nodes, and instant incremental query execution
- **Picometer-precision database** — All coordinates stored as 64-bit integer picometers (1pm = 10⁻¹² m)
- **Scale invariance** — Same tool for PCBs (millimeters) and silicon chips (nanometers)
- **Deterministic compilation** — Same input always produces bit-identical output
- **Zero-stamping scene graph** — Components stored once, instances as lightweight transforms
- **Plain-text source** — Git-friendly, LLM-readable, human-editable

**Read the full vision**: [VISION.md](VISION.md)

---

## Features

### ✅ Core Compiler (v0.2.1)

**Syntax & Language (UHWSL v0.2.1):**
- **Text-based design** — Write hardware like code in `.hw` files
- **Unified 3-File Architecture** — `hw.toml`, `hw.lock`, and `.hw` source
- **Range Syntax** — Indexing and vector slicing for signals and buses (`bus[0..7]`, `pin[1..4]`)
- **Device Definitions** — Dedicated `device` keyword for multi-gate ICs and precise pin bindings
- **Multi-Line Declarations** — Clean block definitions for modules, spaces, and components
- **Export Control** — Explicit symbol export using `export module` and `export component`
- **Native SI unit parsing** — `254µm`, `4.7kΩ`, `100nF` parsed directly in lexer

**Compilation Pipeline:**
- **AST Arena & Query Engine** — High-performance arena allocation with Salsa-inspired incremental queries
- **Symbol table & Relational Placement** — Relative layout positioning (`named B at 5mm right of A`)
- **Logical netlist synthesis** — `NetlistArena` module-to-schematic extraction
- **Device binding validation** — Physical layout matches logical schematic (LVS)
- **Physical continuity checking** — Verifies all nets are connected (no floating islands)
- **Clippy-Level Error Intelligence** — `hwsd`-powered diagnostic engine with inline context snippets, fix hints, and JSON output mode for AI agents

**Routing & Physical Synthesis:**
- **Topological Obstacle-Aware Router** — Axis-Aligned Slab Method with connection interface routing
- **Via Array & Depth Controls** — Multi-via arrays for high current and explicit blind/buried via depth limits across stackup layers
- **Trace geometry** — Dynamic width, clearance, and spacing rule enforcement
- **Pour support** — Polygon copper pours with thermal relief boundary definitions

**Physics & Electrical Validation:**
- **DRC & LVS** — Full physical rule checking and layout-versus-schematic verification
- **Crosstalk Analysis** — Analytical parallel trace coupling and interference bounds
- **Electromigration & Thermal** — Trace current-density checks against thermal limits
- **RF Parasitics** — Wheeler-Sakurai BEM parasitic extraction (R/C/L/M)

**Export Formats:**
- **SPICE netlist (`.sp`)** — Full circuit netlist output
- **BOM (`.csv`)** — Extended Bill of Materials with manufacturer part numbers, pricing, and tolerances
- **Gerber X3 & Excellon** — Production-ready copper layers, silkscreen, solder mask, drill files
- **GLB / OBJ** — 3D scene model export
- **DXF 2D drawings** — Mechanical CAD outlines
- **GDSII** — Silicon foundry IC format

### 🔄 Active Development (v0.2.2+)

- **Automatic BGA escape routing**
- **Public HPM package registry deployment**
- **Language Server Protocol (LSP) for VS Code**
- **Live monitor (`hsm`) enhancements** — Babylon.js hot-reload performance improvements

---

## The Unified File Architecture

Hardware Script uses exactly **3 file extensions**:

| File | Purpose |
|------|---------|
| `hw.toml` | Project manifest — metadata, targets, dependencies |
| `hw.lock` | Lockfile — reproducible builds, hashed dependencies |
| `.hw` | Universal source — materials, profiles, components, modules, spaces, tests |

The compiler produces a **compiled exchange binary** (`.hsx`) which the live monitor (`hsm`) watches and hot-reloads.

---

## Project Structure

```
├── hwc/                     # Rust compiler workspace
│   ├── Cargo.toml
│   ├── crates/
│   │   ├── hwc-cli/         # Command-line interface
│   │   ├── hwc-parser/      # Lexer + AST parser
│   │   ├── hwc-compiler/    # Two-pass compiler
│   │   ├── hwc-engine/      # Voxel grid + routing engine
│   │   ├── hwc-physics/     # DRC, LVS, thermal, electrical
│   │   ├── hwc-export/      # Gerber, GDSII, DXF, GLB emitters
│   │   ├── hwc-materials/   # Materials database
│   │   └── hwc-stdlib/      # Standard library prelude
│   ├── stdlib/              # .hw standard library files
│   └── tests/               # Integration tests (written in Hardware Script)
```

---

## Technical Stack

- **Compiler**: Rust (`logos`, `miette`, `rayon`, `rustc-hash`, `compact_str`, `smallvec`)
- **Live Monitor**: Tauri v2 + SolidJS + Babylon.js + PixiJS + uPlot + `dxf-viewer`
- **Testing**: Hardware Script `.hw` integration test files
- **Viewing**: Babylon.js Sandbox (3D), LibreCAD (DXF), Gerbv Viewer (Gerber/Drill)

---


## Roadmap: The 5 Critical Problems

### 1️⃣ Hardware Description Language ✅ (v0.1.x)

A clean, text-based, unified language for describing hardware at any scale.

```hw
space Board:
    dimensions: 20mm by 20mm by 2.0mm
    grid: 200 by 200 by 4
```

### 2️⃣ Component Knowledge Database 🔄 (v0.2)

A universal component library with electrical limits, pins, footprints, and 3D meshes.

**Strategy**: GitHub-based registry (like Homebrew or Go modules).

### 3️⃣ Physics/Electrical Validation ✅ (v0.1.x)

Compiler-level physics validation with structured errors and fix hints.

```
Error E0042: Voltage mismatch
  Expected: 3.3V  Got: 5V
  Hint: Insert a 3.3V LDO between Battery.Out and ESP32.VIN
  Install: hpm install power/ldo_3v3
```

### 4️⃣ Integrated Toolchain ✅ (v0.1.x)

Single pipeline from source to manufacturing-ready outputs.

```bash
hwc check board.hw      # Validate
hwc build board.hw      # Compile to .hsx
hsm build/board.hsx     # Live preview
hwc build --target pcb  # Manufacturing files
```

### 5️⃣ Parametric Hardware Modules 🔄 (v0.2+)

The holy grail: reusable hardware components with parameters.

```hw
add BuckConverter (input: 12V, output: 5V, current: 2A) named Converter1
```

**Read the full roadmap**: [ROADMAP.md](ROADMAP.md)



## License & Business Model

### Open Source (AGPLv3)

Hardware Script is **free and open source** under the **GNU AGPLv3** license.

**You own your hardware designs.** We own the compiler. Think of it like Microsoft Word: Microsoft owns Word, but you own the documents you create with it.

### When You Need a Commercial License

You only need a Commercial License in these specific cases:

- ✅ **Modifying the Compiler** — You change `hwc` source code and want to keep changes private.
- ✅ **Hosting as a Service** — You run the compiler on a cloud server accessible via web/API.
- ✅ **Enterprise Support** — You need dedicated support and SLA guarantees.
- ✅ **Corporate AGPL Ban** — Your company's legal team prohibits AGPL software.

**See full details**: [LICENSE-FAQ.md](LICENSE-FAQ.md) | [COMMERCIAL-LICENSE.md](COMMERCIAL-LICENSE.md)

---

## Contributing

We welcome contributions! This is an open-source project with a clear roadmap.

### Priority Areas

1. **Component Library** — Define standard parts in `.hw` format.
2. **Integration Tests** — Write tests in Hardware Script.
3. **Export Formats** — Drill files, silkscreen, solder mask.
4. **Documentation** — Examples, tutorials, and use cases.

### How to Contribute

Hardware Script follows **The Lean Core Philosophy** — the compiler stays as lightweight and fast as possible. We operate on an **Issue-Driven Development** model. We do **not** accept Pull Requests for the core compiler code (`hwc`). This isn't about declining contributions — it's about ensuring **rigorous research and alignment** with our core philosophy before implementing. When you share an idea via Issue, we research it thoroughly (benchmarks, papers, edge cases) before writing the code.

1. **Found a bug?** Open a [Bug Report Issue](../../issues/new?template=bug_report.md).
2. **Have an optimization idea?** Open an [Issue](../../issues/new?template=optimization.md) — we'll research it and implement it if it aligns with our vision.
3. **Want to write hardware code?** Build an [HPM package](../../issues/new?template=syntax_proposal.md) and publish it to the registry!
4. **Want to fix a typo in the docs?** Submit a PR for the `Docs/` folder.

Read [CONTRIBUTING.md](CONTRIBUTING.md) for the full guide.

---

## Community

- **GitHub**: https://github.com/HardwareScript
- **Discord**: https://discord.gg/9zqH8nuCet
- **Twitter**: @hwsl_lang
- **Email**: hardwarescript@gmail.com

---

## Links

- **🔮 Vision**: [VISION.md](VISION.md) — The "Matrix moment" and where we're going
- **🌐 Ecosystem**: [ECOSYSTEM.md](ECOSYSTEM.md) — The complete toolchain (`hwc`, `hpm`, `hsm`, `hwsd`)
- **🗺️ Roadmap**: [ROADMAP.md](ROADMAP.md) — The 5-problem strategy
- **📝 Changelog**: [CHANGELOG.md](CHANGELOG.md)
- **🤝 Contributing**: [CONTRIBUTING.md](CONTRIBUTING.md)
- **⚖️ License**: [LICENSE.md](LICENSE.md) — AGPLv3 + Commercial
- **💡 Integration Tests**: [tests](tests)
- **🔧 Compiler CLI**: [hwc/crates/hwc-cli](hwc/crates/hwc-cli)

---

> "We proved that hardware design can be as simple as writing text."

> "Same input, same output, every time. Hardware is now deterministic."

> "From a `.hw` file to Gerber, GLB, SPICE, and DXF in milliseconds."

---

**Hardware Script v0.2.1** — Making hardware design as simple as writing code.
