# Hardware Script - The Vision

**The Evolution**: From discrete 3D tensor grids to continuous vector-first architecture with picometer precision.

---

## The Current Reality (v0.1.8-alpha)

Hardware Script has proven that hardware design can be text-based, deterministic, and Git-friendly. The v0.1.8 compiler successfully:

- Compiles `.hw` text files to SPICE, BOM, GLB, and DXF formats
- Validates physical continuity and logical correctness (LVS)
- Performs design rule checking (DRC)
- Handles ASIC and PCB designs with semantic layer abstraction
- Provides clear, structured error messages for debugging

**The foundation works.** Now we're building the advanced routing and synthesis pipeline.

---

## The Architectural Evolution

### From Voxels to Vectors (v0.1.5-v0.1.7 → v0.1.8)

**The Voxel Era (v0.1.5-v0.1.7):**
- Proved the concept with discrete 3D tensor grid
- Morton Z-curve encoding for spatial efficiency
- $O(1)$ collision detection via grid lookups
- Successfully validated deterministic compilation

**The Vector Evolution (v0.1.8):**
- **Picometer-precision database** — All coordinates as 64-bit integer picometers (1pm = 10⁻¹² m)
- **Zero-stamping scene graph** — Components stored once with FixedTransform2D instances
- **Continuous coordinates** — No grid quantization, no staircase artifacts
- **Hybrid spatial indexing** — `rstar` (dynamic) + `geo-index` (static) for $O(\log N)$ queries
- **Scale invariance preserved** — From PCBs to sub-nanometer silicon in the same tool

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
- Same `.hw` source = identical output across all machines

**Developer Experience:**
- Plain text `.hw` files (Git-friendly, diff-friendly, merge-friendly)
- AI-readable and AI-writable (LLMs can generate valid hardware)
- Import system for modular, reusable designs

---

## The Ultimate Vision: Hardware as Code

### The Fundamental Insight

Physical hardware is just:
```
Space × Materials × Routing × Physics
```

Where:
- **Space** = 3D tensor grid (discrete coordinates).
- **Materials** = Database of physical properties.
- **Routing** = Analytical mathematical paths.
- **Physics** = Validation rules (Ohm's law, thermal limits, etc.).

**This is pure mathematics.** It's deterministic, provable, and AI-native.

### What This Enables

#### 1. AI-Native Hardware Generation

```
User: "Design a 5V to 3.3V LDO regulator"

LLM: [Reads component database]
     [Calculates optimal layout]
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

### v0.1 (Current Series) - The Foundation ✅
- Unified 3-File Architecture (`hw.toml`, `hw.lock`, `.hw`).
- Rust compiler core (`hwc`) with zero-copy exchange binaries (`.hsx`).
- High-performance live visualizer (**Hardware Script Monitor** `hsm`) with Babylon.js.
- Continuous mathematical lines (`AnalyticTrace`) and cylindrical vias.
- Standard Library (`@std`) and public HPM Package Registry.
- Comptime module flattening, loop unrolling, and logic synthesis.

### v0.2 (Planned Q2 2026) - Stabilization & Release
- First production-ready compiler release.
- Complete vendor package database.
- Multi-layer substrate and power planes.
- Signal timing, differential pairs, and impedance constraints.

### v0.3 (Planned Q3 2026) - Advanced Optimization
- Automatic BGA escape routing.
- Advanced thermal simulation (Ngspice waveform integration).
- Real-time physics visual debugging overlays.

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
