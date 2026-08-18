# HardwareScript Commercial Licensing

HardwareScript's core engines (`hwc` compiler and `hsm` runtime) are dual-licensed under the **GNU Affero General Public License v3.0 (AGPLv3)** and a **Commercial Enterprise License**.

---

## 1. When You Can Use HardwareScript for FREE

You do **NOT** need a commercial license if you are:

- Using the official `hwc` compiler locally on your machine or CI/CD pipelines to build hardware designs.
- Designing physical circuit boards (PCBs) or integrated circuits (ASICs) and selling the physical hardware.
- Keeping your hardware design files (`.hw`, `.hwx`, DXF, GDSII, Gerber, SPICE) proprietary and closed-source.
- Contributing to or building packages for the open HardwareScript registry under permissive licenses (MIT/Apache 2.0).

**Local CLI usage is completely free for everyone—individuals, startups, and Fortune 500 companies alike.**

---

## 2. When You MUST Obtain a Commercial License

A Commercial License is legally required if your organization wants an exemption from AGPLv3 copyleft terms in any of the following scenarios:

### A. Cloud, AI & Hosted SaaS Platforms (AGPLv3 Section 13)

If you run `hwc`, `hwc-engine`, or `hsm` on a network server to provide cloud compilation, automated AI layout synthesis, remote DRC, or web-based EDA services to third parties, AGPLv3 mandates that you make your **entire surrounding backend service, API wrapper, and orchestration code publicly available under AGPLv3**. 

If you wish to offer a hosted or AI-driven hardware service while keeping your server stack, backend infrastructure, and platform proprietary, you **must obtain a Commercial Cloud License**.

**Examples requiring a license:**
- "Prompt-to-Silicon" AI platforms that use HardwareScript behind an API
- Cloud PCB/ASIC synthesis services with web interfaces
- Hosted EDA platforms offering automated layout generation
- SaaS tools that run `hwc` on remote servers for customers

### B. Proprietary Tool Embedding & Integration

If you embed or link `hwc-engine`, `hwc-compiler`, or `hwc-parser` directly into closed-source commercial software, EDA toolchains, or proprietary desktop applications where you cannot disclose your software's source code under AGPLv3.

**Examples requiring a license:**
- EDA vendors integrating HardwareScript into commercial tools
- Silicon foundries embedding `hwc` in proprietary design flows
- Hardware companies building closed-source design automation platforms

### C. Proprietary Engine Forks & Modifications

If you modify the internal Rust compiler source code of `hwc` or `hsm` and distribute those binaries to employees, customers, or third parties without releasing your modified engine source code.

### D. Corporate Legal Compliance

If your enterprise has strict internal legal policies forbidding the installation or use of copyleft/AGPLv3 licensed software.

---

## 3. Commercial License Tiers

We offer commercial exemptions, dedicated SLAs, and legal indemnification:

### Tier 1: Startup / Cloud Integrator

**For**: Startups offering hosted tools, SaaS backends, or AI layout services

**Pricing**: $5,000 - $15,000 USD per year

**Includes**:
- AGPLv3 exemption for cloud/SaaS deployments
- Access to stable releases and updates
- Email support (48-hour response time)
- License covers up to 10 developers
- Patent grant protection

**Perfect for**: AI/ML startups, cloud EDA platforms, hosted synthesis services

---

### Tier 2: EDA Vendor / Tool Embedding

**For**: Software vendors embedding the HardwareScript compiler engine into proprietary desktop or enterprise tools

**Pricing**: $25,000 - $100,000 USD per year

**Includes**:
- All Tier 1 benefits
- Proprietary embedding rights
- Priority email support (24-hour response time)
- Feature request consideration
- Quarterly technical sync calls
- License covers up to 50 developers

**Perfect for**: EDA tool vendors, design automation companies, CAD platforms

---

### Tier 3: Enterprise & Foundry Compliance

**For**: Companies over $50M USD revenue, silicon foundries, or enterprises requiring custom agreements

**Pricing**: Custom negotiated agreements (typically $100,000+ USD per year)

**Includes**:
- All Tier 2 benefits
- Dedicated account manager
- Custom integrations and consulting
- Priority feature development
- Full legal indemnification and warranties
- Service Level Agreements (SLA)
- Unlimited developers
- Multi-year volume discounts

**Perfect for**: Large corporations, semiconductor foundries, defense contractors, enterprise legal compliance

---

## 4. What's Included in All Commercial Tiers

Every commercial license includes:

✅ **Legal Rights**
- AGPLv3 exemption for your specific use case
- No requirement to open-source your backend, API, or proprietary integrations
- Sublicensing rights for your products
- Patent grant protection

✅ **Technical Access**
- All stable releases and updates
- Bug fixes and security patches
- Access to documentation and examples
- Priority bug reports

✅ **Support**
- Email support (response time varies by tier)
- Technical implementation guidance
- Community forum access
- Legal compliance assistance

✅ **Protection**
- Patent grant (see PATENTS file)
- Warranty and indemnification (Tier 3)
- Legal compliance assistance

---

## 5. Self-Assessment: Do You Need a License?

### ✅ FREE - No License Needed

- **Hardware engineers** using `hwc` locally to design proprietary PCBs or ASICs
- **Startups** building IoT devices with closed-source `.hw` files
- **Defense contractors** designing classified hardware on air-gapped systems
- **Universities** teaching circuit design and semiconductor physics
- **Hobbyists** prototyping personal electronics projects
- **Open-source projects** sharing designs under AGPLv3 or permissive licenses

**Key principle**: If you run `hwc` on your own computer or CI/CD to generate hardware designs, you never need a license—even if you sell millions of physical devices.

---

### ❌ LICENSE REQUIRED

