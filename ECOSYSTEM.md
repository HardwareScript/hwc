# Hardware Script - Ecosystem Architecture

**From a "cool script" to an industry standard**

---

## The Complete Ecosystem

Hardware Script isn't just a compiler—it's a complete ecosystem for hardware development, inspired by the best parts of modern software tooling (Rust, Elixir, Node.js). It unifies logical schematics, physical layouts, mechanical rules, and materials databases into a singular, highly efficient developer workflow.

---

## 1. UHWSL - The Formal Specification

**Universal Hardware Scripting Language**

**Purpose**: The formal language specification (like ECMAScript for JavaScript or IEEE 1364 for Verilog).

**Usage**:
- **Formal**: "This board is written in UHWSL."
- **Casual**: "I'm using Hardware Script" or "HWS."

**Why**: Establishes Hardware Script as a legitimate standard, not just a one-off tool.

**Comparison**:
- **VHDL** = VHSIC Hardware Description Language
- **HTML** = HyperText Markup Language
- **UHWSL** = Universal Hardware Scripting Language

---

## 2. The Unified File Architecture

Hardware Script eliminates file-type fragmentation by adopting a unified **3-File Architecture** for development, plus a single compiled binary exchange format:

*   **`hw.toml`**: The Project Manifest. Stores metadata, build targets, profile references, and package dependencies.
*   **`hw.lock`**: The Version Lockfile. Auto-generated to secure exact versions, cryptographic hashes, and dependency trees for reproducible builds.
*   **`.hw`**: The Universal Source File. Everything that defines physical or logical reality—materials, profiles, components, modules, constraints, and tests—is authored in standard `.hw` files.
*   **`.hsx`**: The Hardware Script eXchange Binary. The unified compiler output that packs all 2D/3D visual geometry, SPICE netlists, and logical `NetlistArena` connections into a high-performance, memory-mappable binary.

---

### `.hw` - The Universal Source File

In Hardware Script, there are no separate file types for materials, footprints, or schematics. Everything is defined using clean, top-level blocks inside `.hw` source files. This design utilizes the **Logical/Physical Duality** principle to separate concerns while keeping the ecosystem unified:

*   **Logical Layers (Pure Intent)**:
    *   `module`: Electrical schematics containing logic blocks and logical net routes with no coordinates.
*   **Physical Layers (Absolute Reality)**:
    *   `space`: The physical board assembly, defining physical boundaries, component placement coordinates, and trace routing.
    *   `component`: Parametric footprint definitions, local pin grids, and physical bodies.
    *   `material`: Atomic, thermal, and electrical properties of chemical compounds.
    *   `profile`: Factory capabilities, fabrication limits, and via stackup rules.
*   **Constraints Layers (Rules)**:
    *   `mechanical`: Physical enclosure keep-outs and mounting rules.
    *   `signal_group`: Differential pairs, RF properties, and signal timing bounds.
    *   `interface`: Logical-to-firmware pin bindings.
    *   `test`: CI/CD physics and continuity assertions.

---

### `.hsx` - The Hardware Script eXchange Binary

**Purpose**: A unified zero-copy-ready binary representation of the compiled system.

**Contains**:
- Embedded 3D mesh data (AnalyticTrace primitives: LineSegments and Rectangular Prisms).
- Embedded 2D vector data (traces, outlines).
- Embedded logical netlist (`NetlistArena` database).
- Embedded SPICE definitions.
- Visual Preview and diagnostic states (dirty chunks, dirty vectors).

**Usage**:
The `.hsx` file is consumed directly by the live-preview companion, **Hardware Script Monitor** (`hsm`), which watches the file for compiler refreshes, providing hot-reload feedback in under 50ms.

---

### `.hw` Libraries - The Sub-Assembly Concept

**Purpose**: Pre-routed modules (the "holy grail" of hardware reusability)

**The Problem**: Nobody wants to place 15 tiny components just to regulate power. They want a complete, tested, pre-wired module.

**The Solution**: Treat a `.hw` file as a reusable library. A library is a fully routed mini-board on its own local grid that can be imported and placed as a single unit.

