# Hardware Script Licensing FAQ

## Quick Summary

Hardware Script is dual-licensed:

- **AGPLv3** (Free) - For open source projects, hobbyists, students, and research
- **Commercial License** (Paid) - For proprietary/closed-source hardware development

## Scope of the License & Package Ecosystem

### 1. The Toolchain (AGPLv3 + Commercial)

The Hardware Script compiler (`hwc`), the documentation engine (`hwsd`), the package manager CLI (`hpm`), and the underlying routing/physics engines are licensed under AGPLv3 (or Commercial License).

**This includes**:
- Parser and syntax analyzer
- Physics validation engine
- Routing algorithms
- Multi-format exporters (Gerber, GDSII, OBJ, etc.)
- Command-line tools
- Standard materials database (Copper, Silicon, FR4, etc.)

**Think of it like Microsoft Word**: Microsoft owns Word, but you own the documents you create with it.

### 2. Your Hardware Designs (You Own Them)

**We do not own the hardware designs you create.**

When you write a `.hw` file and compile it with Hardware Script:
- **You own the `.hw` source file** (your design)
- **You own the output files** (Gerber, GDSII, OBJ, etc.)
- **You choose the license** (MIT, proprietary, open-source, etc.)

**You do NOT need to open-source your designs just because you used the AGPLv3 compiler.**

**Analogy**: Just like Python doesn't own your Python scripts, Hardware Script doesn't own your `.hw` designs. You retain full copyright and licensing control.

### 3. When You Need a Commercial License

The AGPLv3 license for the compiler triggers in these specific situations:

**You need a Commercial License if**:

✅ **Modifying the Compiler**
- You modify the `hws` compiler source code itself
- You want to keep those modifications private
- **Solution**: Purchase Commercial License

✅ **Hosting as a Service**
- You run the compiler on a cloud server
- Users access it via web browser or API (network use)
- AGPLv3's network clause requires open-sourcing your backend
- **Solution**: Purchase Commercial License

✅ **Enterprise Support & Warranties**
- You need dedicated support and SLA guarantees
- You need legal indemnification and warranties
- You're running mission-critical manufacturing pipelines
- **Solution**: Purchase Tier 2 or Tier 3 Commercial License

✅ **Corporate AGPL Ban**
- Your company has a blanket ban on AGPL software
- Legal department won't allow AGPL downloads
- **Solution**: Purchase Commercial License

**You do NOT need a Commercial License if**:

❌ **Using the compiler in isolation**
- Download `hws`, write `.hw` files, generate Gerber files
- Keep your designs proprietary
- **This is completely free and legal under AGPLv3**

❌ **Selling hardware products**
- Use Hardware Script to design your products
- Manufacture and sell physical boards
- Keep your `.hw` source files private
- **This is completely free and legal**

### 4. Package Registry & Community Components

**Registry Licensing Policy**: To protect all users (from hobbyists to enterprises), all packages submitted to the official Hardware Script Registry must use permissive licenses.

**Accepted Licenses for Registry Packages**:
- MIT License (recommended)
- Apache 2.0
- CC0 (Public Domain)
- BSD 2-Clause or 3-Clause

**Why this matters**:
- **No license bombs**: Companies can safely use community components
- **No royalty traps**: Component creators can't suddenly demand payment
- **Immutable versions**: Once published under MIT, that version stays MIT forever
- **Frictionless ecosystem**: Everyone can build on each other's work

**Registry Rules**:

When you submit a component to the official registry:
1. Your component must be licensed MIT, Apache 2.0, or CC0
2. You retain copyright to your component
3. You cannot revoke the license for published versions
4. Users can use your component in both open-source AND proprietary designs

**Example `package.hw.json`**:
```json
{
  "name": "@sensors/imu-module",
  "version": "1.0.0",
  "license": "MIT",
  "author": "Your Name"
}
```

**Note**: You can still sell premium components or consulting services separately. The registry just ensures the basic building blocks are free for everyone.

### 5. The Ecosystem Architecture

