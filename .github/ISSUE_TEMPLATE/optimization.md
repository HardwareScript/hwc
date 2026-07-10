---
name: Optimization Idea
about: Share a mathematical or algorithmic optimization for the compiler
title: "[OPT] "
labels: optimization
assignees: ''
---

## Summary

A clear and concise description of your optimization idea.

## Motivation

Why is this optimization important? What problem does it solve?

**Hardware Script follows The Lean Core Philosophy** — the compiler must stay as lightweight and fast as possible. Optimizations should aim for O(1) lookups, minimal memory usage, and sub-millisecond compilation.

## Current Behavior

Describe the current performance or behavior that could be improved.

## Proposed Solution

Explain your proposed optimization approach in detail. Include:

- Mathematical foundations or algorithmic theory
- Pseudocode or high-level implementation approach
- Expected performance improvement (if known)

## Research & References

Link to any papers, articles, or prior art that support this optimization.

- [ ] Paper/article link 1
- [ ] Paper/article link 2

## Benchmark Data (Optional)

If you have benchmarked this approach, share the results:

```
# Benchmark results here
```

## Impact Assessment

- **Performance**: [e.g., 2x faster routing, 50% less memory]
- **Scope**: [e.g., affects routing engine, affects parser]
- **Complexity**: [e.g., low, medium, high]

## Example

If applicable, show a before/after comparison:

**Before:**
```rust
// Current implementation approach
```

**After:**
```rust
// Proposed implementation approach
```

## Checklist

- [ ] I have searched for existing optimization issues
- [ ] I have provided mathematical or algorithmic justification
- [ ] I understand this will be researched and implemented by the core team
- [ ] I have included references to prior art (if any)
