# Hardware Script

**Text-Based Hardware Design Language**

[![Status](https://img.shields.io/badge/status-Alpha%20v0.1.8-orange)]()
[![Rust](https://img.shields.io/badge/compiler-Rust-orange)]()
[![License](https://img.shields.io/badge/license-AGPLv3-blue)]()

---

## What is Hardware Script?

Hardware Script (`.hw`) is an experimental text-based hardware design language that compiles to industry-standard formats. The goal is to design PCBs, silicon chips, and electronic systems from human-readable, Git-friendly text.

**The Vision**: Bring the npm/software workflow to hardware. Write hardware like code, compile it deterministically, and manufacture real boards from a single source of truth.

**Current Reality (v0.1.8-alpha)**: The compiler successfully handles single-layer designs and basic ASIC layouts. Multi-layer routing with automatic via insertion is under active development.

```hw
space MyBoard:
    dimensions: 20mm by 20mm by 2.0mm
    grid: 200 by 200 by 4
    profile: JLCPCB_2Layer
    origin: tl by t

    add Transistor_NPN named Switch at [x: 5mm, y: 5mm, layer: l1]

    route Switch.Collector to Power.Out:
        path:
            - [x: 5mm, y: 6mm, layer: l1]
            - [x: 15mm, y: 6mm, layer: l1]
            - [x: 15mm, y: 15mm, layer: l1]
```

**Compiles to** (with limitations):
- ✅ SPICE netlist (analog simulation) — Fully functional
- ✅ BOM (Bill of Materials) — CSV format
- ✅ GLB (3D models) — Basic geometry visualization
- ✅ DXF (2D CAD) — Board outlines


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

**That's where we're headed.** v0.1.8-alpha has the foundation working with active development on advanced routing features.

### The "Matrix Moment"

Hardware Script v0.1.8 is transitioning to a **vector-first continuous coordinate architecture** with picometer precision. This architectural evolution unlocks capabilities impossible in traditional tools:

- **Picometer-precision database** — All coordinates stored as 64-bit integer picometers (1pm = 10⁻¹² m)
- **Scale invariance** — Same tool for PCBs (millimeters) and silicon chips (nanometers)
- **Deterministic compilation** — Same input always produces identical output
- **Zero-stamping scene graph** — Components stored once, instances as lightweight transforms
- **Plain-text source** — Git-friendly, LLM-readable, human-editable

**Read the full vision**: [VISION.md](VISION.md)

---

## Features

### ✅ Core Compiler (v0.1.8-alpha)

**Syntax & Language:**
- **Text-based design** — Write hardware like code in `.hw` files
- **Unified 3-File Architecture** — `hw.toml`, `hw.lock`, and `.hw` source
- **Unified syntax (v0.1.6)** — Bare identifiers, `[]` lists, `:` for structure, `=` for logic
- **Native SI unit parsing** — `254µm`, `4.7kΩ`, `100nF` parsed directly in lexer
- **Parametric components** — Components accept measurement parameters

**Compilation Pipeline:**
- **Rust compiler workspace** — `logos` lexer, `miette` error reporting, 7+ crates
- **Symbol table** — Component, material, profile, stackup management
- **Logical netlist synthesis** — Module-to-schematic extraction
- **Device binding validation** — Physical layout matches logical schematic (LVS)
- **Physical continuity checking** — Verifies all nets are connected (no floating islands)
- **DRC validation** — Design rule checking (spacing, width, clearances)

**Routing & Physical Synthesis:**
- **Manual routing** — Full path specification with `route ... path:` statements
- **Single-layer auto-routing** — ✅ Working for simple designs
- **Layer abstraction** — `on layer: <name>` semantic layer references
- **Trace geometry** — Width, spacing, clearance validation
- **Pour support** — Copper pours with boundary definitions

**Export Formats:**
- **SPICE netlist** — `.sp` files with device parameters
- **BOM (Bill of Materials)** — `.csv` component lists
- **GLB 3D models** — Visual preview (basic geometry)
- **DXF 2D drawings** — Board outlines

**Development Tools:**
- **Standard library** — SI units (`@std/units.hw`)
- **Test suite** — Integration tests in `.hw` format
- **Error diagnostics** — Clear error messages with suggestions



**In Development:**
- Vector-first routing engine (migration from voxel-based)
- Topological line-search router with Axis-Aligned Slab Method
- Hybrid spatial indexing (`rstar` + `geo-index`)
- Pattern-guided meander injection
- Wheeler-Sakurai BEM parasitic extraction

### � Roadmap (v0.2+)

- **Multi-layer auto-router** — Automatic via generation with bridge rule application
- **HPM package registry** — Public component library
- **Complete export suite** — Full Gerber, Excellon drill, pick-and-place
- **Advanced routing** — Length matching, differential pairs, RF features
- **LSP integration** — VS Code language server
- **Live monitor (`hsm`)** — Tauri-based visual preview with hot-reload

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
- **Discord**: https://discord.gg/G9VBxKpW
- **Twitter**: @hwsl_lang
- **Email**: hwsl.dev@gmail.com

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

**Hardware Script v0.1.7** — Making hardware design as simple as writing code.
