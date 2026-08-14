# Hardware Script - The Vision

**The Evolution**: From discrete 3D tensor grids to AST Arena Database-Driven architecture with picometer precision.

---

## The Current Reality (v0.2.1)

Hardware Script has proven that hardware design can be text-based, deterministic, and Git-friendly. The v0.2.1 compiler successfully:

- Compiles `.hw` source to SPICE (`.sp`), BOM (`.csv`), GLB, DXF, Gerber X3, Excellon, and GDSII formats
- Evaluates AST queries with an arena-based incremental compiler engine
- Provides Clippy-level diagnostic intelligence (`hwsd`) with fix suggestions and JSON mode for LLM agents
- Enforces topological obstacle-aware routing, via depth controls (blind/buried vias), and multi-via arrays
- Validates physical continuity, LVS, DRC, crosstalk limits, electromigration, and thermal current density

**The foundation works.** We are expanding physical synthesis capabilities and public registry tooling.

---

## The Architectural Evolution

### From Voxels to Vectors to Database-Driven Arenas (v0.1.5 → v0.1.8 → v0.2.1)

**1. The Voxel Era (v0.1.5-v0.1.7):**
- Proved the concept with discrete 3D tensor grid
- Morton Z-curve encoding for spatial efficiency
- $O(1)$ collision detection via grid lookups

**2. The Vector Evolution (v0.1.8):**
- **Picometer-precision database** — All coordinates as 64-bit integer picometers (1pm = 10⁻¹² m)
- **Zero-stamping scene graph** — Components stored once with lightweight transform instances
- **Continuous coordinates** — No grid quantization artifacts

**3. The AST Arena & Database Era (v0.2.0-v0.2.1):**
- **Arena Allocation & Zero-Copy Interning** — AST Arena eliminates pointer chasing and memory fragmentation
- **Salsa-Inspired Query Engine** — Incremental re-computation of netlists, layout positions, and DRC rules
- **Clippy-Level Error Intelligence** — Structured error diagnostics with context snippets, fix hints, and JSON output mode
- **Relational Placement & Range Syntax** — High-level layout constraints (`named B at 5mm right of A`) and signal slicing (`bus[0..7]`)

### Why This Matters

**Picometer Precision:**
- 64-bit integer coordinates (±9,220 km addressable range)
- No floating-point jitter or rounding errors
- Perfect for both PCB (mm scale) and ASIC (nm scale)

**Scale Invariance:**
- Change materials database: FR4+Copper → Silicon+Polysilicon
- Same compiler, same syntax, same workflow
- Hobbyist PCBs to custom ASICs in one tool

**Deterministic Compilation:**
- FixedTransform2D with i128 intermediate arithmetic
- Integer-only coordinate transforms prevent platform-specific results
- Same `.hw` source = bit-identical output across all machines

**Developer:**
- Plain text `.hw` files (Git-friendly, diff-friendly, merge-friendly)
- Modular `export module` / `export component` symbol scoping

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
- **Routing** = Analytical mathematical paths & topological slab search.
- **Physics** = Validation rules (Ohm's law, DRC, LVS, crosstalk, thermal/EM limits).

**This is pure mathematics.** It's deterministic, provable, and AI-native.

### What This Enables

#### 1. AI-Native Hardware Generation

```
User: "Design a 5V to 3.3V LDO regulator"

LLM: [Reads component database]
     [Calculates optimal layout & relative placement]
     [Generates .hw code]
     [hwc validates physics & rules]
     [Outputs compiled .hsx]

Result: First-try hardware success
```

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

### v0.2.1 (Current Release) - AST Arena & Synthesis Foundation ✅
- Unified 3-File Architecture (`hw.toml`, `hw.lock`, `.hw`).
- AST Arena & Salsa query engine with zero-copy binary exchange (`.hsx`).
- High-performance live visualizer (**Hardware Script Monitor** `hsm`) with Babylon.js.
- Continuous mathematical lines (`AnalyticTrace`), via depth controls, and via arrays.
- Range syntax, device definitions, and BOM export engine (`.csv`).
- Physics & validation engine (DRC, LVS, Crosstalk, Electromigration, Thermal checks).

### v0.2.2+ (Target Q3-Q4 2026) - Production Auto-Routing & Public Registry
- Public HPM component registry launch.
- Automatic BGA escape routing and meander length matching.
- Language Server Protocol (LSP) for IDE integration.

### v0.3 (Target 2027) - Advanced Optimization & Simulation
- Full SPICE waveform simulation integration.
- Advanced RF parasitic extraction and thermal finite-element modeling.

---

## Why This Architecture Wins

### 1. Discrete > Continuous

**Continuous geometry** (traditional):
- Floating-point errors.
- Complex collision detection.
- Unpredictable routing.
- Hard for AI to reason about.

**Discrete grid** (Hardware Script):
- Integer coordinates (exact nanometer fixed-point math).
- $O(1)$ collision detection.
- Deterministic routing.
- AI-native representation.

### 2. Tensor > Geometry

**Geometric representation** (traditional):
- Lines, arcs, polygons.
- Complex intersection math.

**Tensor representation** (Hardware Script):
- 3D array of states.
- Simple array operations.
- Same math at all scales.


## The Market Opportunity

### Current EDA Market

- **PCB Tools**: KiCad (free), Altium ($7K/year), Eagle (acquired)
- **Silicon Tools**: Cadence, Synopsys ($100K+/year)
- **Total Market**: $12B+ annually
- **Problem**: Fragmented, expensive

### Hardware Script Opportunity

**Hobbyist/Education** (Free tier):
- Students learning electronics.
- Makers and hobbyists.
- Open-source projects.
- AI experimentation.

**Professional** (Commercial license):
- Startups designing custom boards.
- Companies needing rapid prototyping.
- Teams wanting Git-based workflows.
- AI-driven hardware generation.

## The End Game

### 10 Years from Now

**Hardware design looks like software development**:

```hw
# Import standard libraries
import power, sensors, comms

# Define system
space SmartDevice:
    dimensions: 50mm by 50mm by 10mm
    
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

This vision is achievable. The v0.1.7 compiler proves the foundation works.

**What we need**:
- Community to build component libraries.
- Contributors to add features.
- Companies to adopt and fund development.
- Researchers to push the boundaries.

---

**Hardware Script** - Making hardware design as simple as writing code.

**Join the revolution**: [[GitHub Repository](https://github.com/HardwareScript/hwc)]

---

**Document Status**: Vision Statement  
**Last Updated**: Q2 2026  
**This is where we're going. v0.1 is just the beginning.**