**Example Library (`@power/5v-regulator.hw`)**:
```hw
# This is a complete, reusable 5V voltage regulator module
module Regulator5V:
    pins: [VIN, VOUT, GND]

    # Internal components
    add LM7805 named Regulator
    add Capacitor (val: 10uF) named C1
    add Capacitor (val: 1uF) named C2

    # Logical connections
    route C1.Plus to Regulator.In
    route Regulator.Out to C2.Plus
    route C1.Minus to Regulator.GND
    route C2.Minus to Regulator.GND

    # Exposed pins
    VIN = Regulator.In
    VOUT = Regulator.Out
    GND = Regulator.GND
```

**Usage in Main Board (`main.hw`)**:
```hw
import Regulator5V from "@power/5v-regulator"

space MyRobot:
    dimensions: 100mm by 100mm by 2.0mm
    grid: 1000 by 1000 by 4
    profile: JLCPCB_4Layer
    origin: tl by t

    # Place the entire pre-routed module as one piece
    add Regulator5V named PowerSupply at [x: 50mm, y: 50mm, layer: l1]

    # Connect to the exposed pins
    route Battery.Plus to PowerSupply.VIN
    route PowerSupply.VOUT to ESP32.VIN
```

**What Happens**: The compiler drops the entire regulator module (with all internal components and routing) into the main board's layout.

**Why This Is Revolutionary**:
- Junior developers can build complex systems by snapping together pre-built modules.
- Modules are tested once, reused everywhere.
- Same concept as npm packages, but for physical hardware.
- AI can reason about high-level architecture instead of individual resistors.

**Analogy**: Like importing a React component library vs writing raw HTML/CSS.

---

## 3. hwc - The Core Compiler

**Hardware Script Compiler**

**Purpose**: The main compilation toolchain, written entirely in Rust.

**Implementation**:
- **v0.1.0**: Python proof-of-concept (NumPy, PyYAML).
- **v0.1.4+**: Native Rust rewrite for blazing sub-millisecond compilation, strictly sandboxed.
- **v0.1.7**: Continuous analytic trace rendering (`AnalyticTrace`), high-performance memory-mapped binary (`.hsx`) generation.
- **v0.2.0-v0.2.1**: AST Arena database-driven query architecture (Salsa-inspired incremental engine), Relational Topological Placement Engine (Single Anchor Law, intra-device DRC exemptions), Material-Specific Via Depth & Substrate Cutout Resolver, Automated PDK Geometry Extraction ($AD, AS, PD, PS$), Clippy-level error diagnostics (`hwsd`), range syntax (`[0..7]`), device definitions, via depth/array controls, BOM export (`.csv`), automated SPICE testbench suite generation (`ac.sp`, `dc.sp`, `tran.sp`), and full DRC/LVS/crosstalk/thermal physics validation.

### Commands

```bash
# Compile a board to unified .hsx representation
hwc build board.hw

# Validate syntax and design rules without building
hwc check board.hw

# Generate specific manufacturing outputs
hwc build board.hw --target pcb     # Gerbers, drills, DXF outline
hwc build board.hw --target spice   # SPICE netlist (.cir)
hwc build board.hw --target viz     # Static 3D models (.glb, .obj)

# Run CI/CD physics validation tests
hwc test tests.hw

# Watch mode (recompile on changes)
hwc watch board.hw

# Show version
hwc --version

# Get help
hwc --help
```

**Analogy**: Like `rustc` (Rust compiler) or `tsc` (TypeScript compiler).

---

## 4. hpm - The Package Manager

**Hardware Package Manager** (or Hardware Script Package Manager)

**Purpose**: Install, manage, and publish hardware components and Soft IP libraries.

**Why "hpm"**: Instantly recognizable to anyone who's used `npm` (Node Package Manager) or `cargo` (Rust package manager).

### Commands

```bash
# Initialize a new project (creates hw.toml, main.hw, and .hw-llm-index.md)
hpm init

# Install a single component
hpm install passive/resistor_0805
hpm install ics/esp32_c3
hpm install active/transistor_2n2222

# Install a library/sub-assembly module
hpm install @power/5v-regulator      # Complete voltage regulator module
hpm install @motor/h-bridge          # Complete motor driver circuit
hpm install @sensors/imu-module      # IMU with all support components

# Install with options
hpm install ics/esp32_c3 --full    # Full component with 3D models
hpm install ics/esp32_c3 --lite    # Minimal (pins and behavior only)

# Search for components or libraries
hpm search voltage regulator
hpm search motor driver

# Publish a component or library (.hw source format)
hpm publish transistor_2n2222.hw
hpm publish @power/5v-regulator.hw

# Update all components (respects hw.toml bounds and updates hw.lock)
hpm update

# List installed components and libraries
hpm list

# Show component/library info
hpm info ics/esp32_c3
hpm info @power/5v-regulator
```

