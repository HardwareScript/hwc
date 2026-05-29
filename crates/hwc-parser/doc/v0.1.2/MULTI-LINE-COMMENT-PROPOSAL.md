# Multi-Line Comment Blocks - IMPLEMENTED

**Status**: ✅ IMPLEMENTED  
**Version**: v0.1.3  
**Implemented**: March 17, 2026

---

## Problem Statement

Currently, Hardware Script requires comment symbols on every line:

```hw
# This is a comment
# This is another line
# And another line
```

For multi-line comments or temporarily disabling code blocks, this is tedious and error-prone.

---

## Proposed Solution

Add **block comment syntax** using paired delimiters:

### Single-Line Comments (Current - Keep as is)
```hw
# Regular comment - ignored by compiler
## Documentation comment - extracted for docs
```

### Multi-Line Comment Blocks (NEW)
```hw
#[
This is a multi-line comment block.
Everything between #[ and ]# is ignored.
No need to add # on every line!
]#
```

### Multi-Line Documentation Blocks (NEW)
```hw
##[
This is a multi-line documentation block.
Everything between ##[ and ]## is extracted for documentation.
Perfect for detailed component descriptions!
]##
```

---

## Syntax Specification

### Comment Block
- **Start**: `#[` (must be on its own line or with leading whitespace)
- **End**: `]#` (must be on its own line or with trailing whitespace)
- **Behavior**: Everything between delimiters is ignored by compiler
- **Nesting**: Not allowed (keeps lexer simple)
- **Whitespace Rule**: There must be whitespace (space, tab, or newline) after `#[` and before `]#`

### Documentation Block
- **Start**: `##[` (must be on its own line or with leading whitespace)
- **End**: `]##` (must be on its own line or with trailing whitespace)
- **Behavior**: Content is extracted and attached to next AST node
- **Nesting**: Not allowed
- **Whitespace Rule**: There must be whitespace (space, tab, or newline) after `##[` and before `]##`

### Whitespace Requirements

The whitespace requirement prevents ambiguity and ensures clean syntax:

**Valid** (with whitespace):
```hw
#[ This is a comment ]#

#[
Multi-line comment
with proper spacing
]#

##[ Documentation block ]##

##[
Multi-line documentation
with proper spacing
]##
```

**Invalid** (no whitespace - would be treated as text or error):
```hw
#[This has no space after opening]#
#[ This has no space before closing]#
##[No space after opening]##
##[No space before closing]##
```

This rule ensures:
1. Clear visual separation between delimiters and content
2. No ambiguity with potential future syntax
3. Consistent, readable code style
4. Easier lexer implementation (simpler regex patterns)

---

## Use Cases

### 1. Temporarily Disable Code
```hw
define Space "Test":
  dimensions: 50mm by 50mm by 4mm
  grid: 500 by 500 by 4
  
  add Battery named Power at [1,5,5]
  
  #[
  # Temporarily disabled for testing
  add LED named Light at [1,8,8]
  route Power.Plus to Light.Anode:
    path:
      - [1,5,5]
      - [1,8,8]
  ]#
  
  expose Power.Plus as VCC
```

### 2. Multi-Line Documentation
```hw
##[
Advanced Motor Driver Module

This module provides a complete motor control solution with:
- 12V LiPo battery power
- Overcurrent protection via 4.7kΩ pull-up
- Decoupling capacitor for noise reduction
- Fault detection exposed as ErrorSignal

Usage:
  import MotorDriver from @myproject/drivers
  add MotorDriver named Motor at [1,100,100]
]##
define Space "Smart_Motor_Driver":
  dimensions: 50mm by 50mm by 4mm
  grid: 500 by 500 by 4
  # ... rest of definition
```

### 3. Section Comments
```hw
define Space "Complex_Board":
  dimensions: 100mm by 100mm by 4mm
  grid: 1000 by 1000 by 4
  
  ##[
  Power Supply Section
  Provides regulated 5V and 3.3V rails
  ]##
  add Battery (12V) named MainPower at [1,50,50]
  add Regulator_5V named Reg5V at [1,100,50]
  add Regulator_3V3 named Reg3V3 at [1,150,50]
  
  #[
  TODO: Add sensor section here
  - Temperature sensor
  - Humidity sensor
  - Pressure sensor
  ]#
  
  ##[
  Communication Section
  UART and I2C interfaces
  ]##
  add UART named Serial at [1,50,200]
  add I2C named Bus at [1,100,200]
```

---

## Implementation Notes

