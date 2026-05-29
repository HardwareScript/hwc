# Standard Library Parse Errors - TODO

## Current Status

The stdlib is now embedded as **pre-compiled binary AST** for instant loading (~86ms for 16 modules).

However, some stdlib files have parse errors and are currently **skipped** during compilation.

## Performance

- **Before (text parsing):** 1430ms total build time
- **After (binary AST):** 169ms total build time ✅ **8.5x faster!**
- **Stdlib loading:** 87ms (vs 833ms before) - **9.6x faster!**

## Files with Parse Errors

These files use `pad_shapes:` blocks which are not yet supported by the parser:

1. `components/bga_packages.hw` - Uses `pad_shapes:` to define pad geometry
2. `components/capacitors.hw` - Uses `pad_shapes:` to define pad geometry
3. (possibly more - need to scan all files)

## The Issue

The parser only recognizes these component blocks:
- `metadata:`
- `pins:`
- `layout:`
- `electrical:`
- `render:`

But these files use:
- `pad_shapes:` ❌ (not recognized)

## Solution Options

### Option 1: Add `pad_shapes:` support to parser (proper fix)
- Update `hwc/crates/hwc-parser/src/parser/definitions/component.rs`
- Add `pad_shapes` to the list of recognized blocks
- Define AST structure for pad shapes
- This is the correct long-term solution

### Option 2: Move pad_shapes into layout block
- Change syntax from:
  ```
  layout:
      pin_positions:
          A1 at [0.4mm, 0.4mm]
      pad_shapes:
          A1: Circle(0.4mm)
  ```
- To nested structure or different syntax

### Option 3: Remove pad_shapes from stdlib (temporary)
- These are just metadata for visualization
- Not critical for compiler functionality
- Can be added back later when parser supports them

## Action Items

- [ ] Scan all stdlib files for parse errors
- [ ] Decide on syntax for pad shapes
- [ ] Implement parser support for pad_shapes
- [ ] Re-enable all stdlib files in binary AST compilation
- [ ] Update this document when fixed

## Notes

The binary AST approach is working perfectly for the 16 modules that do parse correctly. Once we fix the parser to handle `pad_shapes:`, all stdlib files will compile to binary AST and load instantly.