### Package Registry Structure

**GitHub-based (Public Community Registry)**:
```
hardwarescript-registry/
├── registry.yaml
└── packages/
    ├── passive/
    │   ├── resistor_0805.yaml
    │   └── capacitor_0603.yaml
    ├── active/
    │   └── transistor_2n2222.yaml
    └── ics/
        └── esp32_c3.yaml
```

**Registry Entry**:
```yaml
packages:
  # Single component
  "ics/esp32_c3":
    type: "component"
    url: "https://github.com/hw-components/esp32-c3"
    version: "2.1.0"
    description: "ESP32-C3 RISC-V WiFi/BLE microcontroller"
    author: "Hardware Script Community"
    license: "MIT"
    verified: true
    downloads: 15420
  
  # Library/sub-assembly
  "@power/5v-regulator":
    type: "library"
    url: "https://github.com/hw-libraries/5v-regulator"
    version: "1.0.3"
    description: "Complete 5V voltage regulator with LM7805, capacitors, and routing"
    author: "Hardware Script Community"
    license: "MIT"
    verified: true
    downloads: 8932
    dependencies:
      - "ics/lm7805"
      - "passive/capacitor_electrolytic"
```

**Installation Flow**:
1. `hpm install ics/esp32_c3`
2. `hpm` fetches `registry.yaml` from GitHub.
3. `hpm` finds package URL.
4. `hpm` clones component repo to `~/.hw/components/ics/`.
5. Component is available for import.

**Cost**: $0 (GitHub hosts everything).

---

## 5. hwsd - The Documentation & Error Engine

**Hardware Script Documentation** (and LLM Feedback System)

**Purpose**: AI-native structured error handling and auto-generated documentation.

### The Documentation Syntax: # vs ##

**Inspired by**: Rust's `///` and Elixir's `@doc` system.

**Two types of comments**:
- `#` - Ignored by compiler, for humans reading the code.
- `##` - Intercepted by `hwsd`, becomes auto-generated documentation.

**Example (`motor_controller.hw`)**:
```hw
## The Main Motor Controller
## Accepts standard 3.3V PWM signals and drives up to 2A DC motors.
## WARNING: Ensure the main trace is at least 2mm wide for current capacity.
module MotorController:
    # I used the cheaper L298N here to save costs for the MVP
    add Driver_L298N named Driver at [x: 5mm, y: 5mm, layer: l1]
    
    ## Input: PWM signal from microcontroller (3.3V logic)
    ControlSignal = Driver.PWM_IN
    
    ## Output: High-current motor connection (up to 2A)
    MotorPower = Driver.MOTOR_OUT
```

**When you run**:
```bash
hwsd generate motor_controller.hw
```

**hwsd outputs**:
- Beautiful HTML documentation (like Rust's `rustdoc` or Elixir's `HexDocs`).
- Searchable API reference.
- All `##` comments extracted and formatted.
- Pin descriptions, warnings, and usage examples.

**Why This Is a Superpower**:
- LLMs can read the auto-generated docs to learn how to use any library.
- No separate documentation to maintain (docs live in the code).
- AI instantly knows all inputs, outputs, warnings, and constraints.
- Community libraries are self-documenting.

### The Problem hwsd Solves

**Traditional compiler errors**:
```
Error at line 42: Voltage mismatch
```

**LLM response**: "I don't know what to do."

**hwsd errors**:
```
[Error] Voltage mismatch at [x: 10mm, y: 20mm, layer: l1]
  Expected: 3.3V
  Got: 5V
  Component: ESP32_C3 (max input: 3.6V)

[Hint] Insert a voltage regulator between Battery.Out and ESP32.VIN
  Suggested component: LDO_3V3
  Install: hpm install power/ldo_3v3
  
[Example]
  add LDO_3V3 named Regulator at [x: 8mm, y: 20mm, layer: l1]
  route Battery.Out to Regulator.In
  route Regulator.Out to ESP32.VIN
```

**LLM response**: Instantly fixes the code and recompiles.

### The AI Feedback Loop

```
1. LLM generates .hw code
2. hwc compiles with hwsd validation
3. hwsd outputs structured errors + hints
4. LLM reads errors, understands problem
5. LLM modifies code based on hints
6. Repeat until compilation succeeds
```

