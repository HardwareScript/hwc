# Hardware Script MVP Lexicon (Dictionary)

## Purpose

In compiler design, before you can write the Lexer (which chops text into tokens), you must define the exact, exhaustive list of words and symbols the compiler is allowed to understand.

This document is the **Hardware Script MVP Dictionary**. Every word here will become an exact `Token` enum in the Rust `hwc-parser` crate.

## Visual Validation

The syntax highlighting screenshot validates this lexicon - the visual hierarchy makes it completely obvious what the code is doing without even reading the words. The color-coded rendering proves the language design is a winner.

## 1. Action Verbs (The "Doers")

These are the primary commands that trigger the compiler to take a major action.

| Keyword | Purpose | Example |
|---------|---------|---------|
| `import` | Fetch a component, material, or module from standard library or package registry (hpm) | `import FR4 from standard.materials` |
| `define` | Initialize a major block or environment constraint | `define Space "PCB_Board":` |
| `add` | Instantiate a physical object and place it into the 3D tensor grid | `add Resistor (4.7kΩ) named PullUp` |
| `route` | Trigger the routing engine to create an electrical connection | `route MainPower.Plus to Driver.VIN:` |
| `expose` | Export a pin/signal so the module can be imported elsewhere | `expose Driver.Fault as ErrorSignal` |

## 2. Connectors & Prepositions (The "Gluers")

These words make the language read like English. They link actions to data.

| Keyword | Purpose | Example |
|---------|---------|---------|
| `from` | Specify the package or path source for imports | `import FR4 from standard.materials` |
| `named` | Assign a unique, user-defined identifier to a component | `named PowerSource` |
| `at` | Anchor a component to a specific [Z, X, Y] coordinate | `at [1, 10, 10]` |
| `rotated` | Modify the default orientation of a placed component | `rotated 45` or `rotated -30.5` |
| `to` | Specify the destination of a route or end boundary of a spanning region | `spanning [1, 1, 1] to [4, 500, 500]` |
| `by` | Separator for X, Y, Z spatial boundaries | `50mm by 50mm by 4mm` |
| `spanning` | Define a 3D volumetric region (bounding box) | `spanning [1, 1, 1] to [4, 500, 500]` |
| `as` | Rename/alias an exposed signal | `expose Driver.Fault as ErrorSignal` |

## 3. Block Keys (The "Properties")

These act as keys inside indented blocks to define parameters.

| Key | Purpose | Example |
|-----|---------|---------|
| `dimensions:` | Continuous physical measurements (length, width, depth) | `dimensions: 50mm by 50mm by 4mm` |
| `grid:` | Discrete integers defining the resolution grid size | `grid: 500 by 500 by 4` |
| `path:` | Opens an indented YAML-style list of waypoints for manual trace routing | `path:` followed by `- [1, 10, 10]` |

## 4. Rotation System (Arbitrary Angles)

**Design Decision**: Originally considered strict enums (North/South/East/West), but this artificially limits real-world hardware design. LEDs in circular arrays need 30°, 60°, etc.

**Final Implementation**: `rotated` keyword followed by a number (integer or float).

### Syntax Options

```hw
# Integer rotation
add ESP32_C3 named MCU at [1,30,15] rotated 45

# Float rotation (for precise placement)
add LED named Light at [1,10,10] rotated 35.5

# Negative rotation
add Capacitor (100nF) named Decoupling at [1,45,45] rotated -30.5

# Optional 'deg' unit (for clarity)
add Resistor (4.7kΩ) named PullUp at [1,20,15] rotated 90deg
```

### Compiler Behavior

- **Default assumption**: Degrees (no unit required)
- **Lexer rule**: `rotated` keyword → Number (int or float) → Optional `deg` unit
- **Parser rule**: Accept any numeric value (positive, negative, integer, float)

### Physical Engine Challenge

**90-Degree Rotations**: Pins perfectly align with the resolution grid. X becomes Y. Mathematically perfect.

**Arbitrary Rotations**: Pins land on fractional coordinates (e.g., X: 14.33, Y: 18.91).

**Solution**: The compiler uses trigonometry (sine/cosine) to rotate the component's bounding box and pins, then applies a **Nearest-Neighbor Snapping Algorithm** to lock pins to the nearest resolution coordinate so the router can connect traces.

## 5. Punctuation & Symbols (The "Structure")

The strict grammatical rules of the language.

