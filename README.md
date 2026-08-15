
**Text-Based Hardware Design Language** — `.hw`

[![Version](https://img.shields.io/badge/version-v0.2.1-orange)]()
[![Compiler](https://img.shields.io/badge/compiler-Rust-orange)]()
[![License](https://img.shields.io/badge/license-AGPLv3-blue)]()

---

## What is Hardware Script?

Hardware Script (`.hw`) is a declarative, text-based hardware description language that compiles to industry-standard formats. Write PCB layouts and silicon IC designs as human-readable, Git-friendly text files — then compile to SPICE netlists, Gerber files, DXF drawings, GDSII, BOM, and 3D models.

The compiler (`hwc`) is written in Rust and built for picometer-precision physical synthesis. It works at every scale, from millimeter PCBs to nanometer silicon chiplets, using the same language.

---

## A Complete Example

The following is a real, working Hardware Script file — a SKY130 P+ Polysilicon Resistor targeting the SkyWater open-source PDK:

```hw
# simple_resistor_test.hw
# Device: sky130_fd_pr__res_high_po (350 Ω/□)
# Geometry: 4μm × 1.41μm ≈ ~1.0kΩ

import * from @std/primitives/units
import * from "./resistor_pdk"

module SimpleResistor:
    pins: [input In, output Out]
    route In to Out

space Simple_Resistor_Space implements SimpleResistor:
    dimensions: 20.0um by 10.0um
    profile: Resistor_3D

    nets:
        In:  { classification: signal, potential: 1.8V, current: 1.0uA }
        Out: { classification: signal, potential: 0.0V, current: 1.0uA }
        GND: { classification: ground, potential: 0.0V, current: 0.0uA }

    device_nets R1:
        BULK: GND

    # Parametric dimensions — change one value, everything adjusts
    let res_length = 4.0um
    let res_width  = 1.41um    # SKY130 quantized width
    let via_pitch  = 400nm

    # 1. Single Anchor: The ONLY absolute 'at:' in the entire file
    add pour(Polysilicon) named Resistor_Body on layer: polyres:
        device: R1.A, R1.B
        dimensions: res_length by res_width
        at: [x: 10.0um, y: 5.0um]

    # 2. Relational RPM mask (anchored to body, zero-thickness)
    add pour(Resistor_Poly_Mask) named RPM_Block on layer: rpm:
        dimensions: res_length + 360nm by res_width + 360nm
        align: center_x with Resistor_Body
        align: center_y with Resistor_Body

    # 3. Relational contact head (anchored to body edge)
    add pour(Titanium_Silicide) named Contact_A_LI on layer: li1:
        device: R1.A
        net: In
        dimensions: 400nm by res_width
        align: center_x with Resistor_Body.left + 200nm
        align: center_y with Resistor_Body

    # 4. Comptime via array loop (polyres → li1), 3 vias
    for i in 0..3:
        add contact(Tungsten) named Via_A_Poly_{i} spanning layer: polyres to li1:
            diameter: 170nm
            align: center_x with Contact_A_LI
            align: center_y with Contact_A_LI.center_y + (i - 1) * via_pitch
            net: In

# Testbench — generates circuit.sp, dc.sp, ac.sp, tran.sp automatically
test Simple_Resistor_AC_Test for Simple_Resistor_Space:
    ac: { sweep: dec, points: 20, freq: 100Hz..100MHz }
    tran: { step: 10ps, stop: 200ns }
```

**Compiles to** (from that single source file):

| Output | File |
|--------|------|
| SPICE circuit netlist | `circuit.sp` |
| DC operating point | `dc.sp` |
| AC frequency sweep | `ac.sp` |
| Transient analysis | `tran.sp` |
| Bill of Materials | `bom.csv` |
| 2D mechanical drawing | `board.dxf` |
| Gerber X3 + Excellon | `*.gbr`, `*.drl` |
| GDSII (silicon IC) | `*.gds` |
| 3D model | `*.glb` |

### Visual Output Examples

**2D Layout View:**

![Resistor 2D Layout](assets/resistor_2d_view.png)

**3D Model View:**

![Resistor 3D Model](assets/resistor_3d_view.png)

---

## The File System

Hardware Script uses exactly **three file extensions**:

| Extension | Purpose |
|-----------|---------|
| `hw.toml` | Project manifest — name, version, targets, dependencies |
| `hw.lock` | Deterministic lockfile — binary `rkyv` format, zero-copy loads |
| `.hw` | Universal source — materials, profiles, devices, modules, spaces, tests |

Everything lives in `.hw` files. There is no separate schematic format, no separate layout format.

---

## Core Language Concepts

### `space` — Physical Layout

A `space` is a physical design region. It declares dimensions, a stackup profile, nets, and geometry.

```hw
space My_Board:
    dimensions: 20mm by 20mm
    resolution: 1nm
    profile: StandardPCB
```

### `module` — Logical Interface

A `module` declares the logical schematic contract that a `space` must fulfill.

```hw
module Amplifier:
    pins: [input VIN, power VDD, ground GND, output VOUT]
    route VIN to VOUT

space Amp_Layout implements Amplifier:
    ...
```

### `material` — Physical Properties

```hw
export material Polysilicon:
    category: conductor
    symbol: Poly
    properties:
        resistivity: 7e-5ohm_m
        thermal_conductivity: 30.0W_mK
        max_current_density: 1e2A_mm2
```

### `profile` — Stackup Definition

A `profile` defines the physical layer stackup, via rules, trace constraints, and thermal limits. You reference it in a `space`.

```hw
export profile Resistor_3D:
    technology: "ASIC"
    substrate_net: GND
    stackup:
        pdiff:   [material: P_Plus_Diffusion, thickness: 200nm, routable: true]
        polyres: [material: Polysilicon,       thickness: 180nm, routable: true]
        li1:     [material: Titanium_Silicide, thickness: 100nm, routable: true]
        metal1:  [material: Aluminum,          thickness: 360nm, routable: true]
    via:
        min_diameter: 170nm
        min_spacing: 200nm
    trace:
        min_width: 300nm
        min_spacing: 300nm
    thermal:
        ambient_temp: 25C
        max_operating_temp: 125C
```

### `device` — Multi-Terminal Components

A `device` declares the contract for a multi-terminal physical structure (resistor, capacitor, MOSFET). Unlike a plain `pour` (single net), a device lets different nets meet through physics.

```hw
export device Resistor:
    terminals: [A, B, BULK]
    materials:
        A: [Polysilicon, Titanium_Silicide, Aluminum]
        B: [Polysilicon, Titanium_Silicide, Aluminum]
        BULK: [Air, P_Plus_Diffusion]
    spice:
        prefix: X
        subcircuit: sky130_fd_pr__res_high_po
        terminal_order: [A, B, BULK]
        parameters: [W, L]
        parameter_style: named
```

Geometry is bound to device terminals via the `device:` field on `pour` statements:

```hw
add pour(Polysilicon) named Resistor_Body on layer: polyres:
    device: R1.A, R1.B     # body spans both terminals
    dimensions: 4.0um by 1.41um
    at: [x: 10.0um, y: 5.0um]
```

This grants automatic DRC exemption (intentional cross-net overlap) and enables correct SPICE parameter extraction without double-counting parasitic capacitance from the device body.

### `subcircuit` — PDK Circuit Models

PDK-provided SPICE subcircuits are declared natively in `.hw` files. The compiler emits them verbatim into the generated `.sp` netlist:

```hw
export subcircuit sky130_fd_pr__res_high_po:
    terminals: [A, B, BULK]
    parameters: [W = 1.0um, L = 1.0um]
    elements:
        R_head: Resistor(nodes: [A, node_1], value: 362.0ohm)
        R_tail: Resistor(nodes: [node_2, B], value: 362.0ohm)
        R_body: Resistor(nodes: [node_1, node_2], value: 350.0ohm * (L / W))
        C_sub1: Capacitor(nodes: [A, BULK], value: 2.0fF * W * L)
        C_sub2: Capacitor(nodes: [B, BULK], value: 2.0fF * W * L)
```

### `pour` and `contact` — Physical Geometry

- `pour(Material)` — A filled region (conductor, semiconductor, mask layer)
- `contact(Material)` — A via bridging two layers

```hw
# Pour: a rectangular region on a named layer
add pour(Aluminum) named In_Pad on layer: metal1:
    dimensions: 1.0um by 1.0um
    align: center_x with Contact_A_Metal.left - 3.0um
    align: center_y with Contact_A_Metal
    net: In

# Contact: a via spanning from one layer to another
add contact(Tungsten) named Via_A spanning layer: polyres to li1:
    diameter: 170nm
    align: center_x with Contact_A_LI
    align: center_y with Contact_A_LI
    net: In
```

### `route` — Signal Routing

Routes connect named geometry. The compiler's obstacle-aware topological router calculates the physical trace path.

```hw
route In_Pad to Contact_A_Metal:
    net: In
    width: 300nm
    layer: metal1
    intent: Signal
```

### `test` — SPICE Testbenches

A `test` block configures SPICE analysis types for a space. The compiler automatically generates one `.sp` file per analysis type.

```hw
test Simple_Resistor_AC_Test for Simple_Resistor_Space:
    ac:   { sweep: dec, points: 20, freq: 100Hz..100MHz }
    tran: { step: 10ps, stop: 200ns }
```

---

## Placement System

### Absolute Placement

```hw
add pour(Aluminum) named Pad_A on layer: metal1:
    dimensions: 1.0um by 1.0um
    at: [x: 5.0um, y: 5.0um]
```

### Relational Placement (Anchor Arithmetic)

All geometry can be positioned relative to other named geometry using dot-notation anchor queries. This is the primary way to write self-healing, parametric layouts:

```hw
# Right edge of Pad_A, offset by 200nm
add pour(Aluminum) named Pad_B on layer: metal1:
    dimensions: 1.0um by 1.0um
    at: [x: Pad_A.right + 200nm, y: Pad_A.center_y]

# Midpoint between two pads (comptime anchor math)
add pour(Aluminum) named Pad_Mid on layer: metal2:
    dimensions: 500nm by 500nm
    at: [x: (Pad_A.center_x + Pad_B.center_x) / 2, y: Pad_A.center_y]
```

Available anchor properties: `.left`, `.right`, `.top`, `.bottom`, `.center_x`, `.center_y`, `.width`, `.height`.

Rules:
- Dependencies form a **Directed Acyclic Graph** — circular references are a build error (`C22`)
- All anchor math evaluates **once at compile time** to 64-bit integer picometers
- Variables declared with `let` are **immutable** constants

### `align:` Syntax

```hw
add pour(Titanium_Silicide) named Contact_A on layer: li1:
    dimensions: 400nm by 1.41um
    align: center_x with Resistor_Body.left + 200nm
    align: center_y with Resistor_Body
```

### Multi-Line Declarations

When placement constraints are long, use optional brace grouping:

```hw
add plane(Aluminum) named Metal_Pad {
    align: center_x with Poly_Strip
    align: center_y with Poly_Strip
} on layer: metal1:
    net: Signal
```

### Floorplanning Regions

```hw
region AnalogRegion:
    at: space.bottom_left + [100um, 100um]

region DigitalRegion:
    right_of: AnalogRegion with spacing: 500um
    align: top with AnalogRegion
```

---

## Parametric Generation (`for`, `if`, `let`)

Hardware Script supports **compile-time** parametric generation. These constructs evaluate during compilation; no runtime branching exists on the manufactured board.

### `for` Loops — Via and Component Arrays

```hw
# Exclusive range: 0..3 runs 3 times (i = 0, 1, 2)
for i in 0..3:
    add contact(Tungsten) named Via_A_{i} spanning layer: polyres to li1:
        diameter: 170nm
        align: center_x with Contact_A_LI
        align: center_y with Contact_A_LI.center_y + (i - 1) * 400nm
        net: In

# Inclusive range: 0..=4 runs 5 times (i = 0, 1, 2, 3, 4)
for row in 0..=4:
    for col in 0..=4:
        add pour(Aluminum) named Pad_R{row}_C{col} on layer: metal1:
            ...
```

Range semantics follow Rust: `0..N` is exclusive (N items), `0..=N` is inclusive (N+1 items).

### `if` Inside Loops — Compile-Time Conditionals

```hw
for row in 0..5:
    for col in 0..5:
        if (row + col) mod 2 == 0:
            add plane(Aluminum) named L1_R{row}_C{col} on layer: metal1:
                ...
        else:
            add plane(Tungsten) named L1_R{row}_C{col} on layer: metal1:
                ...
```

The `if` is evaluated at compile time during loop unrolling. The emitted `EntityGraph` is a static, deterministic geometry database — no conditional logic exists in the output.

### `let` Constants

```hw
let res_length    = 4.0um
let res_width     = 1.41um
let contact_width = 400nm
let via_pitch     = 400nm

add pour(Polysilicon) named Body on layer: polyres:
    dimensions: res_length by res_width
    at: [x: 10.0um, y: 5.0um]
```

---

## Module System and `export`

All definitions are **private by default**. Use `export` to make them importable.

```hw
# materials.hw
export material Polysilicon:
    category: conductor
    ...

material _InternalHelper:   # Private — cannot be imported
    ...
```

```hw
# resistor_pdk.hw
import Polysilicon, Aluminum from "./materials"

export Polysilicon           # Re-export as part of PDK API
export Aluminum

export profile Resistor_3D:
    ...
```

```hw
# design.hw
import * from "./resistor_pdk"   # Only exported symbols are available
```

Wildcard `import *` brings in all exported symbols. Selective imports are also supported:

```hw
import Polysilicon, Resistor_3D from "./resistor_pdk"
```

---

## Net Declarations

Every net used in a space must be declared with its electrical properties. The compiler uses these for physical validation, trace width derivation, and SPICE stimulus generation.

```hw
nets:
    In:  { classification: signal, potential: 1.8V, current: 1.0uA }
    Out: { classification: signal, potential: 0.0V, current: 1.0uA }
    GND: { classification: ground, potential: 0.0V, current: 0.0uA }
```

Classifications: `signal`, `power`, `ground`, `clock`.

---

## What the Compiler Produces

### SPICE Netlist

The compiler generates a structured, multi-file SPICE package. The `circuit.sp` is the DUT (Device Under Test); analysis files include it:

```spice
* Generated by hwc v0.2.1
* PDK SUBCIRCUIT
.subckt sky130_fd_pr__res_high_po A B BULK W=1u L=1u
RR_head A node_1 362ohm
RR_body node_1 node_2 {350ohm * ({L / W})}
RR_tail node_2 B 362ohm
CC_sub1 A BULK {{2fF * W} * L}
CC_sub2 B BULK {{2fF * W} * L}
.ends sky130_fd_pr__res_high_po

* EXTRACTED DEVICE
XR1 In Out GND sky130_fd_pr__res_high_po W=1.41u L=3.20u

* INTEGRATED TRACE PARASITICS (Sakurai/Wheeler BEM)
RRtr_In_0 nIn_entry In 6.527778e-1
CCgnd_In_0 In GND 1.726567e-16
```

Parasitic extraction uses the **Wheeler–Sakurai BEM method** on interconnect routing traces only. Device bodies (`add pour`) are architecturally excluded — no blocker layers needed.

### Bill of Materials

The BOM is a dual-table CSV covering both discrete component procurement and foundry fabrication material usage:

```csv
Reference,Type,Value,Package,Manufacturer,Part Number,Description,Quantity
Wafer,Substrate,0.02x0.01x0.00mm,,,,,1

# MATERIAL USAGE (Fabrication)
Reference,Type,Material,Layer,Area_nm2,Volume_nm3
Resistor_Body,Pour,Polysilicon,polyres (z:200nm),5640000,1015200000
Via_A_Poly_0,Contact,Tungsten,polyres (z:200nm),28900,8785600
...

# AGGREGATED MATERIAL TOTALS (Foundry Fabrication Summary)
Material,Total_Area_nm2,Total_Volume_nm3,Layer_Count,Coverage_Percentage
Aluminum,4628000,1666080000,6,2.3%
Polysilicon,5640000,1015200000,1,2.8%
```

---

## Compiler Toolchain

```bash
# Compile a design
hwc build my_design.hw

# Compile to specific output targets
hwc build my_design.hw --target spice    # SPICE netlists
hwc build my_design.hw --target pcb     # Gerber + drill files
hwc build my_design.hw --target gds     # GDSII IC format
hwc build my_design.hw --target viz     # GLB/OBJ 3D model
hwc build my_design.hw --target dxf     # DXF 2D drawing

# Validate without full build
hwc check my_design.hw

# Inspect the binary lockfile
hwc lock inspect build/my_design.hsx
```

The **Hardware Script Monitor** (`hsm`) opens the compiled `.hsx` binary and hot-reloads on recompile:

```bash
hsm build/my_design.hsx
```

---

## Compilation Pipeline

The following is the **actual pipeline log** for `hwc build simple_resistor_test.hw` — a 218-line `.hw` file targeting the SKY130 PDK, compiled in under 1 second:

```
[    0.99ms] Source file read successfully (10136 bytes)
[    2.88ms] Lexer complete (922 tokens)
[    6.00ms] Parser complete (2 imports, 3 definitions)
[   23.71ms] Symbol table built

[ROUTING LAYER DB] 7 layers registered (4 routable):
  pdiff   → BASE       centerline Z=100nm
  polyres → BASE       centerline Z=290nm
  li1     → INTERCONNECT centerline Z=630nm
  metal1  → INTERCONNECT centerline Z=1010nm

[PLACEMENT] 25 items placed in DAG-resolved order
  - Relational resolver evaluates anchor expressions left-to-right
  - Device-exempt: skipping clearance between R1.A, R1.B, R1.BULK (same device)
  - Z-separated layers skip cross-layer clearance automatically

[ROUTING] Topological obstacle-aware router
  - Port scoring (East/West/North/South) via clearance × alignment formula
  - In_Pad → Contact_A_Metal on metal1 @ Z=1010nm (intent: Signal)
  - Out_Pad → Contact_B_Metal on metal1 @ Z=1010nm (intent: Signal)

[VIA GEOMETRY] Per-via depth resolution
  - Via_A_Poly_0: lower depth=54nm (Polysilicon 30%), upper depth=50nm (TiSi2 50%)
  - Final span: Z=326nm → 630nm (304nm tall)

[SUBSTRATE MESH] 23 unified copper pools extruded per layer
  - 2 SiO₂ dielectric slabs with 13 polygon via cutouts each
  - earcut triangulation: 64 vertices / 108 faces per slab

[PARASITIC EXTRACTION]
  - 2 analytic routes processed (Sakurai/Wheeler BEM)
  - Net 'In'  seg 0: R=0.653Ω, C=0.173fF  (2.5µm × 300nm Al trace)
  - Net 'Out' seg 0: R=0.653Ω, C=0.173fF  (symmetric layout)
  - 4 parasitic elements total

[PIVB] Physical interconnect verification in 2.74ms
  - Net 'In':  1 device island + 1 non-device island → overlapping ✅
  - Net 'Out': 1 device island + 1 non-device island → overlapping ✅
  - Net 'GND': 1 device island + 1 non-device island → overlapping ✅

[  699ms] Exporter started

   ✅ GLB  exported in 153.8ms
   ✅ DXF  exported in  89.1ms
   ✅ SPICE Suite exported in 7.0ms   (21 components, 11 nets)
      ├─ circuit.sp  (raw DUT)
      ├─ dc.sp       (DC operating point)
      ├─ ac.sp       (AC frequency response: 100Hz–100MHz, 20pts/dec)
      └─ tran.sp     (transient: step=10ps, stop=200ns)
   ✅ BOM  exported                   (0 discrete, 24 material items: 9 pours, 2 routes, 13 contacts)

    Finished build in 0.95s
```

**Stages in order:**

| Stage | What happens |
|-------|-------------|
| **Lex** | Source tokenized via Logos DFA. SI units parsed inline (e.g. `1.41um`, `400nm`, `1.0uA`). |
| **Parse** | Tokens → immutable AST. Imports resolved, `for` loops and `if` blocks recorded as comptime generators. |
| **Symbol table** | Materials, profiles, devices, modules, subcircuits registered. |
| **Relational lowering** | `align:` / anchor-math expressions evaluated via DAG walk. All coordinates become 64-bit integer picometers. |
| **Placement** | Every `pour`, `contact`, and loop-unrolled instance placed into the spatial index. Device-exempt DRC skips cross-terminal overlap errors. Z-layer separation skips cross-layer clearance. |
| **Routing** | Topological Line-Search Router scores cardinal ports (clearance × alignment), selects exit/entry faces, then generates orthogonal trace paths via Axis-Aligned Slab Method. |
| **Via geometry** | Per-contact depth resolution from `material_contact_depths` in the PDK profile. |
| **3D mesh** | Copper pools extruded per layer; dielectric slabs built with polygon via cutouts; `earcut` triangulates for GLB export. |
| **Parasitic extraction** | Sakurai/Wheeler BEM on `analytic_routes` only. Device bodies (`pour`) are architecturally excluded — no blocker layers needed. |
| **PIVB** | Physical Interconnect Verification & Bridging — confirms every net has no floating islands. Device islands and non-device islands are bridged by bounding-box overlap. |
| **Export** | GLB, DXF, SPICE suite, BOM emitted in parallel. |

---

## Project Layout

```
hwc/
├── Cargo.toml
├── crates/
│   ├── hwc-cli/         # Command-line interface
│   ├── hwc-parser/      # Logos lexer + AST parser
│   ├── hwc-compiler/    # Two-pass compiler, Salsa query engine
│   ├── hwc-engine/      # Topological router, physical synthesis
│   ├── hwc-physics/     # DRC, LVS, thermal, EM, parasitic extraction
│   ├── hwc-export/      # SPICE, BOM, Gerber, GDSII, DXF, GLB emitters
│   ├── hwc-materials/   # Materials database
│   └── hwc-stdlib/      # Standard library prelude
├── stdlib/              # .hw standard library files
└── tests/               # Integration tests written in Hardware Script
    └── Resistor-Basics/ # SKY130 resistor — complete working example
```

---

## Technical Stack

| Component | Technology |
|-----------|-----------|
| Compiler | Rust — `logos`, `miette`, `rayon`, `rkyv`, `rstar`, `geo-index`, `clarabel`, `clipper2` |
| Coordinates | 64-bit integer picometers (1pm = 10⁻¹² m); i128 intermediates for transforms |
| Spatial index | Hybrid `rstar` (dynamic) + `geo-index` (static routing layers) |
| Router | Topological Axis-Aligned Slab Method — O(log N) obstacle resolution |
| Legalizer | `clarabel` IPM (macro) + DAG solver (local trace nudge) |
| Parasitic extraction | Wheeler + Sakurai + Greenhouse BEM — 5–10% vs. 3D field solvers |
| DRC | G-cell-local Morton-ordered sweep, AVX-512 SIMD, Rayon parallel |
| Serialization | `rkyv` zero-copy binary lockfile |
| Live monitor | Tauri v2 + SolidJS + Babylon.js (3D) + PixiJS (2D) + uPlot |

---

## Physics Validation

The compiler enforces physics at build time. Errors are structured, with inline source spans and suggested fixes.

| Check | Description |
|-------|-------------|
| **DRC** | Clearance, minimum width, minimum spacing, annular ring |
| **LVS** | Layout-versus-schematic — physical device bindings match logical module |
| **PIVB** | Physical interconnect verification — all nets are fully connected |
| **EM** | Electromigration — trace current density vs. material limits (`I_peak / J_limit`) |
| **Thermal** | Temperature rise — IPC-2152 limits, G-cell-local thermal coupling |
| **Crosstalk** | Analytical parallel trace coupling bounds (Sakurai) |
| **RF Parasitics** | Wheeler–Sakurai microstrip R/C/L/M extraction |

---

## SI Unit Parsing

SI units are parsed natively by the lexer. No conversion functions, no string parsing:

```hw
let width   = 1.41um       # 1.41 micrometers
let pitch   = 400nm        # 400 nanometers
let current = 1.0uA        # 1 microamp
let voltage = 1.8V         # 1.8 volts
let resist  = 350ohm       # 350 ohms
let cap     = 2.0fF        # 2 femtofarads
let freq    = 100MHz       # 100 megahertz
```

---

## License

Hardware Script is open source under the **GNU AGPLv3** license.

You own your hardware designs. We own the compiler. The AGPLv3 applies to the compiler source itself — compiled designs and generated output files (Gerber, SPICE, DXF, GDSII) belong entirely to you.

A commercial license is available for: modifying the compiler privately, hosting the compiler as a web service, or enterprise support agreements.

---

## Community

- **Discord**: https://discord.gg/9zqH8nuCet
- **GitHub**: https://github.com/HardwareScript
- **Email**: hardwarescript@gmail.com