**Result**: AI can debug its own hardware designs autonomously.

### hwsd Commands

```bash
# Generate documentation for a component
hwsd doc transistor_2n2222.hw

# Explain an error code
hwsd explain E0042

# Show all error codes
hwsd errors

# Check for common mistakes
hwsd lint board.hw
```

### Error Format (Structured for AI)

```json
{
  "error_code": "E0042",
  "severity": "error",
  "location": {
    "file": "board.hw",
    "line": 15,
    "column": 5,
    "coordinates": [10000000, 20000000, 1]
  },
  "message": "Voltage mismatch",
  "details": {
    "expected": "3.3V",
    "got": "5V",
    "component": "ESP32_C3",
    "max_input": "3.6V"
  },
  "hint": "Insert a voltage regulator",
  "suggestion": {
    "component": "LDO_3V3",
    "install": "hpm install power/ldo_3v3",
    "example": "add LDO_3V3 named Regulator at [x: 8mm, y: 20mm, layer: l1]"
  },
  "documentation": "https://docs.hardwarescript.org/errors/E0042"
}
```

---

## 6. hsm - The Live-Preview Monitor

**Hardware Script Monitor**

**Purpose**: The live-preview companion application, built with Tauri v2 + SolidJS.

**Workflow**:
1. You edit and save your Hardware Script source file (`main.hw`).
2. The `hwc` compiler triggers in the background, updating the high-performance binary exchange format (`project.hsx`).
3. **`hsm` watches the `.hsx` file and refreshes instantly** (under 50ms).

### Core Viewports & Technologies

*   **3D Viewport**: Uses **Babylon.js** for high-end PBR materials, shadows, and environment reflections. It renders connected `.glb` package meshes positioned precisely in space.
*   **2D Vector Viewport**: Uses **PixiJS** (running in a Web Worker via OffscreenCanvas) to render massive layouts (1M+ copper trace segments) at 60 FPS.
*   **DXF Viewport**: Uses Three.js orthographic rendering to visualize compiled DXF 2D vectors for mechanical checks.
*   **SPICE Waveforms**: Uses **uPlot** to render 10M-point analog/digital waveform data from SPICE simulation tests in under 1ms.
*   **Netlist Inspector**: Interactive tree of connections, showing structural properties and bindings.

---

## 7. The Complete Workflow

### For Humans

```bash
# 1. Initialize project (creates manifest, main board, and AI index)
hpm init

# 2. Install components
hpm install ics/esp32_c3
hpm install power/lm7805

# 3. Write hardware code (modules, components, and layout spaces)
vim board.hw

# 4. Check for design rule violations
hwc check board.hw

# 5. Compile to exchange binary
hwc build board.hw

# 6. Visualize and hot-reload changes
hsm build/board.hsx

# 7. Generate industry manufacturing packages
hwc build board.hw --target pcb
```

### For AI (LLMs)

```bash
# 1. LLM generates code
echo "space MainBoard..." > board.hw

# 2. Compile with structured JSON errors
hwc build board.hw --format json > errors.json

# 3. LLM reads errors and hints
cat errors.json

# 4. LLM updates the code based on the generated hint examples
echo "add LDO_3V3..." >> board.hw

# 5. Recompile
hwc build board.hw --format json

# 6. Repeat until compilation succeeds
```

---

## 8. The Ecosystem Map

```
┌─────────────────────────────────────────────────┐
│                    UHWSL                        │
│         (Universal Hardware Scripting           │
│              Language Specification)            │
└─────────────────────────────────────────────────┘
                         │
         ┌───────────────┼───────────────┐
         │               │               │
     ┌───▼───┐       ┌───▼───┐       ┌───▼───┐
     │hw.toml│       │ .hw   │       │ .hsx  │
     │Manifest│      │Source │       │Binary │
     └───┬───┘       └───┬───┘       └───▲───┘
         │               │               │
         └───────┬───────┘               │
                 │                       │
            ┌────▼────┐             ┌────┴────┐
            │   hwc   │────────────▶│  Build  │
            │Compiler │             │ Outputs │
            └────┬────┘             └─────────┘
                 │
         ┌───────┼───────┐
         │       │       │
     ┌───▼──┐  ┌─▼──┐  ┌─▼──┐
     │ hpm  │  │hwsd│  │hsm │
     │Pkgs  │  │Docs│  │Mon │
     └──────┘  └────┘  └────┘
```

