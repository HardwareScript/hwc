# Hardware Script - The Vision

**The Evolution**: From discrete 3D tensor grids → AST Arena Database-Driven architecture → **Turing-complete comptime HDL on a Linear Bytecode Virtual Machine with Data-Oriented Hierarchical Routing (DOPHR)**.

---

## The Current Reality (v0.3.0)

Hardware Script has proven that hardware design can be text-based, deterministic, Git-friendly — and now **Turing-complete at compile time**. The v0.3.0 compiler successfully:

- Compiles `.hw` source to SPICE (`.sp`), BOM (`.csv`), GLB, DXF, Gerber X3, Excellon, and GDSII formats (unchanged output matrix)
- Implements a **Linear Bytecode Virtual Machine (`hwc-eval`)** — a Turing-complete, sandboxed comptime evaluation engine that executes all layout logic (functions, structs, loops, dimensional math, geometry emission) *before* any synthesis happens
- Computes every physical quantity with **128-bit integer picometer arithmetic** — no floating-point jitter, bit-identical output across Windows/Linux/macOS
- Routes with **DOPHR** (Data-Oriented Progressive Hierarchical Routing): a hardened 3-stage global → panel → detailed pipeline with negotiated-congestion global routing and lock-free spatial 4-coloring
- Provides Clippy-level diagnostic intelligence (`hwsd`) with fix suggestions and machine-readable JSON diagnostics
- Validates physical continuity, LVS, DRC, crosstalk limits, electromigration, and thermal current density

**The foundation is no longer just a markup language — it is a generative hardware computer.** We are expanding physical synthesis capabilities, public registry tooling, and AI-native generation.

---

## The Architectural Evolution

### From Voxels to Vectors to Database-Driven Arenas to Comptime Virtual Machines (v0.1.5 → v0.1.8 → v0.2.1 → v0.3.0)

**1. The Voxel Era (v0.1.5-v0.1.7):**
- Proved the concept with discrete 3D tensor grid
- Morton Z-curve encoding for spatial efficiency
- $O(1)$ collision detection via grid lookups

**2. The Vector Evolution (v0.1.8):**
- **Picometer-precision database** — All coordinates as 64-bit integer picometers (1pm = 10⁻¹² m)
- **Zero-stamping scene graph** — Components stored once with lightweight transform instances
- **Continuous coordinates** — No grid quantization artifacts

**3. The AST Arena & Database Era (v0.2.0-v0.2.2):**
- **Arena Allocation & Zero-Copy Interning** — AST Arena eliminates pointer chasing and memory fragmentation
- **Salsa-Inspired Query Engine** — Incremental re-computation of netlists, layout positions, and DRC rules
- **Clippy-Level Error Intelligence** — Structured error diagnostics with context snippets, fix hints, and JSON output mode
- **Relational Placement & Range Syntax** — High-level layout constraints (`named B at 5mm right of A`) and signal slicing (`bus[0..7]`)

**4. The Turing-Complete Comptime HDL Era (v0.3.0):**
- **Linear Bytecode Virtual Machine (`hwc-eval`)** — Build-time-only evaluator. Functions, spaces, structs, and top-level scripts are compiled into `Chunk` bytecode and executed on a flat activation stack with static activation records. A hermetic sandbox caps execution at `10_000_000` steps and `256` levels of recursion (Halting-Problem guard).
- **128-bit Picometer Arithmetic** — Every `Measurement` is an `i128` scaled to a canonical internal unit (pm, nV, pA, µΩ, aF, pH, fs, Hz, pW, mK, …). Dimensional algebra is enforced: `Length × Length → Area`, `Voltage × Current → Power`, `Current × Resistance → Voltage`. Unit-mismatched operations are compile-time errors.
- **Declarative → Generative** — Real control flow (`if`/`else`/`match` as expressions, `for` loops with `break`/`continue`, `while`), compound assignment (`+= -= *= /= %=`), arrays (`.push`/`.pop`/`.len`/slices), tuples (destructuring), structs, enums, `fn` with typed parameters, named arguments, and default values. Geometry is *emitted* through native `space.add_*` builtins compiled to `EmitPolygon`/`EmitContact`/`EmitDevice`/`EmitRoute` opcodes.
- **DOPHR 3-Stage Router** — Stage 1: 3D Volumetric Tensor global routing (PathFinder negotiated congestion). Stage 2: Panel Track Assignment (continuous track anchors). Stage 3: Guided Detailed Routing with lock-free spatial 4-coloring and adaptive guide inflation.
- **Bit-Identical Reproducibility** — Same `.hw` source produces byte-identical GDSII/GLB across all platforms.

