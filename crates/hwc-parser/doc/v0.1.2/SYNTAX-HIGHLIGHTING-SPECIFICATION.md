# Hardware Script: Syntax Highlighting Specification

## Purpose

This document defines the official syntax highlighting color scheme for Hardware Script. These specifications are used when building IDE extensions (VS Code, IntelliJ, Sublime Text, etc.) to ensure consistent visual presentation across all development environments.

## Color Palette (Dark Theme)

Based on the canonical dark theme specification:

| Token Type | Color Code | Color Name | Example |
|------------|-----------|------------|---------|
| Comments | `#4CAF50` | Green | `## Advanced Motor Driver Module` |
| Keywords | `#C586C0` | Purple | `import`, `define`, `add`, `route`, `expose` |
| Control Keywords | `#569CD6` | Blue | `from`, `to`, `by`, `at`, `named`, `as`, `spanning`, `rotated` |
| Types/Classes | `#4EC9B0` | Teal | `FR4`, `MotorController`, `Battery_LiPo`, `Resistor` |
| Strings | `#CE9178` | Orange | `"Smart_Motor_Driver"`, `standard.materials` |
| Numbers | `#B5CEA8` | Light Green | `50`, `4.7`, `-30.5` |
| Units | `#4FC1FF` | Cyan | `mm`, `V`, `kΩ`, `nF` |
| Properties/Fields | `#DCDCAA` | Yellow | `dimensions`, `grid`, `path`, `Plus`, `VIN` |
| Identifiers/Variables | `#9CDCFE` | Light Blue | `MainPower`, `Driver`, `PullUp`, `ErrorSignal` |
| Inline Comments | `#808080` | Gray | `# Via down to inner layer 3` |

## Complete Highlighted Example

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
  add Substrate(FR4) spanning [1, 1, 1] to [4, 500, 500]

  ## Power & Passives
  add Battery_LiPo (12V) named MainPower at [1, 10, 10]
  add Resistor (4.7kΩ) named PullUp at [1, 20, 15] rotated 90
  add Capacitor (100nF) named Decoupling at [1, 45, 45] rotated -30.5

  # Main Driver Chip (Mounted diagonally for tight circular enclosures)
  add MotorController named Driver at [1, 50, 50] rotated 45

  # Routing with precise 3D waypoints
  route MainPower.Plus to Driver.VIN:
    path:
      - [1, 10, 10]
      - [1, 10, 50]
      - [1, 40, 50]

  # Multi-layer signal routing (Vias)
  route PullUp.Pin2 to Driver.Enable:
    path:
      - [1, 20, 15]
      - [3, 20, 15]  # Via down to inner layer 3
      - [3, 48, 48]  # Route on inner layer
      - [1, 48, 48]  # Via back to surface

  ## Expose pins so this entire board can be imported as a module
  expose Driver.Fault as ErrorSignal
```

## Token-to-Color Mapping

### Comments
- **Pattern**: `##` (section headers) and `#` (inline comments)
- **Color**: `#4CAF50` (Green) for section headers, `#808080` (Gray) for inline
- **Examples**: 
  - `## Advanced Motor Driver Module`
  - `# Via down to inner layer 3`

### Keywords (Primary Actions)
- **Pattern**: `import`, `define`, `add`, `route`, `expose`
- **Color**: `#C586C0` (Purple)
- **Rationale**: These are the main action verbs that structure the code

### Keywords (Control/Prepositions)
- **Pattern**: `from`, `to`, `by`, `at`, `named`, `as`, `spanning`, `rotated`
- **Color**: `#569CD6` (Blue)
- **Rationale**: These are relational keywords that connect concepts

### Types/Classes
- **Pattern**: `FR4`, `MotorController`, `Battery_LiPo`, `Resistor`, `Capacitor`, `Substrate`, `Space`
- **Color**: `#4EC9B0` (Teal)
- **Rationale**: PascalCase identifiers representing component types

### String Literals
- **Pattern**: `"Smart_Motor_Driver"`, module paths like `standard.materials`, `@robotics/motor`
- **Color**: `#CE9178` (Orange)
- **Rationale**: Quoted strings and import paths