---

## 9. Comparison to Other Ecosystems

### JavaScript/Node.js

| JavaScript | Hardware Script |
|------------|----------------|
| ECMAScript | UHWSL |
| `.js` | `.hw` |
| `node` | `hwc` / `hsm` |
| `npm` | `hpm` |
| JSDoc | `hwsd` |

### Rust

| Rust | Hardware Script |
|------|----------------|
| Rust Language | UHWSL |
| `.rs` | `.hw` |
| `rustc` | `hwc` |
| `cargo` | `hpm` |
| `rustdoc` | `hwsd` |

### Python

| Python | Hardware Script |
|--------|----------------|
| Python Language | UHWSL |
| `.py` | `.hw` |
| `python` | `hwc` |
| `pip` | `hpm` |
| Sphinx | `hwsd` |

---

## 10. Secondary Ecosystem Tools

### hwcf - Hardware Script Formatter

**Purpose**: Auto-format `.hw` source files (like `rustfmt` or `prettier`).

```bash
hwcf board.hw
```

### hwcl - Hardware Script Linter

**Purpose**: Check for design style issues, unused definitions, and best practices.

```bash
hwcl board.hw
```

### hwc test - Built-In Test Framework

**Purpose**: Automatically execute `.hw` physics test suites and logical assertions.

```bash
hwc test tests.hw
```

---

## 11. Installation & Platforms

### One-Line Install

```bash
curl -sSf https://hardwarescript.org/install.sh | sh
```

**Installs**:
- `hwc` (compiler)
- `hpm` (package manager)
- `hwsd` (documentation & error system)
- Standard library components (`@std`)

### Platform Support

- **Linux**: Native binary.
- **macOS**: Native binary.
- **Windows**: Native binary + WSL.
- **Web**: WebAssembly (for browser-based sandboxed builds).

---

## 12. The Naming Philosophy

### Why These Names Work

1. **Familiar**: Developers instantly recognize the pattern (`hpm`, `hwc`, etc.).
2. **Consistent**: All tools start with "hw" prefixes.
3. **Memorable**: Short, punchy, easy to type.
4. **Professional**: Sounds like an industry standard, not a hobby script project.
5. **Scalable**: Easy to add new unified tools (`hwcf`, `hwcl`).

### The Psychological Advantage

When developers see:
```bash
hpm install ics/esp32_c3
```
They immediately think: *"Oh, this is like cargo or npm for hardware. I already know how to use this."*

**Result**: Zero learning curve for the tooling, only for the unified language syntax.

---

## 13. The AI Integration

### Why hwsd Is Revolutionary

**Traditional EDA tools**:
- Binary error codes.
- Cryptic messages.
- No actionable hints.
- AI cannot parse or fix designs.

**Hardware Script with hwsd**:
- Structured JSON errors.
- Clear explanations.
- Actionable hints with complete examples.
- Package install commands included.
- AI can parse, understand, and automatically fix errors.

**The Loop**:
```
Human: "Design an LDO motor driver"
    ↓
AI: Generates main.hw
    ↓
hwc: Compiles with hwsd validation
    ↓
hwsd: "Error: Voltage mismatch on pin X. Hint: Use 3.3V LDO."
    ↓
AI: Ingests hint example, adds LDO, and connects traces
    ↓
hwc: Compiles successfully to board.hsx
    ↓
hsm: Instant preview update
    ↓
Human: Gets verified and physically correct board
```

**Time**: Seconds (vs hours of manual debugging).

---

## Conclusion

Hardware Script isn't just a compiler—it's a complete ecosystem:

- **UHWSL**: The formal specification.
- **`hw.toml` / `hw.lock` / `.hw`**: The unified 3-File Architecture.
- **`.hsx`**: The memory-mappable binary exchange format.
- **`hwc`**: The Rust compiler.
- **`hpm`**: The package manager.
- **`hwsd`**: The documentation & error engine.
- **`hsm`**: The live-preview monitor.

**This is how you build a standard, not just a tool.**

**Inspired by the best**:
- Rust's helpful compiler errors.
- Elixir's documentation and FSM syntax.
- Node.js's package ecosystem.
- Git's lockfiles and version control.

**Result**: Hardware development that feels like modern software development.

---

**Document Status**: Ecosystem Architecture  
**Last Updated**: v0.2.1 (Q3 2026)  
**This is how we define a standard.**
