# Hardware Script Package Registry Rules

**Version**: 1.0  
**Last Updated**: March 18, 2026

---

## Purpose

The Hardware Script Package Registry exists to provide a safe, frictionless ecosystem of reusable hardware components that anyone can use—from hobbyists to Fortune 500 companies—without fear of license traps, royalty demands, or legal complications.

---

## Registry Licensing Policy

**To protect all users of Hardware Script**, all packages submitted to the official Hardware Script Registry must be licensed under a permissive, open-source license.

### Accepted Licenses

✅ **MIT License** (recommended)  
✅ **Apache License 2.0**  
✅ **CC0 (Public Domain)**  
✅ **BSD 2-Clause or 3-Clause**

### Why Permissive Licenses?

**No License Bombs**: Companies can safely download and use community components without worrying that a creator will suddenly change terms or demand royalties.

**No Royalty Traps**: Once you publish `@sensors/imu v1.0.0` under MIT, that version stays MIT forever. You cannot revoke it.

**Frictionless Innovation**: Developers can build on each other's work without legal friction, creating a massive ecosystem of reusable components.

**Enterprise-Safe**: Large corporations with strict legal departments can confidently use registry packages in proprietary products.

---

## Submitting a Package

### Requirements

When you submit a Pull Request to add your component to the official registry, you must:

1. **License your package** under MIT, Apache 2.0, CC0, or BSD
2. **Include a LICENSE file** in your package repository
3. **Declare the license** in your `package.hw.json`:

```json
{
  "name": "@sensors/imu-module",
  "version": "1.0.0",
  "license": "MIT",
  "author": "Your Name",
  "repository": "https://github.com/yourname/hw-imu-module"
}
```

4. **Confirm your agreement** by checking the box in the PR template:
   - [ ] I confirm this package is licensed under MIT, Apache 2.0, CC0, or BSD

### What You're Agreeing To

By submitting your package to the registry, you agree that:

- Your component is available for anyone to use, modify, and distribute for free
- Users can incorporate your component into both open-source AND proprietary commercial hardware designs
- You will not revoke this license for published versions
- You retain copyright to your component (you still own it)

---

## What You Still Own

### Copyright

You retain full copyright ownership of your component. The permissive license only grants usage rights; it doesn't transfer ownership.

### Attribution

Users should credit you (most permissive licenses require attribution). Your name stays in the LICENSE file and package metadata.

### Future Versions

