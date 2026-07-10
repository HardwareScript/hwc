---
name: Language Syntax Proposal
about: Suggest a new feature or syntax for the .hw language
title: "[SYNTAX] "
labels: language
assignees: ''
---

## Summary

A clear and concise description of your syntax proposal.

## Where Does This Belong?

Hardware Script follows a **C philosophy** — the compiler stays as lightweight as possible. Advanced features belong in libraries, not the core.

- [ ] **Compiler core**: This is a fundamental language primitive (e.g., `component`, `space`, `route`, `logic:`)
- [ ] **Standard Library (`@std`)**: This is an irreducible baseline primitive that ships with the compiler
- [ ] **HPM Package**: This is a domain-specific feature that should be a community package
- [ ] **Not needed**: This can be built with existing primitives

**If your proposal is for an HPM package, please close this issue and build it as a package instead.** The compiler only needs primitives; the ecosystem provides everything else.

## Motivation

Why is this feature needed? What problem does it solve for hardware designers?

## Proposed Syntax

Show the proposed syntax with a concrete example:

```hw
# Example using the proposed syntax
space Example:
    dimensions: 10mm by 10mm by 2.0mm
    
    # Your proposed syntax here
```

## Design Principles

How does this proposal align with Hardware Script's core principles?

- [ ] **Zero-magic**: No hidden behavior or implicit conversions
- [ ] **Python/Ruby-style aesthetic**: Clean, readable, expressive
- [ ] **Deterministic**: Same input always produces same output
- [ ] **Scale-invariant**: Works for PCBs and silicon alike
- [ ] **AI-native**: Easy for LLMs to generate and understand

## Alternatives Considered

What alternative approaches did you consider? Why is this proposal better?

## Impact Assessment

- **Parser**: [e.g., requires new keywords, modifies existing grammar]
- **Compiler**: [e.g., new AST node, new semantic analysis]
- **Standard Library**: [e.g., new built-in, affects existing components]
- **Documentation**: [e.g., new docs needed, updates to existing docs]

## Comparison to Other Languages

How is this handled in other languages (VHDL, Verilog, SystemVerilog, Chisel)?

```vhdl
-- VHDL example (if applicable)
```

```verilog
// Verilog example (if applicable)
```

## Use Cases

Describe 2-3 concrete use cases for this feature:

1. **Use Case 1**: [Description]
2. **Use Case 2**: [Description]
3. **Use Case 3**: [Description]

## Checklist

- [ ] I have searched for existing syntax proposals
- [ ] I have considered how this aligns with zero-magic philosophy
- [ ] I have confirmed this belongs in the compiler (not a library)
- [ ] I have provided concrete syntax examples
- [ ] I have considered impact on parser and compiler
- [ ] I have compared to other hardware description languages
- [ ] I understand the core team will research this thoroughly before implementation
