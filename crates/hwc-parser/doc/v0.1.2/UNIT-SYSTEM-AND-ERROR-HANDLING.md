# Hardware Script: Strict Unit Lexicon & Error Handling

**Reference Document for Lexer & Parser Implementation (v1.0 Strict Baseline)**

## Philosophy

Hardware Script is designed to be highly optimized, deterministic, and fast. To prevent compiler bloat and ensure absolute consistency across all projects, the Lexer enforces **Strict Unit Notation**.

For any given unit, there are exactly **two allowed inputs**:

1. **The Actual Symbol** (e.g., `4.7kΩ`, `10µF`)
2. **The ONE Canonical Keyboard-Friendly Alias** (e.g., `4.7kOhm`, `10uF`)

All other shorthands (like SPICE's `4.7k` or IEC's `4K7`) are **strictly rejected** by the compiler to maintain a single, readable standard.

## The "One True Pair" Unit Table

| Quantity      | Base | Allowed Symbol | Allowed Keyboard Alias | Examples (Strictly Enforced)    |
|---------------|------|----------------|------------------------|---------------------------------|
| Resistance    | Ω    | Ω              | Ohm                    | `4.7kΩ` or `4.7kOhm`           |
| Capacitance   | F    | µF             | uF                     | `100µF` or `100uF`             |
| Inductance    | H    | µH             | uH                     | `2.2µH` or `2.2uH`             |
| Voltage       | V    | V              | V                      | `3.3V`, `500mV`                |
| Current       | A    | µA             | uA                     | `20mA`, `50µA` or `50uA`       |
| Frequency     | Hz   | Hz             | Hz                     | `60Hz`, `400kHz`, `2.4GHz`     |
| Time          | s    | µs             | us                     | `10ns`, `5µs` or `5us`         |
| Temperature   | °C   | °C             | C                      | `85°C` or `85C`                |
| Angle         | rad  | °              | deg                    | `90°` or `90deg`               |

### Strict Prefix Rules

The compiler natively supports standard SI prefixes: `f`, `p`, `n`, `u`/`µ`, `m`, `k`, `M`, `G`, `T`.

- The symbol `µ` and the letter `u` are the **only permitted variations** for the micro prefix
- `k` is **strictly lowercase** (kilo)
- `M` is **strictly uppercase** (Mega)

## Hint-Efficient Error Handling

If a user attempts to use an unsupported shorthand (like IEC 60062 or SPICE), the compiler will not crash blindly. Hardware Script features a **world-class, hint-efficient diagnostic engine** (built on Rust's `miette` crate) that immediately corrects the user and links to the documentation.

### Example A: User tries to use IEC 60062 shorthand (4K7)

**User Code:**
```hw
add Resistor (4K7) named PullUp at [1, 10, 10]
```

**Compiler Output:**
```
❌ Syntax Error: Unrecognized unit format '4K7'
  ╭─[main.hw:1:15]
1 │ add Resistor (4K7) named PullUp at [1, 10, 10]
  ·               ─┬─
  ·                ╰── Invalid unit formatting
  ╰────

Help: Hardware Script uses strict SI notation to ensure readability.
      Suggestion: Change '4K7' to '4.7kOhm' or '4.7kΩ'.
      Docs: Run `hpm doc read units` for the strict unit table.
```

### Example B: User tries to use SPICE shorthand (100n)

**User Code:**
```hw
add Capacitor (100n) named Decoupling at [1, 20, 20]
```

**Compiler Output:**
```
❌ Syntax Error: Missing base unit in '100n'
  ╭─[main.hw:2:16]
2 │ add Capacitor (100n) named Decoupling at [1, 20, 20]
  ·                ──┬─
  ·                  ╰── Prefix 'n' requires a base unit (e.g., F, s, H)
  ╰────

Help: SPICE-style implied units are not permitted.
      Suggestion: If this is a Capacitor, change '100n' to '100nF'.
      Docs: Run `hpm doc read units` for the strict unit table.
```

## Why This is the Right Decision

By enforcing strict unit notation, we achieve:

1. **Simplified Lexer Logic** - No need for ~50 complex regular expressions
2. **Lightning-Fast Compilation** - Simple pattern matching: Number → Prefix → Base Unit
3. **Community Consistency** - Everyone writes identical, readable code
4. **Clear Error Messages** - Users get immediate, actionable feedback

### Lexer Implementation Strategy

The Lexer logic is incredibly simple:

1. Look for a **Number** (e.g., `4.7`)
2. Look for an optional **Prefix** (e.g., `k`)
3. Look for the exact **Allowed Base Unit or Alias** (e.g., `Ohm` or `Ω`)

If it doesn't match that exact sequence perfectly, it throws the beautiful error message shown above.

This makes the compiler lightning fast and forces the entire community to write clean, identical code.

## Integration with Documentation System

All error messages reference the documentation system via `hpm doc read units`, ensuring users can quickly learn the correct syntax without leaving their terminal.

## Summary

This strict unit system is a **core architectural decision** that:

- Eliminates ambiguity
- Prevents compiler bloat
- Ensures readability across all Hardware Script projects
- Provides world-class error messages that teach users the correct syntax

This document serves as the definitive reference for unit handling in the lexer and parser implementation.
