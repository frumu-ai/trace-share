# Security Audit Report

Date: 2026-02-25  
Scope: `trace-share` Rust CLI/core crates and security-relevant data flows.

## Method

- Manual code review of authentication, transport, local storage, external-process execution, and privacy-sensitive paths.
- Static spot checks via `rg` for network/file/process primitives and sensitive pattern handling.
- Test execution to detect runtime regressions in security-adjacent behavior.

## Findings

### 1) Missing transport security enforcement for authenticated outbound requests (High)

**What I found**

Multiple upload/index/revocation code paths accept user-configured URLs and send bearer tokens without enforcing `https://` schemes. If a user (or compromised config) sets an `http://` endpoint, API tokens and payloads can be exposed to interception or tampering.

- Worker uploads build endpoint from `worker.base_url` and attach bearer auth.  
- Revocation pushes do the same.  
- Upstash publish/index calls build endpoint from `upstash.rest_url` and attach bearer auth.  
- Remote registry fetch also accepts arbitrary URL without scheme restriction.

**Evidence**

- `worker.base_url` is used directly for authenticated POSTs.  
  `crates/trace-share-core/src/worker.rs` lines 49-67, 167-185.
- `upstash.rest_url` is used directly for authenticated POSTs.  
  `crates/trace-share-core/src/publish.rs` lines 40-52, 66-70, 146-159, 194-198.
- Remote registry URL is fetched directly via HTTP client.  
  `crates/trace-share-core/src/sources.rs` lines 181-200, 207.

**Risk**

- Credential disclosure (bearer tokens).
- In-transit tampering of uploaded episodes/metadata.
- Registry poisoning if insecure transport is used.

**Recommendation**

- Validate URL schemes at config load and before request execution; reject non-HTTPS URLs by default.
- Allow explicit insecure override only via dedicated opt-in env flag intended for local testing.
- Consider host allowlist/certificate pinning for high-assurance deployments.

---

### 2) Sensitive local artifacts written without explicit restrictive permissions (Medium)

**What I found**

Security-sensitive local files are created with default process umask rather than explicitly setting owner-only permissions.

- Anonymization salt (`anonymization_salt`) is written without `0600`/equivalent hardening.
- Registry cache is written similarly.
- Local sources manifest write path likewise uses default perms.

**Evidence**

- Salt creation write call: `fs::write(path, &salt)`.  
  `crates/trace-share-core/src/publish.rs` line 136.
- Registry cache write call: `fs::write(cache_path, ...)`.  
  `crates/trace-share-core/src/sources.rs` line 236.
- Sources manifest write call: `fs::write(&path, text)`.  
  `crates/trace-share-core/src/sources.rs` line 177.

**Risk**

- On permissive umask/shared systems, other local users may read sensitive operational metadata (or anonymization salt, which weakens privacy unlinkability assumptions).

**Recommendation**

- Use atomic file creation with explicit permissions (`OpenOptions` + `set_permissions`, or platform-specific secure defaults).
- Restrict security-sensitive state files to owner-read/write.

---

### 3) External tool discovery trusts `PATH` for `gitleaks` binary (Low/Medium)

**What I found**

The sanitization flow auto-executes a `gitleaks` binary discovered by scanning `PATH` and only checks file existence. This can execute a malicious binary if `PATH` is poisoned in the runtime environment.

**Evidence**

- Binary search and selection from `PATH` via `candidate.exists()`.  
  `crates/trace-share-core/src/sanitize.rs` lines 350-366.
- Selected binary is executed with `Command::new(gitleaks_bin)`.  
  `crates/trace-share-core/src/sanitize.rs` lines 288-299.

**Risk**

- Arbitrary code execution in user context when sanitization runs.

**Recommendation**

- Require explicit configured absolute path for `gitleaks` in hardened mode.
- Optionally verify binary integrity/signature or constrain search to trusted directories.

---

### 4) Automatic update check performs network call on every startup (Informational)

**What I found**

CLI startup performs a GitHub request prior to command dispatch unless disabled via env var.

**Evidence**

- Update check called in `main` before command handling.  
  `crates/trace-share-cli/src/main.rs` lines 235-239.
- Remote call to GitHub release API.  
  `crates/trace-share-cli/src/main.rs` lines 260-277.

**Risk**

- Metadata/privacy concern in restricted environments (command invocation leaks to external service by default).

**Recommendation**

- Consider opt-in update checks (or first-run prompt).
- Document behavior clearly in privacy/security docs.

## Positive controls observed

- Sanitization includes multiple regex-based secret/PII detectors and entropy/JWT/PEM handling.  
  `crates/trace-share-core/src/sanitize.rs` lines 34-74, 76-168, 184-261.
- Upload path blocks if sensitive patterns still detected post-sanitize in text/metadata.  
  `crates/trace-share-core/src/publish.rs` lines 25-37, 178-180.
- Source definitions are validated for traversal segments and constrained to allowlisted roots.  
  `crates/trace-share-core/src/sources.rs` lines 336-377.

## Suggested remediation order

1. Enforce HTTPS + explicit insecure override gate.
2. Harden local file permissions for salts/state/cache/config artifacts.
3. Harden external binary trust model for `gitleaks` integration.
4. Make update checks opt-in or more explicitly controllable.