```
┌─────────────────────────────────────────────────────┐
│ Compiler (hws)                                      │
│ License: AGPLv3 + Commercial                        │
│ Your monetization engine                            │
└─────────────────────────────────────────────────────┘
                        ↓
┌─────────────────────────────────────────────────────┐
│ User's Design (my_board.hw)                         │
│ License: User's choice (MIT, proprietary, etc.)     │
│ User owns this completely                           │
└─────────────────────────────────────────────────────┘
                        ↓
┌─────────────────────────────────────────────────────┐
│ Output Files (Gerber, GDSII, OBJ)                   │
│ License: User's choice                              │
│ User owns this completely                           │
└─────────────────────────────────────────────────────┘
                        ↑
┌─────────────────────────────────────────────────────┐
│ Community Packages (@sensors/imu, @power/regulator) │
│ License: MIT / Apache 2.0 (permissive)              │
│ Safe for everyone to use                            │
└─────────────────────────────────────────────────────┘
```

**Result**: Startups get a massive, free library of components they can safely use in proprietary products. As they grow into enterprises needing cloud integration or dedicated support, they purchase Commercial Licenses.

## Copyright and Ownership

**Copyright Holder**: Olowookere Olamide (2026)

All rights may be transferred to the Hardware Script Foundation (a non-profit organization) in the future. Any such transfer will not affect existing licenses or user rights.

## Governance and Long-term Commitment

Hardware Script is committed to remaining permanently open source. See [GOVERNANCE.md](GOVERNANCE.md) for:

- Founder's commitment to never selling to a private corporation
- Plans for establishing a non-profit foundation
- Long-term stewardship structure
- Community protection mechanisms

## Rights Transfer

Copyright and all associated rights are currently held by Olowookere Olamide. These rights may be assigned or transferred to a non-profit foundation or organization in the future.

**Important**: Any such transfer shall not affect:
- The terms of the AGPLv3 license
- Any rights previously granted to users and contributors
- Existing commercial license agreements

**Guarantee**: Hardware Script will never be sold to a private corporation.

## Commercial Licensing

If you wish to use Hardware Script in a proprietary environment where you cannot comply with the AGPLv3 requirements (such as keeping modifications private or using it in closed-source hardware development), you must obtain a separate commercial license.

### Who Needs a Commercial License?

You need a commercial license if:
- You're developing proprietary/closed-source hardware products
- You cannot or will not share your `.hwx` design files publicly
- You're a company selling hardware (even 1 unit)
- You're a consultant doing paid hardware design work

### Who Gets Free Use?

You can use Hardware Script for free under AGPLv3 if you are:
1. Open source hardware projects (sharing your designs publicly)
2. Individual hobbyists and makers (personal projects)
3. Academic and research institutions
4. Early-stage startups (under $100K funding/revenue)
5. Non-profit organizations

### Pricing Structure

Commercial licensing uses a globally-fair tier system with Purchasing Power Parity (PPP) adjustments:

- **Tier 1**: Companies under $1M revenue - $500-$2,000/year
- **Tier 2**: Companies $1M-$50M revenue - $5,000-$25,000/year
- **Tier 3**: Companies over $50M revenue - Custom pricing ($50,000+/year)

See [COMMERCIAL-LICENSE.md](COMMERCIAL-LICENSE.md) for complete details.

### Contact for Commercial Licensing

**Email**: hardwarescript@gmail.com  
**Subject**: "Commercial License Request - [Your Company Name]"

## Contributor License Agreement (CLA)

All contributors must agree to the CLA before their code can be merged. This ensures:
- The project can offer commercial licenses
- Contributors retain copyright to their work
- The dual-licensing model remains sustainable

See [CLA.md](CLA.md) for complete terms.

## Patent Grant

Hardware Script includes an explicit patent grant to protect users and contributors. See [PATENTS](PATENTS) for:

- Perpetual, royalty-free patent license
- Patent non-assertion pledge
- Defensive termination clause
- Prior art declaration

## Frequently Asked Questions

### Can I use Hardware Script for free?

Yes! If you're using the compiler to design hardware (even proprietary hardware), you can use the AGPLv3 version for free. You only need a Commercial License if you:
- Modify the compiler and want to keep changes private
- Host the compiler as a cloud service
- Need enterprise support, SLA, and legal warranties
- Work at a company with an AGPL ban