### Lexer Changes
1. Add new token types:
   - `BlockCommentStart` (`#[`)
   - `BlockCommentEnd` (`]#`)
   - `DocBlockStart` (`##[`)
   - `DocBlockEnd` (`]##`)

2. Add lexer mode for block comments:
   - When `#[` followed by whitespace is encountered, enter "comment block mode"
   - Consume all tokens until whitespace + `]#` is found
   - Emit single `BlockComment(String)` token with content

3. Add lexer mode for doc blocks:
   - When `##[` followed by whitespace is encountered, enter "doc block mode"
   - Consume all tokens until whitespace + `]##` is found
   - Emit single `DocBlock(String)` token with content

4. Regex patterns (with whitespace enforcement):
   ```rust
   // Block comment: #[ ... ]#
   #[regex(r"#\[\s", |lex| {
       let start = lex.span().end;
       let remaining = &lex.source()[start..];
       if let Some(end_pos) = remaining.find(r"\s\]#") {
           // Extract content between delimiters
           let content = &remaining[..end_pos];
           // Advance lexer past the closing delimiter
           // Return BlockComment token
       }
   })]
   BlockComment(String),
   
   // Doc block: ##[ ... ]##
   #[regex(r"##\[\s", |lex| {
       let start = lex.span().end;
       let remaining = &lex.source()[start..];
       if let Some(end_pos) = remaining.find(r"\s\]##") {
           // Extract content between delimiters
           let content = &remaining[..end_pos];
           // Advance lexer past the closing delimiter
           // Return DocBlock token
       }
   })]
   DocBlock(String),
   ```

### Parser Changes
1. Skip `BlockComment` tokens (like regular comments)
2. Collect `DocBlock` tokens (like doc comments)
3. Attach doc blocks to next AST node

### Error Handling
1. Unclosed block comment: Error with helpful message
2. Nested block comments: Error (not supported)
3. Mismatched delimiters: Error with suggestion

---

## Examples

### Before (Current Syntax)
```hw
# Power supply configuration
# Uses 12V LiPo battery
# Regulated to 5V for logic
# Decoupling capacitor on output
add Battery (12V) named Power at [1,10,10]
add Regulator (5V) named Reg at [1,20,10]
add Capacitor (100nF) named Cap at [1,30,10]
```

### After (With Block Comments)
```hw
#[
Power supply configuration
Uses 12V LiPo battery
Regulated to 5V for logic
Decoupling capacitor on output
]#
add Battery (12V) named Power at [1,10,10]
add Regulator (5V) named Reg at [1,20,10]
add Capacitor (100nF) named Cap at [1,30,10]
```

### Documentation Block Example
```hw
##[
Battery Component

Provides main power for the circuit.
Specifications:
- Voltage: 12V nominal
- Chemistry: LiPo
- Capacity: 2200mAh
- Discharge rate: 20C

Safety:
- Always use with protection circuit
- Monitor cell voltage during discharge
]##
add Battery (12V) named Power at [1,10,10]
```

---

## Advantages

1. **Better DX**: No need to add `#` on every line
2. **Easier Code Disabling**: Comment out entire sections quickly
3. **Rich Documentation**: Multi-line docs without line noise
4. **IDE Support**: Easier to implement block folding
5. **Familiar Syntax**: Similar to Rust's `/* */` and doc comments

---

## Backward Compatibility

- All existing single-line comments (`#` and `##`) continue to work
- No breaking changes to existing `.hw` files
- Block comments are purely additive feature

---

## Future Enhancements

### Nested Block Comments (Optional)
If needed in future, could support nesting:
```hw
#[
Outer comment
  #[
  Inner comment
  ]#
Still in outer comment
]#
```

### Inline Block Comments (Optional)
```hw
add Battery #[ temporary ]# named Power at [1,10,10]
```

---

## Decision

**Recommendation**: Implement in v0.2.0 after core parser is stable.

**Priority**: Medium (nice-to-have, not critical for MVP)

**Effort**: Low (2-3 hours of implementation + testing)

---

## Implementation Checklist (When Ready)

- [x] Update lexer with block comment tokens
- [x] Add lexer modes for block comment/doc parsing
- [x] Update parser to handle block tokens
- [x] Add tests for block comments
- [x] Add tests for block documentation
- [x] Add tests for error cases (unclosed, nested)
- [ ] Update grammar specification
- [ ] Update language spec documentation
- [ ] Update syntax highlighting spec
- [ ] Add examples to documentation

---

**Note**: Multi-line comments are now fully implemented and tested in v0.1.3!
