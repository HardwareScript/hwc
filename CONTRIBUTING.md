# Contributing to Hardware Script

Welcome to Hardware Script! We are thrilled that you want to help us bring software-speed iteration to hardware design.

To maintain the blazing-fast performance, architectural purity, and extreme reliability of the compiler, Hardware Script operates on an **Issue-Driven Development** model (similar to SQLite).

Please read this document carefully before contributing.

---

## The "Open Source, Closed Core" Philosophy

Hardware Script is proudly 100% open-source under the AGPLv3 license. However, **we generally do not accept Pull Requests (PRs) that modify the core compiler codebase (`hwc`).**

### Why do we reject core Pull Requests?
1. **Architectural Purity:** Hardware Script follows a strict "C-like" philosophy. We keep the core extremely lean and push domain complexity to the Standard Library and Package Registry. 
2. **The LLM Era:** AI-generated PRs often look functional but introduce subtle performance degradations or violate our strict memory/voxel architectures.
3. **Legal Clarity:** Because Hardware Script offers a dual-license model for enterprises, the core team must retain clean, 100% copyright ownership over the compiler engine. By writing the code internally, we avoid the need for complex Contributor License Agreements (CLAs).

---

## How to Contribute (The Right Way)

While we don't accept core PRs, community contribution is the lifeblood of this project. Here is how you can make a massive impact:

### 1. Architectural & Optimization Suggestions (Issues)
Did you find a way to make the compiler faster? Do you know a superior mathematical approach to Manhattan routing or 3D voxel parsing? 
* **Do not write a Pull Request.**
* **Open an Issue** tagged as `Optimization` or `Architecture`.
* Explain your logic, share your research, or drop a pseudo-code snippet. 
* Our internal team will review it, research it, and if it aligns with our high-performance ideology, *we will implement the core Rust code ourselves* and credit you in the release notes.

### 2. Bug Reports
If you find a gap where the compiler fails, panics, or produces invalid physics/geometry:
* Open a **Bug Report Issue**.
* Include the `.hw` snippet that caused the failure.
* Include the exact terminal output and `hwc` version.
* We will investigate and patch the engine internally.

### 3. Build the Ecosystem (Write Packages!)
If you want to write actual code and build things, **the HPM Registry is where you belong.** 
The core compiler is intentionally primitive. The real power of Hardware Script comes from community packages.
* Don't try to PR the Standard Library. If you want better RF components, create an HPM package!
* Build vendor-specific chips (e.g., `@espressif/esp32`).
* Build domain-specific unit libraries (e.g., `@aerospace/units`).
* Publish them to the open registry for the world to use.

### 4. Documentation & Typo Fixes
We **DO** accept Pull Requests for the `Docs/` folder. 
If you find a typo, want to improve a tutorial, or translate documentation, you are welcome to submit a PR for those specific markdown files.

---

## Opening an Issue

When opening an issue, please select the appropriate template:

1. **Bug Report**: The compiler crashed or generated invalid geometry.
2. **Optimization Idea**: A mathematical or algorithmic suggestion to make the compiler faster or lighter.
3. **Language Syntax Proposal**: A suggestion for the `.hw` language specification (must adhere to our zero-magic, Python/Ruby-style aesthetic).

### Example of a Great Optimization Issue:
> **Title**: Replace HashMap with Flat Array for Voxel Lookups
> **Description**: I noticed in Layer 3 that you are using `FxHashMap` for voxel collision detection. Because the grid bounds are known at compile time, switching to a 1D flat `Vec` with Z-curve (Morton) indexing would increase CPU cache hits and likely speed up routing by 3x. Here is a link to a paper on the math.

*We love issues like this. We will read the paper, benchmark it, and write the implementation.*

---

## The Standard Library (`@std`)

The standard library ships with the compiler. We treat it with the same strictness as the core rust engine. 
* If you find a bug in a standard component's footprint or physics, **open an Issue**.
* If you want a new component that doesn't exist in `@std`, **do not request it to be added**. Instead, build it as an HPM package and publish it to the community registry! The `@std` library is strictly reserved for irreducible baseline primitives.

---

## Summary

* **Found a bug?** Open an Issue.
* **Have a genius optimization?** Open an Issue.
* **Want to fix a typo in the docs?** Submit a PR.
* **Want to write hardware code?** Build an HPM package!

Thank you for helping us build the future of hardware design. By following this Issue-Driven workflow, we ensure Hardware Script remains the fastest, cleanest, and most reliable hardware compiler in the world.