### What if I want to keep my designs private?

You can! The compiler output (your `.hw` files and generated Gerber/GDSII files) belongs to you. You don't need to open-source your designs just because you used an AGPLv3 compiler.

Think of it like Microsoft Word: Microsoft owns Word, but you own the documents you create.

### Can I contribute to Hardware Script?

Yes! All contributors must agree to the CLA. See [CONTRIBUTING.md](CONTRIBUTING.md) and [CLA.md](CLA.md).

### Will Hardware Script always be open source?

Yes. The AGPLv3 license is irrevocable and permanent. See [GOVERNANCE.md](GOVERNANCE.md) for our long-term commitment.

### Can a corporation buy Hardware Script?

No. The project will be transferred to a non-profit foundation, which cannot be acquired by private entities.

### What happens to commercial license revenue?

All revenue supports:
- Foundation operations and development
- Community initiatives and documentation
- Fair compensation for core maintainers
- **Never** private shareholders or investors

### Can I fork Hardware Script?

Yes! AGPLv3 gives you the right to fork. However, your fork must also be AGPLv3 licensed.

### What if I disagree with future governance decisions?

You can always fork the project under AGPLv3. The community has ultimate control.

### How is this different from other dual-licensed projects?

Hardware Script follows the proven model of MongoDB, Qt, and Sidekiq:
- Core project remains truly open source (AGPLv3)
- Commercial licenses available for proprietary use
- Revenue supports non-profit foundation, not private investors
- Community interests protected by governance structure

### Can I use Hardware Script in my commercial product?

Yes! You can use Hardware Script to design proprietary hardware products for free. You only need a Commercial License if you:
- Modify the compiler source code and keep changes private
- Host the compiler as a cloud/SaaS service
- Need enterprise support and legal warranties

### What about my dependencies?

Hardware Script's dependencies (serde, miette, etc.) have their own licenses (MIT/Apache 2.0). See [NOTICE](NOTICE) for attribution.

### Can I redistribute Hardware Script?

Yes, under AGPLv3 terms. You must:
- Include the LICENSE file
- Provide source code
- Maintain copyright notices
- Share any modifications under AGPLv3

### What if I'm in a developing country?

Commercial licensing includes PPP (Purchasing Power Parity) adjustments to ensure fair, affordable pricing based on local economic conditions.

### Can I get a trial license?

Yes. Contact us to discuss evaluation licenses for testing and proof-of-concept work.

### What support is included?

- **AGPLv3 users**: Community support via GitHub issues and Discord
- **Commercial Tier 1**: Email support (48-hour response)
- **Commercial Tier 2**: Priority email support (24-hour response)
- **Commercial Tier 3**: Dedicated account manager and SLA

## Legal Clarity

This licensing policy supplements the AGPLv3 license. If you cannot or will not comply with AGPLv3 requirements (sharing your source code), you must obtain a commercial license to use Hardware Script legally.

**The determination of licensing terms, including pricing, is at the sole discretion of the copyright holder (currently Olowookere Olamide, and in the future, the Hardware Script Foundation).**

## Additional Resources

- **Full AGPLv3 Text**: [LICENSE](LICENSE)
- **Commercial Licensing Details**: [COMMERCIAL-LICENSE.md](COMMERCIAL-LICENSE.md)
- **Contributor Agreement**: [CLA.md](CLA.md)
- **Governance Structure**: [GOVERNANCE.md](GOVERNANCE.md)
- **Patent Grant**: [PATENTS](PATENTS)
- **Dependency Attribution**: [NOTICE](NOTICE)
- **Contributing Guide**: [CONTRIBUTING.md](CONTRIBUTING.md)

## Contact

- **Email**: hardwarescript@gmail.com
- **GitHub**: https://github.com/hwsl-lang
- **Discord**: https://discord.gg/9zqH8nuCet
- **Twitter**: @hwsl_lang

---

**Last Updated**: March 18, 2026  
**Version**: 1.0  
**Maintained by**: Olowookere Olamide
