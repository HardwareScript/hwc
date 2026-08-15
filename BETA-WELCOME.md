# Welcome to HardwareScript v0.2.1 Beta! 🎉

Thank you for being an early adopter of HardwareScript - a revolutionary hardware description language for designing electronic circuits at the physical level.

## Quick Start

### Installation

1. **Download the compiler**: Get `hwc.exe` from this release
2. **Add to PATH** (optional but recommended):
   - Move `hwc.exe` to a permanent location (e.g., `C:\Program Files\hwc\`)
   - Add that folder to your System Environment Variables PATH
   - Or just run it directly: `C:\path\to\hwc.exe <command>`

3. **Verify installation**:
   ```cmd
   hwc --version
   ```

### Available Commands

The HardwareScript compiler (`hwc`) currently supports these commands:

- **`hwc build <file.hw>`** - Compile your `.hw` file to generate output formats (GDSII, SPICE netlists, BOMs, 3D models)
- **`hwc drc <build-dir>`** - Run design rule checks on an existing build (spacing, width, crosstalk violations)
- **`hwc physics <build-dir>`** - Run physics validation (electromigration, thermal, parasitic extraction)
- **`hwc check <file.hw>`** - Quick syntax validation without building
- **`hwc init <project-name>`** - Initialize a new HardwareScript project with boilerplate
- **`hwc materials`** - Manage materials database
- **`hwc simulate`** - Run physics simulation
- **`hwc doc`** - Access documentation (coming soon)

### Get Syntax Highlighting (VS Code)

For a better development experience with syntax highlighting and language support:

1. Visit the [HardwareScript VS Code Extension repository](https://github.com/HardwareScript/hws_VS_Code_Extension)
2. Download `hardwarescript-visual-0.2.1.vsix` from the releases
3. In VS Code: 
   - Press `Ctrl+Shift+P`
   - Type "Extensions: Install from VSIX"
   - Select the downloaded `.vsix` file

Now your `.hw` files will have proper syntax highlighting, code snippets, and language support!

**Extension Repository**: https://github.com/HardwareScript/hws_VS_Code_Extension

## Documentation

We're actively working on our website and centralized documentation. For now, comprehensive documentation is available in our dedicated [Docs repository](https://github.com/HardwareScript/Docs).

**Current version docs**: Check the `v0.2.1` folder for:
- Language specification
- Architecture details
- Design rule implementation guides
- Physics validation features
- Example code and tutorials

Each version has its own documentation folder, so you can always reference what's supported in your compiler version.

**Documentation Repository**: https://github.com/HardwareScript/Docs

## Getting Help

If you get stuck:
- Browse the [Docs repo](https://github.com/HardwareScript/Docs) for detailed guides
- Check the `tests/` folder in the compiler repo for working examples
- Open an issue on GitHub
- Reach out to the team

## What's Working in Beta

✅ Full language parsing and compilation  
✅ GDSII layout export  
✅ SPICE netlist generation  
✅ Design rule checking (DRC)  
✅ Physics validation (electromigration, thermal, crosstalk)  
✅ Parasitic extraction  
✅ BOM generation  
✅ 3D visualization output  

## Quick Example

Here's a simple resistor design to get you started:

```hw
// simple_resistor.hw
material Copper from "materials.hw"

space SimpleResistor {
    layer Metal1 {
        trace resistor {
            material: Copper,
            width: 10um,
            length: 100um
        }
    }
}
```

Compile it:
```cmd
hwc build simple_resistor.hw
```

Your outputs will be in the `build/` directory!

## Beta Notes

This is beta software - expect some rough edges, but the core functionality is solid. Your feedback will shape the future of HardwareScript!

**Known Limitations**:
- Website and online documentation still in development
- Some advanced features still being refined
- Error messages could be more helpful (we're working on it!)

## Links

- **Compiler**: https://github.com/HardwareScript/hwc
- **Documentation**: https://github.com/HardwareScript/Docs
- **VS Code Extension**: https://github.com/HardwareScript/hws_VS_Code_Extension

Happy designing! ⚡