### Numbers
- **Pattern**: `50`, `4.7`, `-30.5`, `100`
- **Color**: `#B5CEA8` (Light Green)
- **Rationale**: All numeric literals (integers and floats)

### Units
- **Pattern**: `mm`, `V`, `kΩ`, `nF`, `Hz`, `°C`, `deg`
- **Color**: `#4FC1FF` (Cyan)
- **Rationale**: Physical unit suffixes attached to numbers

### Properties/Fields
- **Pattern**: `dimensions`, `grid`, `path`, `Plus`, `VIN`, `Pin2`, `Enable`, `Fault`
- **Color**: `#DCDCAA` (Yellow)
- **Rationale**: Property names in key-value pairs or dot-notation

### Identifiers/Variables
- **Pattern**: `MainPower`, `Driver`, `PullUp`, `Decoupling`, `ErrorSignal`
- **Color**: `#9CDCFE` (Light Blue)
- **Rationale**: Named instances of components

## TextMate Grammar Scopes

For VS Code and other TextMate-compatible editors, use these scope names:

```json
{
  "comment.line.number-sign.hw": "#4CAF50",
  "comment.line.documentation.hw": "#4CAF50",
  "keyword.control.hw": "#C586C0",
  "keyword.other.hw": "#569CD6",
  "entity.name.type.hw": "#4EC9B0",
  "string.quoted.double.hw": "#CE9178",
  "constant.numeric.hw": "#B5CEA8",
  "keyword.other.unit.hw": "#4FC1FF",
  "variable.other.property.hw": "#DCDCAA",
  "variable.other.readwrite.hw": "#9CDCFE"
}
```

## Light Theme Variant

For light theme support, adjust colors to maintain contrast:

| Token Type | Light Theme Color | Contrast Ratio |
|------------|------------------|----------------|
| Comments | `#2E7D32` | Dark Green |
| Keywords | `#7B1FA2` | Dark Purple |
| Control Keywords | `#1976D2` | Dark Blue |
| Types/Classes | `#00796B` | Dark Teal |
| Strings | `#D84315` | Dark Orange |
| Numbers | `#558B2F` | Olive Green |
| Units | `#0277BD` | Deep Cyan |
| Properties/Fields | `#F57F17` | Dark Yellow |
| Identifiers/Variables | `#0288D1` | Medium Blue |
| Inline Comments | `#616161` | Dark Gray |

## Implementation Guidelines

### For VS Code Extensions

1. Create a `syntaxes/hw.tmLanguage.json` file
2. Define patterns for each token type
3. Map to TextMate scopes
4. Reference in `package.json` under `contributes.grammars`

### For Language Servers (LSP)

1. Implement semantic token provider
2. Return token types and modifiers
3. Let editor apply theme colors based on token types

### For Web Editors (Monaco, CodeMirror)

1. Define language configuration
2. Create tokenizer rules
3. Map tokens to CSS classes
4. Apply color scheme via CSS

## Testing Checklist

When implementing syntax highlighting, verify:

- [ ] Comments are clearly distinguished (green for sections, gray for inline)
- [ ] Keywords stand out (purple for actions, blue for prepositions)
- [ ] Types are visually distinct from variables (teal vs light blue)
- [ ] Numbers and units are separately colored (light green and cyan)
- [ ] Strings are easily identifiable (orange)
- [ ] Properties are highlighted in dot-notation (yellow)
- [ ] Unicode characters (Ω, µ, °) render correctly
- [ ] Nested indentation maintains readability
- [ ] Dark and light themes both have sufficient contrast

## Visual Design Philosophy

The color scheme is designed to:

1. **Maximize readability** - High contrast, distinct colors
2. **Guide the eye** - Important keywords (purple) stand out
3. **Show relationships** - Blue prepositions connect concepts
4. **Distinguish types** - Teal types vs light blue instances
5. **Highlight data** - Numbers and units are visually paired
6. **Maintain consistency** - Follows VS Code's dark+ theme conventions

This specification ensures Hardware Script code looks beautiful and professional in any IDE.
