# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |
| < 0.1   | :x:                |

## Reporting a Vulnerability

If you discover a security vulnerability within Hardware Script, please send an email to hardwarescript@gmail.com. All security vulnerabilities will be promptly addressed.

**Please do not report security vulnerabilities through public GitHub issues.**

### What to Include

When reporting a vulnerability, please include:

- A description of the vulnerability
- Steps to reproduce the issue
- Potential impact
- Suggested fix (if any)

### Response Timeline

- **Acknowledgment**: We will acknowledge receipt of your vulnerability report within 48 hours.
- **Assessment**: We will assess the vulnerability and determine its impact within 5 business days.
- **Resolution**: We will work on a fix and release it as soon as possible.
- **Disclosure**: We will coordinate with you on the timing of public disclosure.

### Scope

This security policy applies to:

- The Hardware Script compiler (`hwc`)
- The package manager (`hpm`)
- The documentation engine (`hwsd`)
- The live monitor (`hsm`)

### Out of Scope

- Vulnerabilities in third-party dependencies (report these to the respective projects)
- Issues that require physical access to the user's machine
- Social engineering attacks

## Security Best Practices

### For Users

- Always verify the integrity of downloaded binaries
- Use the official installation methods
- Keep your tools updated to the latest version
- Report any suspicious behavior immediately

### For Contributors

- Follow secure coding practices
- Never commit secrets, keys, or credentials
- Review dependencies for known vulnerabilities
- Use dependency scanning tools

## Disclosure Policy

We follow a coordinated disclosure process:

1. **Report**: Researcher reports vulnerability privately
2. **Acknowledge**: We acknowledge receipt within 48 hours
3. **Assess**: We assess severity and impact
4. **Fix**: We develop and test a fix
5. **Release**: We release the fix
6. **Disclose**: We publicly disclose the vulnerability

We request that researchers:

- Give us reasonable time to address the issue before public disclosure
- Avoid exploiting the vulnerability beyond what is necessary to demonstrate it
- Do not access or modify other users' data

## Bug Bounty

Currently, Hardware Script does not offer a bug bounty program. However, we deeply appreciate security researchers who help us improve the project. Contributors who report valid security issues will be credited in the release notes (unless they prefer to remain anonymous).

## Contact

- **Security Email**: hardwarescript@gmail.com
- **Subject Line**: [SECURITY] Brief description
- **GitHub**: https://github.com/HardwareScript

---

**Last Updated**: July 2026
**Version**: 1.0