You can change the license for future versions if you want (though we don't recommend it). But versions already published under MIT stay MIT forever.

---

## Monetization Options

**You can still make money from your components!**

### Allowed Monetization Strategies

✅ **Consulting Services**: Offer paid integration help  
✅ **Premium Versions**: Publish basic version to registry (MIT), sell advanced version separately  
✅ **Support Contracts**: Offer paid support for your components  
✅ **Custom Development**: Get hired to customize components for specific clients  
✅ **Donations**: Accept donations via GitHub Sponsors, Patreon, etc.

### What You Cannot Do

❌ **Dual-License Registry Packages**: Don't publish to registry under MIT then try to sell the same version commercially  
❌ **Revoke Published Versions**: Once v1.0.0 is MIT, it stays MIT  
❌ **Add Royalty Terms**: No "free for non-commercial, paid for commercial" schemes in registry packages

---

## Why This Protects Everyone

### For Individual Developers

- Your components get maximum adoption
- Companies can safely use your work
- You get credit and recognition
- You can still monetize through services

### For Startups

- Build products using hundreds of free components
- No legal review needed for each component
- No surprise royalty demands
- Focus on innovation, not licensing

### For Enterprises

- Legal departments approve registry packages once
- No supply chain license risk
- Safe to use in proprietary products
- Predictable, stable ecosystem

### For the Ecosystem

- Massive library of reusable components
- Network effects: more components = more users = more components
- Hardware Script becomes the industry standard
- Everyone wins

---

## Factual Data Exception

**Defining component specifications is essentially documenting facts.**

When you create a package for an LM7805 voltage regulator, you're writing down:
- Pin definitions (VIN, GND, VOUT)
- Physical dimensions from the datasheet
- Electrical characteristics (5V output, 1.5A max)

This is factual data, not creative work. It should be free for everyone to use.

**That's why we require permissive licenses for registry packages.**

---

## Enforcement

### Registry Maintainers Will

✅ Check that packages have proper LICENSE files  
✅ Verify `package.hw.json` declares an accepted license  
✅ Reject packages with restrictive or unclear licenses  
✅ Remove packages if creators try to retroactively change terms (for published versions)

### We Will NOT

❌ Police what you do outside the registry  
❌ Force you to use permissive licenses for private packages  
❌ Prevent you from selling premium versions separately  
❌ Control how you monetize your expertise

---

## Examples

### ✅ Good: MIT-Licensed Component

```json
{
  "name": "@passive/resistor-10k",
  "version": "1.0.0",
  "license": "MIT",
  "description": "Standard 10kΩ resistor component"
}
```

**Result**: Accepted into registry. Anyone can use it freely.

---

### ✅ Good: Premium Version Strategy

**Registry Package** (MIT):
```json
{
  "name": "@sensors/imu-basic",
  "version": "1.0.0",
  "license": "MIT",
  "description": "Basic IMU sensor with standard features"
}
```

**Separate Premium Package** (Your License):
```json
{
  "name": "@sensors/imu-pro",
  "version": "1.0.0",
  "license": "Commercial",
  "description": "Advanced IMU with calibration and filtering"
}
```

**Result**: Basic version in registry (free for all). Pro version sold separately. Everyone wins.

---

### ❌ Bad: Dual-License Trap

```json
{
  "name": "@sensors/imu-module",
  "version": "1.0.0",
  "license": "MIT for non-commercial, Commercial for commercial use"
}
```

**Result**: Rejected. This creates exactly the license uncertainty we're trying to avoid.

---

### ❌ Bad: Restrictive License

```json
{
  "name": "@power/regulator",
  "version": "1.0.0",
  "license": "GPL-3.0"
}
```

**Result**: Rejected. GPL is copyleft (forces derivative works to be GPL). Not permissive enough for registry.

**Alternative**: Publish under MIT to registry, or host separately for GPL users.

---

## Future: Centralized Registry

**Current Status**: Bootstrapped via GitHub PRs to `registry.yaml`

**Future (v0.3+)**: Centralized package hosting (like npmjs.com)

When we build centralized hosting, these rules will be formalized in Terms of Service. For now, this document serves as the community agreement.

---

## Questions?

### "Can I publish a component under GPL separately?"

Yes! You can host GPL-licensed components on your own GitHub. They just won't be in the official registry.

### "What if I want to change my component's license later?"

You can change future versions. But versions already published under MIT stay MIT forever (that's how open-source licenses work).

### "Can I fork someone's MIT component and improve it?"

Yes! That's the point of MIT. Fork it, improve it, publish your improved version (also MIT).

### "What if someone uses my component in a billion-dollar product?"

That's success! They used your component because it was good. You get:
- Recognition and credibility
- Potential consulting opportunities
- Contributions back to your component
- Proof of impact for your portfolio

### "Can I require attribution?"

MIT and Apache 2.0 already require attribution (keeping your copyright notice). That's built-in.

---

## Contact

Questions about registry rules?

- **GitHub Discussions**: https://github.com/hwsl-lang/discussions
- **Discord**: https://discord.gg/9zqH8nuCet
- **Email**: hardwarescript@gmail.com

---

## Summary

**Registry packages must be MIT/Apache/CC0/BSD** to ensure a safe, frictionless ecosystem for everyone.

**You still own your components** and can monetize through services, premium versions, and consulting.

**This protects users** from license bombs and creates the massive component library that makes Hardware Script the industry standard.

---

**Last Updated**: March 18, 2026  
**Version**: 1.0  
**Maintained by**: Hardware Script Community
