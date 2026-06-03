# Via Engine Implementation Checklist

This document tracks the implementation and testing of 3D via modeling and their interactions with substrate/copper planes.

## List 1: Physical Via Types (Geometric & Plating Classifications)

- [x] **1. Plated Through-Hole (PTH) Via**
  - *What it is:* The most common via; a hole drilled through the entire board after lamination.
  - *3D Representation:*
    - A thin hollow cylinder (the plating barrel) spanning the board height.
    - Two distinct flat annular rings (pads) at the top and bottom, connected to the barrel edges.
  - *Engine Implementation:*
    - **Unified Manifold**: Generated as a single mesh with 4 primary face types (Inner Wall, Outer Plating, Bottom Ring, Top Ring).
    - **Winding**: Strict CCW winding for all faces to prevent GPU backface culling.
    - **Geometry**: Explicitly separated the pad radius from the plating radius to ensure the barrel remains thin and rings remain flat disks.
  - *Engine Parameters:* `drill_diameter`, `plating_thickness`, `annular_ring_diameter`, `start_layer: top`, `end_layer: bottom`.

- [x] **2. Blind Via**
  - *What it is:*  A via that starts on an outer layer (top or bottom) but terminates on an inner layer, making it visible from only one side of the board.
  - *3D Representation:*
    - A hollow cylinder starting at Z_outer and terminating exactly at the bottom surface of the target inner copper layer.
    - A solid disk (no hole) at the termination layer, and annular rings (with holes) on the outer layer and any intermediate layers.
  - *Engine Implementation:*
    - **Unified Parametric Interconnect**: Controlled via `top_cap` and `bottom_cap` properties in the hardware script.
    - **Voxel Logic**: Cap thickness is accounted for in the sparse substrate occupancy check.
  - *Engine Parameters:* `drill_diameter`, `plating_thickness`, `start_layer: top/bottom`, `end_layer: inner_N`.

- [x] **3. Buried Via**
  - *What it is:* A via that connects two or more inner layers but does not touch either the top or bottom outer layers. It is completely hidden inside the board.
  - *3D Representation:*
    - A hollow cylinder bounded entirely within the inner dielectric layers.
    - Flat annular rings on the connected inner copper layers.
  - *Engine Implementation:*
    - **Unified Parametric Interconnect**: Uses `top_cap: solid` and `bottom_cap: solid` to seal the inner-layer connections.
  - *Engine Parameters:* `start_layer: inner_A`, `end_layer: inner_B` (where both are ≠ top/bottom).

- [x] **4. Microvia (µVia)**
  - *What it is:* Very small vias (typically <0.15mm diameter) usually drilled with lasers, typically spanning only one layer deep.
  - *3D Representation:*
    - Laser drills naturally produce a slight taper. Model this as a tapered hollow cone (frustum) rather than a perfect cylinder.
    - The top diameter is slightly larger than the bottom diameter.
  - *Engine Implementation:*
    - **Tapered Tube Primitive**: Supports linear interpolation between top and bottom diameters in both voxel grid and mesh generation.
  - *Engine Parameters:* `diameter` (top), `bottom_diameter`, `plating_thickness`, `span: 1 layer`.

- [x] **5. Stacked Vias**
  - *What it is:* Multiple microvias stacked directly on top of each other to span multiple layers in High-Density Interconnect (HDI) boards.
  - *3D Representation:*
    - Concentric, stacked hollow frustums or cylinders sharing the same XY center but spanning adjacent Z-intervals.
  - *Engine Implementation:*
    - **Composite Primitive**: Implemented by instantiating multiple `add contact` primitives at the same XY location with contiguous layer spans.
  - *Engine Parameters:* `array of vias`, `shared_XY_center`.

- [x] **6. Staggered Vias**
  - *What it is:* Microvias on adjacent layers that are offset from each other horizontally, usually connected by a short trace on the intermediate layer.
  - *3D Representation:*
    - Displaced hollow frustums with overlapping outer pads or a connecting horizontal copper track.
  - *Engine Implementation:*
    - **Manifold Interaction**: Verified that offset vias can be connected by standard `add pour` traces on intermediate layers.
  - *Engine Parameters:* `offset_distance`, `rotation_angle`.

- [ ] **7. Filled and Capped Via (Via-in-Pad / VIPPO)**
  - *What it is:* A via positioned directly inside a component landing pad. To prevent solder from flowing down the hole, it is filled and capped.
  - *3D Representation:*
    - A solid cylinder (instead of hollow) representing the fill material (conductive/non-conductive epoxy).
    - Flat, solid copper disks (caps) on the outer layers that completely seal the cylinder ends, making them perfectly flush with the landing pad.
  - *Engine Parameters:* `fill_material_color`, `cap_thickness`.

## List 2: Vias in Relation to the Substrate (Interactions & Clearances)

- [ ] **1. Substrate Drill Void (The Cutout)**
  - *What it is:* The actual physical hole drilled through the FR4 substrate.
  - *3D Representation:*
    - A solid cylinder subtracted from your FR4 substrate mesh.
    - The diameter of this subtraction must equal the `drill_diameter` (before plating), which is larger than the finished via hole size.
  - *Testing Goal:* Ensure that when the FR4 is rendered, you can see clean cylindrical holes passing through it.

- [ ] **2. Antipad (Plane Clearance)**
  - *What it is:* When a via passes through a copper plane (like a ground or power plane) that it is not supposed to connect to, a clearance gap is required to prevent a short circuit.
  - *3D Representation:*
    - A cylinder subtracted from the inner copper plane mesh.
    - This creates a "donut hole" of empty space around the via.
  - *Engine Parameters:* `antipad_diameter` (typically `drill_diameter + 2 * clearance_requirement`).

- [ ] **3. Thermal Relief (Thermal Tie Connection)**
  - *What it is:* When a via does connect to a copper plane, solid copper acts as a heat sink, making soldering difficult. A thermal relief connects the via to the plane using narrow spokes.
  - *3D Representation:*
    - An antipad (clearance ring) is cut into the copper plane, but 2 or 4 solid copper "spokes" (bridges) are left intact, connecting the via's annular ring to the surrounding plane.
  - *Engine Parameters:* `spoke_width`, `spoke_count` (usually 2 or 4), `antipad_diameter`.

- [ ] **4. Solder Mask Opening (Tented vs. Exposed Vias)**
  - *What it is:* Solder mask is the protective outer coating (typically green) on a PCB. Vias can either be covered by it ("tented") or left open ("exposed").
  - *3D Representation:*
    - If Tented: Render the solder mask layer completely flat over the top of the via hole (effectively capping the visual hole with the solder mask color).
    - If Exposed: Subtract a cylinder from the solder mask layer, exposing the copper annular ring and the open hole beneath it.
  - *Engine Parameters:* `is_tented: boolean`, `mask_clearance_diameter`.

- [ ] **5. Non-Functional Pad (NFP) Removal**
  - *What it is:* On inner layers where a via does not make an electrical connection, the circular copper pad is often omitted during manufacturing to save space and reduce capacitance.
  - *3D Representation:*
    - The engine must selectively render annular rings on inner copper layers only if there is an active electrical net connection on that specific layer. If not connected, the via barrel passes through an antipad cutout with no copper pad surrounding it.
  - *Testing Goal:* Verify that inner copper pads dynamically appear or disappear based on the netlist connectivity of that layer.
