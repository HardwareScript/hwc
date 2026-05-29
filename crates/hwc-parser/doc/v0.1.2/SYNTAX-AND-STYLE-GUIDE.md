# Hardware Script Syntax and Style Guide

## Philosophy: Ruby-Inspired Readability

Hardware Script follows the Ruby philosophy of making code read like beautiful, natural English sentences. Every syntactic choice prioritizes human readability over compiler convenience.

## Whitespace Rules

### The Space After Prepositions Rule

Prepositions in Hardware Script (`at`, `to`, `by`, `from`) must be followed by a space before their arguments. This maintains the natural English sentence flow.

**Correct:**
```hw
add Battery_LiPo (12V) named MainPower at [1,10,10]
spanning [1,1,1] to [4,500,500]
dimensions: 50mm by 50mm by 4mm
```

**Incorrect (looks like C++ array indexing):**
```hw
add Battery at[1,10,10]        // NO - glued preposition
spanning[1,1,1] to[4,500,500]  // NO - breaks readability
```

### How the Lexer Handles Spaces

The Rust-based lexer (using `logos`) treats whitespace as token separators. These two lines are mathematically identical to the compiler:

```hw
add Battery at[1, 10, 10]
add Battery at [1, 10, 10]
```

The lexer tokenizes both as:
```
[Token::Add] [Token::Identifier("Battery")] [Token::At] [Token::OpenBracket] ...
```

However, **human readability is our #1 priority**, so the style guide enforces the spaced version.

## The Formatter: `hws fmt`

Just like Rust has `rustfmt` and JavaScript has Prettier, Hardware Script will include a built-in formatter.

### Auto-Correction Examples

If a user accidentally writes:
```hw
spanning[1, 1, 1] to[4, 500, 500]
```

Running `hws fmt` will automatically correct it to:
```hw
spanning [1, 1, 1] to [4, 500, 500]
```

Similarly, routing paths will be cleaned and properly spaced:
```hw
route MainPower.Plus to Driver.VIN:
  path:
    - [1, 10, 10]
    - [1, 10, 50]
```

This ensures the syntax remains light, airy, and Ruby-esque across all codebases.

## The Canonical Visual Reference

This is the absolute standard for Hardware Script syntax. This is the visual baseline the parser is built for:

```hw
## Advanced Motor Driver Module
## Demonstrates arbitrary rotation and multi-layer routing

import FR4 from standard.materials
import MotorController from @robotics/motor

define Space "Smart_Motor_Driver":
  dimensions: 50mm by 50mm by 4mm
  grid: 500 by 500 by 4

  # 4-Layer Board Substrate
  add Substrate(FR4) spanning [1,1,1] to [4,500,500]

  ## Power & Passives
  add Battery_LiPo (12V) named MainPower at [1,10,10]
  add Resistor (4.7kOhm) named PullUp at [1,20,15] rotated 90
  add Capacitor (100nF) named Decoupling at [1,45,45] rotated -30.5

  # Main Driver Chip
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

## Design Goals

This syntax is designed so that:

1. **A software engineer who has never touched hardware** can instantly understand what's happening
2. **The code reads like natural English**, not like C++ or assembly
3. **Whitespace is meaningful to humans**, even if the compiler ignores it
4. **The formatter enforces consistency** across all projects

## Unit Aliases and Spacing

All unit specifications follow the same spacing rules:

```hw
dimensions: 50mm by 50mm by 4mm    # Spaced prepositions
grid: 500 by 500 by 4              # Consistent spacing
add Resistor (4.7kOhm)             # No space before parentheses
rotated 90                         # Space before numeric arguments
at [1,10,10]                       # No spaces inside coordinate brackets
```

## Coordinate Formatting

Coordinates use **compact notation without spaces after commas**:

```hw
# Correct - compact coordinates
at [1,10,10]
spanning [1,1,1] to [4,500,500]
- [3,48,48]

# Incorrect - unnecessary spaces
at [1, 10, 10]        // NO
spanning [1, 1, 1]    // NO
```

**Rationale**: The comma already provides sufficient visual separation. Compact coordinates keep the syntax tight and reduce visual noise.

## Summary

Hardware Script's syntax is **stunningly clean**. Every design decision prioritizes readability and natural language flow. The formatter (`hws fmt`) ensures this standard is maintained automatically across all codebases.

This document serves as the definitive reference for parser implementation and style enforcement.
