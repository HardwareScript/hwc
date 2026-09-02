# Hardware Script Compiler — Licensing Summary

**Version**: 0.3.1  
**Last Updated**: March 18, 2026  
**Copyright**: © 2024-2026 Olowookere Olamide and HardwareScript Contributors

---

## Quick Reference

| Component | License | Commercial Use | Source Required |
|-----------|---------|----------------|-----------------|
| **Compiler (`hwc`)** | AGPLv3 + Commercial | ✅ Free for local use | ❌ Not required for local use |
| **Your `.hw` designs** | Your choice | ✅ Fully yours | ❌ Keep private if you want |
| **Compiler output** (Gerber, GDSII, etc.) | Your choice | ✅ Fully yours | ❌ Keep private if you want |
| **Plugins (WASM)** | Your choice | ✅ Can be proprietary | ❌ Plugin Exception applies |
| **Plugin SDK** | MIT / Apache 2.0 | ✅ Fully permissive | ❌ No requirements |

---

## License Structure

### 1. Core Compiler (AGPLv3 with Exceptions)

The Hardware Script compiler (`hwc`) and all its subsystems are licensed under:

```
GNU Affero General Public License v3.0 or later (AGPL-3.0-or-later)
WITH HardwareScript-Compiler-Output-Exception
```

**What this means:**
- ✅ **Free to use locally** — Download, compile `.hw` files, sell hardware products (no license needed)
- ✅ **Free to modify** — Fork, patch, customize for your own use
- ✅ **Source code available** — Full compiler source code is open and auditable
- ❌ **Cloud/SaaS requires license** — Running `hwc` as a hosted service requires AGPLv3 compliance OR a Commercial License
- ❌ **Proprietary forks require license** — Distributing modified binaries without source code requires a Commercial License

**SPDX Identifier in source files:**
```rust
// SPDX-License-Identifier: AGPL-3.0-or-later WITH HardwareScript-Compiler-Output-Exception
// Copyright (C) 2024-2026 Olowookere Olamide and HardwareScript Contributors
```

**Key Documents:**
- [LICENSE.md](LICENSE.md) — Full AGPLv3 license text
- [COMPILER-OUTPUT-EXCEPTION.md](COMPILER-OUTPUT-EXCEPTION.md) — Compiler output exception
- [COMMERCIAL-LICENSE.md](COMMERCIAL-LICENSE.md) — Commercial licensing details
- [LICENSE-FAQ.md](LICENSE-FAQ.md) — Frequently asked questions

---

### 2. Compiler Output (User-Owned, No Restrictions)

**Your hardware designs belong to YOU.**

When you compile a `.hw` file with `hwc`, you own:
- ✅ The `.hw` source file (your design)
- ✅ All output files (Gerber, GDSII, OBJ, DXF, STEP, etc.)
- ✅ The physical hardware manufactured from those files

**You can:**
- Keep your `.hw` designs proprietary and closed-source
- Sell hardware products without any licensing fees
- Use any license you want for your designs (MIT, proprietary, etc.)
- Manufacture and sell millions of devices without telling us

**Think of it like Microsoft Word:**
- Microsoft owns Word (AGPLv3 = compiler)
- You own the documents you create (Your designs)
- You can sell books written in Word without paying Microsoft

---

### 3. Plugin Exception (WASM Plugins)

**Proprietary plugins are explicitly allowed.**

The compiler includes a **Plugin Exception** that allows:
- ✅ Writing proprietary, closed-source plugins in any language (Rust, Zig, C++)
- ✅ Distributing compiled WASM plugins without source code
- ✅ Loading plugins at runtime via the WASM ABI
- ✅ Selling commercial plugins on the Hardware Script registry

**What's covered:**
- Custom routers (proprietary pathfinding algorithms)
- Logic synthesis engines (FPGA/ASIC mapping)
- Export plugins (proprietary foundry formats)
- Simulation backends (custom SPICE engines)

**See:** [COMPILER-OUTPUT-EXCEPTION.md](COMPILER-OUTPUT-EXCEPTION.md) Section 2

---

### 4. Plugin SDK (MIT / Apache 2.0)

**The Plugin SDK is permissively licensed for maximum compatibility.**

