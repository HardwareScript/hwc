# Hardware Script Grammar Files

This directory contains the grammar specification for the Hardware Script language.

## Files

### `hardware.grammar`
The **single source of truth** for Hardware Script syntax. This file documents:
- Complete language grammar in BNF-like notation
- All keywords, operators, and punctuation
- Syntax rules and conventions
- Unit system specification
- Example code

**When to update**: Before implementing any new syntax feature, update this file first.

### `hardware.pest` (deprecated)
The old Pest grammar file. This is kept for reference but is no longer used.
The new implementation uses Logos (lexer) + hand-written recursive descent parser.

## Grammar Notation

The grammar uses a simplified BNF notation:

```
rule ::= definition          # Basic rule
rule ::= option1 | option2   # Alternatives
rule ::= item*               # Zero or more
rule ::= item+               # One or more
rule ::= item?               # Optional
[a-z]                        # Character class (regex)
"keyword"                    # Literal token
```

## Workflow for Adding New Syntax

1. **Update `hardware.grammar`**
   - Add the new syntax rule
   - Document any new keywords or tokens
   - Add an example in the comments

2. **Update `src/lexer.rs`**
   - Add new tokens to the `Token` enum
   - Add Logos patterns for new keywords/operators
   - Update tests in `src/lexer_tests.rs`

3. **Update `src/parser.rs`**
   - Add new parsing methods for the syntax
   - Update AST nodes in `src/ast.rs` if needed
   - Add tests

4. **Update documentation**
   - Update `doc/v0.1.3/PARSER-SPECIFICATION.md` if needed
   - Add examples to test suite

## Example: Adding a New Keyword

Let's say we want to add a `connect` keyword as an alias for `route`.

### Step 1: Update `hardware.grammar`

```diff
# ROUTING
route_definition ::= 
-   "route" pin_reference "to" pin_reference ":"
+   ("route" | "connect") pin_reference "to" pin_reference ":"
    INDENT
        "path" ":"
        ...
```

### Step 2: Update `src/lexer.rs`

```rust
#[derive(Logos, Debug, Clone, PartialEq)]
pub enum Token {
    // ... existing tokens ...
    
    #[token("route")]
    Route,
    
    #[token("connect")]  // Add new token
    Connect,
    
    // ... rest of tokens ...
}
```

### Step 3: Update `src/parser.rs`

```rust
fn parse_route(&mut self) -> Result<Route, ParseError> {
    // Accept either "route" or "connect"
    if !self.check(&Token::Route) && !self.check(&Token::Connect) {
        return Err(self.error("Expected 'route' or 'connect'"));
    }
    self.advance();
    
    // ... rest of parsing logic ...
}
```

### Step 4: Add tests

```rust
#[test]
fn test_connect_keyword() {
    let source = "connect Power.Plus to LED.Anode:";
    // ... test implementation ...
}
```

## Grammar Philosophy

The Hardware Script grammar follows these principles:

1. **Readability First**: Code should read like natural English
2. **Consistency**: Similar concepts use similar syntax
3. **Explicitness**: No implicit behavior or magic
4. **Strictness**: One correct way to write things (enforced by formatter)
5. **Extensibility**: Easy to add new features without breaking existing code

## References

- **v0.1.3 Specification**: `doc/v0.1.3/PARSER-SPECIFICATION.md`
- **MVP Lexicon**: `doc/v0.1.2/MVP-LEXICON.md`
- **Syntax Guide**: `doc/v0.1.2/SYNTAX-AND-STYLE-GUIDE.md`
- **Unit System**: `doc/v0.1.2/UNIT-SYSTEM-AND-ERROR-HANDLING.md`
- **Stress Test**: `doc/v0.1.2/LEXER-STRESS-TEST.md`

## Maintenance

This grammar file should be:
- ✅ Updated before implementing new features
- ✅ Kept in sync with the lexer and parser
- ✅ Referenced in code reviews
- ✅ Used as documentation for users
- ✅ Validated against test examples

The grammar is the contract between the language designer and the implementer.
Keep it accurate, complete, and up-to-date!