- **AI startup** offering "Prompt-to-Silicon" cloud service → **Tier 1**
- **SaaS platform** providing web-based PCB synthesis via API → **Tier 1**
- **EDA vendor** embedding `hwc-engine` in proprietary desktop software → **Tier 2**
- **Silicon foundry** integrating HardwareScript into closed-source design flow → **Tier 3**
- **Enterprise** with corporate policy banning AGPLv3 software → **Tier 3**
- **Cloud provider** offering hosted EDA tools with HardwareScript backend → **Tier 1**

---

## 6. Why This Licensing Model?

### Protects Against AI SaaS Free-Riding

The biggest threat to open-source compilers is not traditional software piracy—it's cloud AI platforms that wrap your engine in a proprietary API and capture 100% of the commercial value while contributing nothing back.

AGPLv3 Section 13 specifically addresses this: if you run HardwareScript on a server and let users interact with it over a network, you must open-source your entire platform—or buy a commercial license.

### Enables Frictionless Local Adoption

Hardware engineers, especially in defense and semiconductor sectors, work in air-gapped environments and will never adopt software that "phones home" or tracks usage. Our model requires zero telemetry and zero tracking.

You can use `hwc` locally forever, for free, without ever telling us—even if you're a Fortune 500 company shipping millions of devices.

### Transparent and Globally Fair

- No revenue thresholds that require financial audits
- No hidden usage tracking or phone-home requirements
- No complex per-seat or per-chip calculations
- Three clear tiers based on use case, not company size

## 7. Contact for Commercial Licensing

For commercial licensing inquiries: **hardwarescript@gmail.com**

**Subject Line**: "Commercial License Request - [Your Company Name]"

**Please include**:
1. Company name and location
2. Brief description of your use case (cloud/SaaS, embedding, fork, compliance)
3. Number of developers who will use HardwareScript
4. Which tier you believe applies to you (Tier 1, 2, or 3)
5. Any specific technical requirements or integration needs

**Response time**: We typically respond within 48 hours with:
- Confirmation of appropriate tier
- Exact pricing quote
- License agreement draft
- Payment options and next steps

---

## 8. Frequently Asked Questions

### "I'm building IoT devices and selling them. Do I need a license?"

**No.** If you use `hwc` locally to design hardware and sell physical products, you never need a license—even if you keep your `.hw` files proprietary. This applies regardless of company size or revenue.

### "I'm building an AI tool that generates HardwareScript code. Do I need a license?"

**It depends.** If your AI only generates `.hw` text files and users run `hwc` on their own machines, you don't need a license. If your cloud backend runs `hwc` to compile designs for users, you need a **Tier 1 Cloud License**.

### "We want to embed HardwareScript in our proprietary EDA tool. What do we need?"

You need a **Tier 2 Tool Embedding License** that grants you AGPLv3 exemption for proprietary integration.

### "Can we evaluate HardwareScript before committing to a license?"

Yes. For cloud/SaaS evaluation, you can test locally or use AGPLv3-compliant open-source prototypes. For commercial evaluation licenses, contact us to discuss terms.

### "What if we're a university or non-profit?"

Academic and non-profit use is free under AGPLv3. If you need a commercial exemption for specific partnerships, contact us for special academic pricing.

### "Our company policy prohibits AGPLv3 software. Can we still use HardwareScript?"

Yes. A **Tier 3 Enterprise Compliance License** provides full AGPLv3 exemption and legal indemnification for corporate environments with restrictive policies.

### "Can we pay in local currency or get PPP adjustments?"

Tier 1 and Tier 2 pricing can be adjusted for purchasing power parity and local economic conditions. Contact us to discuss payment options.

---

## 9. Legal Clarity

This commercial licensing policy supplements the AGPLv3 license. If you cannot or will not comply with AGPLv3 requirements (specifically Section 13 for network use), you must obtain a commercial license to use HardwareScript legally.

**The determination of licensing terms, including pricing, is at the sole discretion of the copyright holder (currently Olowookere Olamide, and in the future, the HardwareScript Foundation).**

**Key Legal Points:**
- Local CLI use is **always free** under AGPLv3
- Selling physical hardware designed with HardwareScript is **always free**
- Keeping your `.hw`/`.hwx` design files private is **always free**
- Running `hwc` on a server for others requires either AGPLv3 compliance or a commercial license
- Embedding in proprietary software requires a commercial license
- Forking/modifying the engine for distribution requires either AGPLv3 compliance or a commercial license

---

## 10. Future: Foundation-Based Licensing

Once the HardwareScript Foundation is established:
- All licensing revenue goes to the foundation (non-profit)
- Licensing policies will be reviewed periodically by the community
- Transparent reporting on licensing revenue and usage
- Community input on licensing approach and pricing

---

**Last Updated**: August 18, 2026  
**Version**: 3.0  
**Maintained by**: Olowookere Olamide

---

## Summary: The AGPLv3 + Commercial Dual License Advantage

This licensing model gives HardwareScript the best of both worlds:

✅ **Frictionless Grassroots Adoption**: Hardware engineers, startups, and hobbyists can download `hwc` and build proprietary boards/ASICs without paying a cent or fearing licensing traps.

✅ **Ironclad Protection Against AI Cloud Free-Riders**: Any venture-backed AI service that tries to wrap the engine into a cloud product will be stopped by AGPLv3 Section 13 and must pay for a commercial license.

✅ **No Telemetry or Usage Tracking**: Engineers in defense, semiconductor, and security sectors can use HardwareScript in air-gapped environments without phone-home requirements.

✅ **Sustainable Revenue**: Cloud platforms, EDA vendors, and enterprises that capture commercial value are required to contribute back through commercial licensing.

This is the same proven model used by Qt, MongoDB, and other successful dual-licensed infrastructure software.