Repository: [HardwareScript/hw-plugin-sdk](https://github.com/HardwareScript/hw-plugin-sdk)

License: **MIT OR Apache-2.0** (dual-licensed, choose either)

**What's included:**
- Pure Rust ABI definitions (`#[repr(C)]` structs)
- C/C++ headers (`hardwarescript.h`)
- Zig bindings (`hardwarescript.zig`)
- Plugin templates and examples

**Why separate?**
- ✅ Corporate legal scanners won't flag AGPLv3
- ✅ Clean IP provenance for proprietary plugins
- ✅ Zero friction for third-party developers
- ✅ Safe for foundries, EDA vendors, and enterprises

---

## When You Need a Commercial License

You **only** need a Commercial License if you:

### A. Cloud/SaaS Deployment (AGPLv3 Section 13)
- Running `hwc` on a server and offering compilation-as-a-service
- Building AI-powered "Prompt-to-Silicon" platforms
- Hosting web-based EDA tools with `hwc` backend
- **Why**: AGPLv3 requires open-sourcing your entire backend stack

### B. Proprietary Tool Embedding
- Embedding `hwc-engine` or `hwc-compiler` into closed-source commercial software
- Integrating Hardware Script into proprietary EDA toolchains
- **Why**: AGPLv3 requires sharing your modified compiler code

### C. Proprietary Compiler Forks
- Distributing modified `hwc` binaries without releasing source code
- Creating proprietary forks for internal use at scale
- **Why**: AGPLv3 requires publishing modifications

### D. Corporate AGPL Ban
- Your enterprise has strict legal policies against AGPLv3 software
- You need legal indemnification and warranties
- **Why**: Commercial License provides AGPLv3 exemption

**Commercial License Tiers:**
- **Tier 1**: Startup / Cloud Integrator — $5,000-$15,000/year
- **Tier 2**: EDA Vendor / Tool Embedding — $25,000-$100,000/year
- **Tier 3**: Enterprise / Foundry — Custom pricing ($100,000+/year)

**Contact**: hardwarescript@gmail.com

---

## What You DON'T Need a License For

### ✅ Hardware Design & Manufacturing

- Using `hwc` locally to design PCBs and ASICs
- Keeping your `.hw` design files proprietary
- Selling physical hardware products (even billions of devices)
- Manufacturing classified/defense electronics
- Running `hwc` in air-gapped environments
- Using `hwc` in CI/CD pipelines

**No revenue thresholds, no usage tracking, no phone-home.**

### ✅ Open-Source Contributions

- Contributing to the core compiler (with CLA)
- Publishing open-source `.hw` designs
- Building open-source plugins (MIT, Apache 2.0, etc.)
- Educational and research use

### ✅ Plugin Development

- Writing proprietary WASM plugins using the Plugin SDK
- Distributing closed-source plugins on the registry
- Selling commercial plugins

---

## Cargo.toml Configuration

All compiler crates use workspace-level licensing:

```toml
[workspace.package]
version = "0.3.1"
edition = "2021"
authors = ["Olowookere Olamide and HardwareScript Contributors"]
license = "AGPL-3.0-or-later"
repository = "https://github.com/HardwareScript/hwc"
```

**Note**: The `license` field shows `AGPL-3.0-or-later` because Cargo doesn't support custom SPDX expressions with exceptions. The full license (including the Compiler Output Exception) is documented in [LICENSE.md](LICENSE.md) and [COMPILER-OUTPUT-EXCEPTION.md](COMPILER-OUTPUT-EXCEPTION.md).

---

## SPDX Headers in Source Files

All Rust source files include the standard SPDX header:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later WITH HardwareScript-Compiler-Output-Exception
// Copyright (C) 2024-2026 Olowookere Olamide and HardwareScript Contributors
//
// This file is part of the Hardware Script compiler (hwc).
//
// hwc is free software: you can redistribute it and/or modify it under the terms
// of the GNU Affero General Public License as published by the Free Software Foundation,
// either version 3 of the License, or (at your option) any later version, WITH the
// HardwareScript Compiler Output Exception.
//
// See LICENSE.md and COMPILER-OUTPUT-EXCEPTION.md in the repository root for details.
```

---

## Repository Structure

```
hwc/
├── LICENSE.md                        ← Full AGPLv3 license text
├── COMPILER-OUTPUT-EXCEPTION.md      ← Compiler output + plugin exception
├── COMMERCIAL-LICENSE.md             ← Commercial licensing details
├── LICENSE-FAQ.md                    ← Frequently asked questions
├── LICENSING-SUMMARY.md              ← This file
├── CLA.md                            ← Contributor License Agreement
├── PATENTS                           ← Patent grant
├── GOVERNANCE.md                     ← Project governance
├── Cargo.toml                        ← Workspace license configuration
└── crates/
    ├── hwc-cli/                      ← CLI binary (AGPLv3)
    ├── hwc-compiler/                 ← Compiler core (AGPLv3)
    ├── hwc-engine/                   ← Physical engine (AGPLv3)
    ├── hwc-router/                   ← Routing engine (AGPLv3)
    ├── hwc-parser/                   ← Parser (AGPLv3)
    ├── hwc-physics/                  ← Physics validation (AGPLv3)
    ├── hwc-export/                   ← Export engines (AGPLv3)
    ├── hwc-materials/                ← Materials database (AGPLv3)
    ├── hwc-stdlib/                   ← Standard library (AGPLv3)
    ├── hwc-synthesis/                ← Logic synthesis (AGPLv3)
    ├── hwc-types/                    ← Core types (AGPLv3)
    └── hwc-diagnostics/              ← Diagnostics (AGPLv3)
```

---

## Related Repositories

| Repository | License | Purpose |
|------------|---------|---------|
| [HardwareScript/hwc](https://github.com/HardwareScript/hwc) | AGPLv3 + Commercial | Core compiler engine |
| [HardwareScript/hw-plugin-sdk](https://github.com/HardwareScript/hw-plugin-sdk) | MIT / Apache 2.0 | Plugin ABI and developer SDK |
| [HardwareScript/Docs](https://github.com/HardwareScript/Docs) | CC-BY-4.0 | Language documentation |
| [HardwareScript/ROADMAP](https://github.com/HardwareScript/ROADMAP) | CC-BY-4.0 | Implementation roadmaps |

---

## License Compliance Tools

### Checking License Information

```bash
# Display license information for all crates
cargo tree --workspace -e normal --prefix none | sort -u

# Check SPDX identifiers in source files
rg "SPDX-License-Identifier" --type rust

# Generate license report
cargo install cargo-license
cargo license --workspace
```

### For Automated Scanners

- **SPDX**: `AGPL-3.0-or-later WITH HardwareScript-Compiler-Output-Exception`
- **Cargo License Field**: `AGPL-3.0-or-later`
- **REUSE Compliance**: See [.reuse/](https://reuse.software/) directory (planned)

---

## FAQ

### Can I use `hwc` for free in my company?

**Yes**, if you're using it locally to design hardware. No license needed, regardless of company size or revenue.

### Do I need to open-source my `.hw` designs?

**No.** Your designs are completely yours. You can keep them proprietary.

### Can I sell hardware designed with `hwc`?

**Yes**, absolutely. No royalties, no licensing fees, no restrictions.

### Can I write closed-source plugins?

**Yes.** The Plugin Exception explicitly allows proprietary WASM plugins.

### When do I need a commercial license?

Only if you're running `hwc` as a cloud service, embedding it in proprietary software, or need enterprise support.

### Can I fork the compiler?

**Yes**, under AGPLv3 terms. You must publish your fork's source code under AGPLv3.

### What if my company bans AGPLv3 software?

Purchase a Commercial License for AGPLv3 exemption.

---

## Contact & Support

- **Email**: hardwarescript@gmail.com
- **GitHub**: https://github.com/HardwareScript/hwc
- **Discord**: https://discord.gg/9zqH8nuCet
- **Commercial Licensing**: hardwarescript@gmail.com (Subject: "Commercial License Request")

---

## Legal Notice

This summary is provided for informational purposes. For legally binding terms, refer to:
- [LICENSE.md](LICENSE.md) — AGPLv3 license
- [COMPILER-OUTPUT-EXCEPTION.md](COMPILER-OUTPUT-EXCEPTION.md) — Exception terms
- [COMMERCIAL-LICENSE.md](COMMERCIAL-LICENSE.md) — Commercial terms

**The determination of licensing terms is at the sole discretion of the copyright holder (Olowookere Olamide, and in the future, the HardwareScript Foundation).**

---

**Last Updated**: March 18, 2026  
**Version**: 0.3.1  
**Maintained by**: Olowookere Olamide and the HardwareScript Contributors
