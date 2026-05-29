# Hardware Script: Lexer Stress Test & Token Specification

## Purpose

This document contains the ultimate, holistic test script designed to act as our **"Lexer Stress Test."** It includes absolutely everything we've defined and serves as the canonical reference for lexer implementation.

## The Complete Test Script

```hw
## Advanced Motor Driver Module
## Demonstrates arbitrary rotation and multi-layer routing

import FR4 from standard.materials
import MotorController from @robotics/motor
import PassiveComponents from standard.passives

define Space "Smart_Motor_Driver":
  dimensions: 50mm by 50mm by 4mm
  grid: 500 by 500 by 4

  # 4-Layer Board Substrate
  add Substrate(FR4) spanning [1,1,1] to [4,500,500]

  ## Power & Passives
  add Battery_LiPo (12V) named MainPower at [1,10,10]
  add Resistor (4.7kΩ) named PullUp at [1,20,15] rotated 90
  add Capacitor (100nF) named Decoupling at [1,45,45] rotated -30.5

  # Main Driver Chip (Mounted diagonally for tight circular enclosures)
  add MotorController named Driver at [1,50,50] rotated 45

  # Routing with precise 3D waypoints
  route MainPower.Plus to Driver.VIN:
    path:
      - [1,10,10]
      - [1,10,50]
      - [1,40,50]

  # Multi-layer signal routing (Vias)
  route PullUp.Pin2 to Driver.Enable:
    path:
      - [1,20,15]
      - [3,20,15]  # Via down to inner layer 3
      - [3,48,48]  # Route on inner layer
      - [1,48,48]  # Via back to surface

  ## Expose pins so this entire board can be imported as a module
  expose Driver.Fault as ErrorSignal
```

## Lexer Feature Coverage

This test script exercises every critical lexer feature:

### 1. Arbitrary Rotations
- **Positive integers**: `rotated 90`
- **Negative floats**: `rotated -30.5`
- **Standard numbers**: `rotated 45`

### 2. Parameters with Mixed Units
- Parentheses with units: `(12V)`, `(4.7kΩ)`, `(100nF)`
- Must split into: `(Number, Unit)` pairs

### 3. Module System Keywords
- `expose` and `as` keywords for modular hardware design
- Allows boards to be imported elsewhere like software modules

### 4. All Punctuation
- Hyphenated lists: `- [1, 10, 10]`
- Dot-notation: `Driver.VIN`
- Square brackets: `[1, 20, 15]`
- Colons: `path:`
- Commas: `[1, 2, 3]`

### 5. Indentation Tracking
- Lexer must emit `INDENT` tokens when indentation increases
- Lexer must emit `DEDENT` tokens when indentation decreases
- Critical for Python-style block structure

## Detailed Token Breakdown

### Floating Point & Negatives

**Input:** `rotated -30.5`

**Lexer Requirements:**
- Regex must explicitly allow optional `-` sign
- Regex must allow optional `.5` decimal portion
- Must handle both integers and floats

**Token Stream:**
```
[Token::Rotated] [Token::Number(-30.5)]
```

### Glued Units vs. Raw Numbers

**Input:** `12V` vs `45`

**Lexer Requirements:**
- `12V` → Split into `(Number: 12, Unit: Volts)`
- `45` → Just `(Number: 45)`

**Token Streams:**
```
12V  → [Token::NumberWithUnit(12.0, Unit::Volts)]
45   → [Token::Number(45.0)]
```

### Special Characters in Units

**Input:** `4.7kΩ`

**Lexer Requirements:**
- Must correctly process Unicode characters like `Ω` (Omega)
- Must not panic on non-ASCII characters
- Must handle both `Ω` and `Ohm` alias

**Token Stream:**
```
[Token::NumberWithUnit(4.7, Unit::Resistance(Prefix::Kilo))]
```

### Dot-Notation

**Input:** `Driver.VIN`

**Lexer Requirements:**
- Must tokenize as three separate tokens
- Dot is punctuation, not part of identifier

**Token Stream:**
```
[Token::Identifier("Driver")] [Token::Dot] [Token::Identifier("VIN")]
```

### Indentation Tracking

**Input:**
```hw
define Space "Smart_Motor_Driver":
  dimensions: 50mm by 50mm by 4mm
  grid: 500 by 500 by 4
```

**Lexer Requirements:**
- Count spaces before `dimensions:` and emit `INDENT`
- When returning to left edge, emit `DEDENT`
- Track indentation level throughout file

**Token Stream:**
```
[Token::Define] [Token::Space] [Token::String("Smart_Motor_Driver")] [Token::Colon]
[Token::Indent]
[Token::Identifier("dimensions")] [Token::Colon] ...
[Token::Identifier("grid")] [Token::Colon] ...
[Token::Dedent]
```

### New Module System Keywords

**Input:** `expose Driver.Fault as ErrorSignal`

**Lexer Requirements:**
- Recognize `expose` and `as` as keywords
- Allow modular hardware design patterns

**Token Stream:**
```
[Token::Expose] [Token::Identifier("Driver")] [Token::Dot] 
[Token::Identifier("Fault")] [Token::As] [Token::Identifier("ErrorSignal")]
```

## Visual Rhythm Analysis

The syntax maintains a consistent visual rhythm:

- **Keywords** are lowercase and spaced: `add`, `route`, `expose`
- **Types** are PascalCase: `Battery_LiPo`, `MotorController`
- **Instances** are PascalCase: `MainPower`, `Driver`
- **Properties** are PascalCase: `Plus`, `VIN`, `Fault`
- **Units** are glued to numbers: `12V`, `4.7kΩ`, `100nF`
- **Coordinates** are bracketed: `[1, 10, 10]`
- **Comments** use `#` for single-line, `##` for section headers

## Implementation Checklist

When implementing the Rust lexer, ensure it handles:

- [ ] Floating point numbers with optional negative sign
- [ ] Unicode characters in unit symbols (Ω, µ, °)
- [ ] Number-unit pairs without spaces (`12V`)
- [ ] Dot-notation for property access
- [ ] Indentation tracking (INDENT/DEDENT tokens)
- [ ] All keywords: `import`, `from`, `define`, `add`, `named`, `at`, `spanning`, `to`, `by`, `rotated`, `route`, `path`, `expose`, `as`
- [ ] All punctuation: `()`, `[]`, `:`, `,`, `.`, `-`, `#`, `##`
- [ ] String literals with quotes: `"Smart_Motor_Driver"`
- [ ] Module paths with `@` prefix: `@robotics/motor`
- [ ] Dot-separated paths: `standard.materials`

## Success Criteria

The lexer successfully passes this stress test when it can:

1. Tokenize the entire script without panicking
2. Correctly identify all keywords, identifiers, and literals
3. Properly handle Unicode unit symbols
4. Track indentation levels accurately
5. Split number-unit pairs correctly
6. Preserve source location information for error reporting

This test script represents the **complete surface area** of the Hardware Script lexer and serves as the definitive validation benchmark.