| Symbol | Purpose | Example |
|--------|---------|---------|
| `:` | Opens a new scoped, indented block, or acts as assignment | `define Space "Board":` or `dimensions: 50mm` |
| `[ ]` | Denotes 3D tensor coordinates [Z, X, Y] | `[1, 10, 10]` |
| `( )` | Passes inline parameters/arguments to a component | `Battery (3.7V)` or `Resistor (4.7kΩ)` |
| `-` | Defines an item in an ordered list (path waypoints) | `- [1, 10, 10]` |
| `.` | Dot-notation to access pins on a component instance | `MCU.GPIO_4` or `Driver.VIN` |
| `#` | Single-line human comment (ignored by compiler) | `# This is a comment` |
| `##` | Documentation comment (extracted for auto-docs) | `## Power & Passives` |
| `,` | Separator in lists and coordinates | `[1, 10, 10]` |
| `@` | Prefix for package registry paths | `@robotics/motor` |

## 6. Native Data Types

The compiler natively parses these, instantly separating numbers from units.

### Identifiers
- **Pattern**: Words starting with a letter
- **Usage**: User-defined names
- **Examples**: `IoT_Sensor_Node`, `MCU`, `MainPower`, `Plus`

### Coordinates
- **Pattern**: 1-indexed integers
- **Format**: `[Z,X,Y]` where Z = Layer, X = Column, Y = Row (no spaces after commas)
- **Examples**: `[1,10,10]`, `[3,48,48]`

### Distance Units
- **Supported**: `mm` (millimeters), `cm` (centimeters)
- **Examples**: `50mm`, `4mm`, `2.5cm`

### Electrical Units
- **Voltage**: `V` (Volts), `mV` (Millivolts)
- **Current**: `A` (Amps), `mA` (Milliamps), `µA` or `uA` (Microamps)
- **Resistance**: `Ω` or `Ohm`, with prefixes `kΩ`/`kOhm`, `MΩ`/`MOhm`
- **Capacitance**: `F`, `µF`/`uF`, `nF`, `pF`
- **Inductance**: `H`, `µH`/`uH`, `mH`
- **Frequency**: `Hz`, `kHz`, `MHz`, `GHz`

### Numbers
- **Integers**: `50`, `500`, `4`
- **Floats**: `4.7`, `35.5`, `-30.5`
- **Negative**: `-30.5`, `-45`

### Strings
- **Pattern**: Double-quoted text
- **Examples**: `"Smart_Motor_Driver"`, `"PCB_Board"`

## 7. Rust Token Enum Structure

When we build the Lexer in Rust using the `logos` crate, this dictionary translates directly to:

```rust
use logos::Logos;

#[derive(Logos, Debug, PartialEq)]
pub enum Token {
    // Action Verbs
    #[token("import")] Import,
    #[token("define")] Define,
    #[token("add")] Add,
    #[token("route")] Route,
    #[token("expose")] Expose,
    
    // Connectors & Prepositions
    #[token("from")] From,
    #[token("named")] Named,
    #[token("at")] At,
    #[token("rotated")] Rotated,
    #[token("to")] To,
    #[token("by")] By,
    #[token("spanning")] Spanning,
    #[token("as")] As,
    
    // Block Keys
    #[token("dimensions")] Dimensions,
    #[token("grid")] Grid,
    #[token("path")] Path,
    
    // Punctuation
    #[token(":")] Colon,
    #[token("[")] OpenBracket,
    #[token("]")] CloseBracket,
    #[token("(")] OpenParen,
    #[token(")")] CloseParen,
    #[token("-")] Hyphen,
    #[token(".")] Dot,
    #[token(",")] Comma,
    
    // Data Types
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*")] Identifier,
    #[regex(r"-?\d+(\.\d+)?")] Number,
    #[regex(r#""[^"]*""#)] String,
    
    // Comments
    #[regex(r"##[^\n]*")] DocComment,
    #[regex(r"#[^\n]*")] Comment,
    
    // Whitespace (skip)
    #[regex(r"[ \t\n\f]+", logos::skip)] Whitespace,
    
    // Error
    Error,
}
```

## MVP Completeness

This Dictionary represents the **absolute core of the MVP**. If the parser can:

1. Read these words
2. Understand indentation
3. Build an Abstract Syntax Tree (AST)

...then we have a **working language**.

## Next Steps

With this lexicon locked, the immediate next step is to:

1. Initialize the Rust workspace structure
2. Implement the Lexer using the `logos` crate
3. Validate against the stress test in `LEXER-STRESS-TEST.md`

This dictionary is the foundation for the entire Hardware Script compiler.
