# Hardware Script Parser Specification v0.1.3

**Version**: 0.1.3 (Implementation Ready)  
**Status**: Consolidated - Authoritative for Implementation  
**Previous Version**: [v0.1.2 Documentation](../v0.1.2/)

## Purpose

This document consolidates the parser specification from v0.1.2 and resolves any conflicts or ambiguities. It serves as the authoritative reference for implementing the Hardware Script lexer and parser.

## Documentation Structure

The complete specification is documented in v0.1.2. This document only highlights:
1. Final decisions where alternatives were considered
2. Implementation priorities
3. Cross-references to detailed documentation

## Key Design Decisions

### 1. Rotation System (RESOLVED)

**Decision**: Arbitrary numeric angles (integers and floats, positive and negative)

**Previous Consideration**: v0.1.2 notes mentioned strict enums (North/South/East/West at 0°/90°/180°/270°), but this was rejected as it artificially limits real-world hardware design.

**Final Syntax**:
```hw
rotated 45        # Integer
rotated -30.5     # Float (negative)
rotated 90deg     # Optional 'deg' unit
```

**Rationale**: LEDs in circular arrays need 30°, 60°, etc. Components in tight enclosures need precise angles. The physical engine will handle pin snapping to the voxel grid.

**Full Details**: See [v0.1.2/MVP-LEXICON.md](../v0.1.2/MVP-LEXICON.md#4-rotation-system-arbitrary-angles)

### 2. Whitespace After Prepositions (CONFIRMED)

**Decision**: Space required after prepositions (`at`, `to`, `by`, `from`, `spanning`, `rotated`)

**Syntax**:
```hw
at [1,10,10]        # Correct
at[1,10,10]         # Lexer accepts, formatter corrects
```

**Rationale**: Maintains Ruby-like readability. Prevents C++-style array indexing appearance.

**Full Details**: See [v0.1.2/SYNTAX-AND-STYLE-GUIDE.md](../v0.1.2/SYNTAX-AND-STYLE-GUIDE.md#the-space-after-prepositions-rule)

### 3. Compact Coordinates (CONFIRMED)

**Decision**: No spaces after commas inside coordinate brackets

**Syntax**:
```hw
[1,10,10]           # Correct - compact
[1, 10, 10]         # NO - unnecessary spaces
```

**Rationale**: Comma provides sufficient visual separation. Keeps syntax tight and reduces visual noise.

**Full Details**: See [v0.1.2/SYNTAX-AND-STYLE-GUIDE.md](../v0.1.2/SYNTAX-AND-STYLE-GUIDE.md#coordinate-formatting)

### 4. Strict Unit System (CONFIRMED)

**Decision**: Exactly two allowed formats per unit - symbol and keyboard alias

**Examples**:
- Resistance: `4.7kΩ` or `4.7kOhm` (ONLY)
- Capacitance: `100µF` or `100uF` (ONLY)

**Rejected**: SPICE notation (`4.7k`), IEC 60062 (`4K7`)

**Rationale**: Eliminates ~50 regex patterns, ensures community consistency, enables beautiful error messages.

**Full Details**: See [v0.1.2/UNIT-SYSTEM-AND-ERROR-HANDLING.md](../v0.1.2/UNIT-SYSTEM-AND-ERROR-HANDLING.md)

## Complete Specification Reference

All detailed specifications are in v0.1.2:

| Topic | Document | Status |
|-------|----------|--------|
| Token Dictionary | [MVP-LEXICON.md](../v0.1.2/MVP-LEXICON.md) | ✅ Authoritative |
| Syntax & Style | [SYNTAX-AND-STYLE-GUIDE.md](../v0.1.2/SYNTAX-AND-STYLE-GUIDE.md) | ✅ Authoritative |
| Unit System | [UNIT-SYSTEM-AND-ERROR-HANDLING.md](../v0.1.2/UNIT-SYSTEM-AND-ERROR-HANDLING.md) | ✅ Authoritative |
| Test Suite | [LEXER-STRESS-TEST.md](../v0.1.2/LEXER-STRESS-TEST.md) | ✅ Authoritative |
| IDE Integration | [SYNTAX-HIGHLIGHTING-SPECIFICATION.md](../v0.1.2/SYNTAX-HIGHLIGHTING-SPECIFICATION.md) | ✅ Authoritative |

## Implementation Roadmap

### Phase 1: Basic Lexer (Priority 1)
1. Implement `Token` enum with `logos` crate
2. Handle all keywords from [MVP-LEXICON.md](../v0.1.2/MVP-LEXICON.md#1-action-verbs-the-doers)
3. Parse numbers, strings, identifiers
4. Handle punctuation

**Validation**: Must tokenize the stress test in [LEXER-STRESS-TEST.md](../v0.1.2/LEXER-STRESS-TEST.md)

### Phase 2: Unit System (Priority 2)
1. Extend lexer for unit suffixes
2. Implement strict validation per [UNIT-SYSTEM-AND-ERROR-HANDLING.md](../v0.1.2/UNIT-SYSTEM-AND-ERROR-HANDLING.md#the-one-true-pair-unit-table)
3. Add `miette`-based error messages

### Phase 3: Indentation (Priority 3)
1. Track indentation levels
2. Emit INDENT/DEDENT tokens
3. Handle mixed tabs/spaces errors

### Phase 4: Parser (Priority 4)
1. Build AST from token stream
2. Implement semantic validation
3. Generate IR for compiler

## Quick Reference: Token Categories

From [MVP-LEXICON.md](../v0.1.2/MVP-LEXICON.md):

- **Action Verbs**: `import`, `define`, `add`, `route`, `expose`
- **Connectors**: `from`, `named`, `at`, `rotated`, `to`, `by`, `spanning`, `as`
- **Block Keys**: `dimensions`, `grid`, `path`
- **Punctuation**: `:`, `[]`, `()`, `-`, `.`, `,`, `#`, `##`, `@`

## Canonical Example

The complete stress test example is in [LEXER-STRESS-TEST.md](../v0.1.2/LEXER-STRESS-TEST.md#the-complete-test-script).

Key features demonstrated:
- Arbitrary rotation: `rotated 90`, `rotated -30.5`, `rotated 45`
- Unit system: `(12V)`, `(4.7kΩ)`, `(100nF)`
- Module system: `expose Driver.Fault as ErrorSignal`
- Multi-layer routing with vias

## Implementation Notes

### Rust Dependencies
```toml
[dependencies]
logos = "0.13"      # Lexer generation
miette = "5.0"      # Error reporting
```

### Token Enum Structure
See [MVP-LEXICON.md](../v0.1.2/MVP-LEXICON.md#7-rust-token-enum-structure) for complete implementation.

### Error Handling
See [UNIT-SYSTEM-AND-ERROR-HANDLING.md](../v0.1.2/UNIT-SYSTEM-AND-ERROR-HANDLING.md#hint-efficient-error-handling) for error message examples.

### Syntax Highlighting
See [SYNTAX-HIGHLIGHTING-SPECIFICATION.md](../v0.1.2/SYNTAX-HIGHLIGHTING-SPECIFICATION.md) for IDE extension implementation.

## Summary

This specification is locked and ready for implementation. All design decisions have been finalized:

- ✅ Arbitrary rotation angles (not strict enums)
- ✅ Strict unit system (symbol + keyboard alias only)
- ✅ Ruby-inspired whitespace rules
- ✅ Complete token dictionary
- ✅ Comprehensive test suite

Refer to v0.1.2 documents for complete implementation details.