### Why This Matters

**Picometer Precision:**
- 128-bit integer coordinates (±9,220 km addressable range for 64-bit; full i128 for synthesized math)
- No floating-point jitter or rounding errors
- Perfect for both PCB (mm scale) and ASIC (nm scale)

**Scale Invariance:**
- Change materials database: FR4+Copper → Silicon+Polysilicon
- Same compiler, same syntax, same workflow
- Hobbyist PCBs to custom ASICs in one tool

**Deterministic Compilation:**
- 128-bit integer coordinate + measurement transforms prevent platform-specific results
- Integer-only dimensional math → no `0.1 + 0.2` style drift
- Same `.hw` source = bit-identical output across all machines

**Generative Power:**
- Layout is *computed*, not merely described — loops, recursion (bounded), functions, and parametric PCells generate geometry
- The comptime VM exits before synthesis, so the manufactured board contains no runtime logic

**Developer:**
- Plain text `.hw` files (Git-friendly, diff-friendly, merge-friendly)
- Modular `export module` / `export component` / `export fn` symbol scoping

---

## The Ultimate Vision: Hardware as Code

### The Fundamental Insight

Physical hardware is just:
```
Space × Materials × Routing × Physics
```

Where:
- **Space** = Continuous picometer spatial coordinate system.
- **Materials** = Database of atomic, thermal, and electrical properties.
- **Routing** = Analytical mathematical paths & topological slab search (DOPHR).
- **Physics** = Validation rules (Ohm's law, DRC, LVS, crosstalk, thermal/EM limits).

**This is pure mathematics.** It's deterministic, provable, and AI-native.

### What This Enables

#### 1. Generative Layout (Computed, Not Just Described)

```hw
# A comptime function stamps a parametric via array
fn via_array(name: String, net: Net, at: Point2D, count: Int) {
    for i in 0..count {
        let vy = at.y + (i * 400nm)
        space.add_contact(from: "polyres", to: "li1",
            at: [at.x, vy], diameter: 170nm, net: net)
    }
}

space Resistor_Space {
    via_array("Via_A", In, [10.0um, 5.0um], 3)
}
```

**Result**: Layout is computed at compile time — loops, functions, and PCells generate geometry deterministically.

#### 2. Formal Verification

```rust
// Prove mathematically that no shorts exist
assert!(netlist_arena.verify_no_shorts());
```

**Result**: Provably correct hardware (like formal verification in software).

#### 3. Parametric Hardware Libraries

```hw
import BuckConverter from "@power/buck"

add BuckConverter (
    input: 12V,
    output: 5V,
    current: 2A,
    efficiency: 0.90
) named Converter1 at [x: 10mm, y: 10mm, layer: l1]
```

**Result**: Hardware becomes as reusable as software libraries.

#### 4. Cross-Scale Design

```hw
space System:
    # Custom silicon chip
    add CustomASIC named Processor at [x: 10mm, y: 10mm, layer: l1]

    # PCB board
    add MotherBoard named Board at [x: 0, y: 0, layer: l1]
```

**Result**: System-level design in a single compile pass.

---

## The Roadmap to This Vision

### v0.3.0 (Current Release) - Turing-Complete Comptime HDL ✅
- **`hwc-eval` Linear Bytecode VM** — Turing-complete, sandboxed comptime evaluation engine (86-instruction ISA, `Chunk`/`OpCode` model)
- **128-bit Picometer Arithmetic** — `MeasurementValue { raw: i128, dimension }` with strict dimensional algebra
- **Generative language ergonomics** — `fn`, `struct`, `enum`, `match`, expression-`if`, block tail expressions, `break`/`continue`, compound assignment, arrays + slices, tuple destructuring, named/default args, unit converters (`.to_float()`/`.to_pm()`/`.to_um()`), `{}` string interpolation
- **DOPHR 3-Stage Router** — volumetric tensor global → panel track assignment → guided detailed routing with spatial 4-coloring
- **Standard library PDK modules** — `@std/primitives/{units,math}`, `@std/layout/{placement,via,passives,pcb}`, `@std/pdk/sky130/{rules,devices,nmos,pmos,tap,strap,pad,passives}`
- **Dual CLI modes** — `hwc build` (full synthesis) and `hwc eval` / `hwc run` (<5ms pure comptime compute, zero meshing)
- Continuous mathematical lines (`AnalyticTrace`), via depth controls, and via arrays
- Range syntax, device definitions, and BOM export engine (`.csv`)
- Physics & validation engine (DRC, LVS, PIVB, crosstalk, EM, thermal checks)

### v0.3.1+ (Near-Term) - Production Auto-Routing & Public Registry
- Public HPM component registry launch
- Automatic BGA escape routing and meander length matching
- Miter pass (45° corner chamfering)
- Language Server Protocol (LSP) for IDE integration

### v0.4 (Target 2027) - Advanced Optimization & Simulation
- Full SPICE waveform simulation integration (`ngspice`/`Xyce` runner)
- Advanced RF parasitic extraction and thermal finite-element modeling (FEM)
- Machine-readable diagnostic JSON for tooling and CI integration

---

## Why This Architecture Wins

### 1. Turing-Complete > Markup

**Declarative markup** (old):
- Describes a fixed layout
- No loops, no functions, no computed geometry
- Repetition must be hand-written

**Generative comptime HDL** (Hardware Script v0.3.0):
- `fn`, `struct`, `enum`, `match`, loops compute geometry
- PCells and parametric generators stamp vias/pours
- Same source scales from one resistor to a 25M-gate SoC

### 2. Discrete > Continuous

**Continuous geometry** (traditional):
- Floating-point errors
- Complex collision detection
- Unpredictable routing
- Hard for AI to reason about

**Discrete picometer grid** (Hardware Script):
- 128-bit integer coordinates (exact fixed-point math)
- $O(1)$ collision detection
- Deterministic routing
- Generative, machine-checkable representation

### 3. Tensor > Geometry

**Geometric representation** (traditional):
- Lines, arcs, polygons
- Complex intersection math

**Tensor representation** (Hardware Script):
- 3D array of states
- Simple array operations
- Same math at all scales

---

## The Market Opportunity

### Current EDA Market

- **PCB Tools**: KiCad (free), Altium ($7K/year), Eagle (acquired)
- **Silicon Tools**: Cadence, Synopsys ($100K+/year)
- **Total Market**: $12B+ annually
- **Problem**: Fragmented, expensive

### Hardware Script Opportunity

**Hobbyist/Education** (Free tier):
- Students learning electronics
- Makers and hobbyists
- Open-source projects
- AI experimentation

**Professional** (Commercial license):
- Startups designing custom boards
- Companies needing rapid prototyping
- Teams wanting Git-based workflows
- AI-driven hardware generation

## The End Game

### 10 Years from Now

**Hardware design looks like software development**:

```hw
# Import standard libraries
import power, sensors, comms

# Define system
space SmartDevice:
    dimensions: [50mm, 50mm, 10mm]

    # Custom silicon
    add RISC_V_Core (
        frequency: 1GHz,
        cores: 4,
        process_node: 3nm
    ) named Processor at [x: 10mm, y: 10mm, layer: l1]

    # Power management
    add BuckConverter (input: 12V, output: 5V, current: 2A) named PowerSystem at [x: 30mm, y: 10mm, layer: l1]

    # Sensors
    add TemperatureSensor named TempSens at [x: 10mm, y: 30mm, layer: l1]
    add Accelerometer named Accel at [x: 20mm, y: 30mm, layer: l1]
```

**Result**: Hardware becomes as easy as writing code.

---

## Call to Action

This vision is achievable. The v0.3.0 compiler proves the generative foundation works.

**What we need**:
- Community to build component libraries
- Contributors to add features
- Companies to adopt and fund development
- Researchers to push the boundaries

---

**Hardware Script** - Making hardware design as simple as writing code.

**Join the revolution**: [[GitHub Repository](https://github.com/HardwareScript/hwc)]

---

**Document Status**: Vision Statement
**Last Updated**: Q3 2026 (v0.3.0 milestone)
**This is where we're going. v0.3.0 is the generative turning point.**
