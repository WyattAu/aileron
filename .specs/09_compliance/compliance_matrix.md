# Compliance Matrix

## Standard Compliance Status

### IEEE 1016-2009: Software Design Descriptions

| Clause | Description | Implementation | Evidence | Status |
|--------|-------------|----------------|----------|--------|
| 5.1 | Design Overview | BP-APP-CORE-001 BP-1 | Blue Paper | COMPLIANT |
| 5.2 | Design Decomposition | BP-APP-CORE-001 BP-2 | Blue Paper | COMPLIANT |
| 5.3 | Design Rationale | BP-APP-CORE-001 BP-3 | Blue Paper | COMPLIANT |
| 5.4 | Traceability | BP-APP-CORE-001 BP-4 | Traceability Matrix | COMPLIANT |
| 5.5 | Interface Design | BP-APP-CORE-001 BP-5 | Interface Contracts | COMPLIANT |
| 5.6 | Data Design | BP-APP-CORE-001 BP-6 | Blue Paper | COMPLIANT |
| 5.7 | Component Design | BP-APP-CORE-001 BP-7 | Blue Paper | COMPLIANT |
| 5.8 | Deployment Design | BP-APP-CORE-001 BP-8 | Blue Paper | COMPLIANT |

### NIST SP 800-53 Rev 5: Security Controls

| Control | Name | Implementation | Status |
|---------|------|----------------|--------|
| AC-3 | Least Privilege | Lua sandboxing, MCP auth | PLANNED |
| AC-4 | Information Flow Enforcement | Trust boundaries | PLANNED |
| AU-2 | Audit Events | MCP audit logging | PLANNED |
| CM-2 | Baseline Configuration | Default secure config | PLANNED |
| IA-2 | User Identification | MCP token auth | PLANNED |
| SC-7 | Boundary Protection | Trust boundary enforcement | PLANNED |
| SC-8 | Transmission Confidentiality | Local-only MCP | PLANNED |

### OWASP Top 10 (2021)

| Category | Mitigation | Status |
|----------|-----------|--------|
| A01: Broken Access Control | MCP auth, per-pane isolation | PLANNED |
| A02: Cryptographic Failures | Credential zeroization | PLANNED |
| A03: Injection | Lua sandboxing, per-pane JS isolation | PLANNED |
| A04: Insecure Design | DOM sanitization for MCP | PLANNED |
| A05: Security Misconfiguration | Secure defaults | PLANNED |
| A06: Vulnerable Components | cargo audit in CI | PLANNED |
| A07: Auth Failures | MCP token auth | PLANNED |
| A08: Data Integrity | Filter list checksums | PLANNED |
| A09: Logging Failures | SensitiveString type | PLANNED |
| A10: SSRF | URL whitelist for MCP tools | PLANNED |

### ISO/IEC 27001:2022

| Clause | Description | Status |
|--------|-------------|--------|
| A.5.1 | Policies for information security | NOT APPLICABLE (open-source, no org) |
| A.8.1 | Asset management | NOT APPLICABLE |
| A.8.25 | Secure development lifecycle | COMPLIANT (R&D lifecycle) |

### XDG Base Directory Specification

| Requirement | Implementation | Status |
|-------------|----------------|--------|
| Config files in $XDG_CONFIG_HOME | `~/.config/aileron/` | PLANNED |
| Data files in $XDG_DATA_HOME | `~/.local/share/aileron/` | PLANNED |
| Cache in $XDG_CACHE_HOME | `~/.cache/aileron/` | PLANNED |
| Respect environment variables | `directories` crate | PLANNED |
