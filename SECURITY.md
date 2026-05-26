# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.20.x  | :white_check_mark: |
| < 0.20  | :x:                |

## Reporting a Vulnerability

**Do not report security vulnerabilities through public GitHub issues.**

Instead, report them via:

- **GitHub Security Advisories:** [Report a vulnerability](https://github.com/WyattAu/aileron/security/advisories/new)
- **Email:** Send a PGP-encrypted email to the maintainer (include your public key)

### What to Include

- Description of the vulnerability
- Steps to reproduce or proof-of-concept
- Affected versions
- Potential impact (remote code execution, information disclosure, privilege escalation, etc.)
- Any suggested mitigations

### Response Timeline

| Action | Target |
|--------|--------|
| Acknowledgment | Within 48 hours |
| Initial assessment | Within 5 business days |
| Fix or mitigation plan | Within 30 days |
| Security advisory published | With the next release |

### Disclosure Policy

- Coordinated disclosure: we will work with you to publish an advisory after a fix is available.
- Please do not disclose the vulnerability publicly until a fix has been released.
- We will credit researchers in the security advisory (unless you prefer to remain anonymous).

## Security Measures

- All 19 `unsafe` blocks have `// SAFETY:` comments explaining the invariant
- Zero `panic!()` in production code
- Zero `todo!()` / `unimplemented!()` in production code
- CI runs `cargo audit` on every push
- Dependency SBOM maintained in SPDX format
- Pre-commit hooks enforce clippy with `-D warnings`
