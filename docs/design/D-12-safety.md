# D-12: Write safety assessment (patch-safety)

> Status: accepted
> Implements: [F-12](../features/F-12-safety.md) (slice 1)

## Goal

Turn workspace write approvals (`edit` / `write`) into a deterministic,
host-owned safety decision before any model or human review:

1. `[safety]` settings control whether constrained writes auto-approve.
2. The workspace provider projects its sandbox policy's writable roots into
   the approval request as evidence.
3. hostd's approval gateway assesses the write targets against those roots:
   fully constrained writes execute one-shot without a prompt or store grant;
   out-of-roots writes fail closed with a deterministic `safety_rejected`
   error; anything unassessable keeps the existing user flow.

## Constraints and non-goals

- hostd stays authoritative: the assessment runs in the approval gateway
  (F-07/F-11 precedent); orchd only supplies writable-root evidence and maps
  the resulting decision to a tool error.
- The store auto-accept check runs first (unchanged F-07); a previously
  granted write is accepted without re-assessment.
- Safety decisions never write session/workspace/permanent grants (one-shot,
  mirroring F-11 allows).
- Out-of-roots writes are rejected, not asked: piko has no path-grant
  mechanism and execution denies them regardless.
- Non-goals: per-file diff analysis, path-grant approval, `edit`/`write`
  tier changes, elicitation pause (slice 2), attestation (rejected in the
  PRD's fusion decisions).

## Proposed design

### 1. Settings: `[safety]`

`HostSettings` gains a `safety: Option<SafetySettings>` section:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct SafetySettings {
    /// Auto-approve workspace writes whose targets are fully inside the
    /// sandbox writable roots (one-shot, no store grant). Default: true.
    pub auto_approve_workspace_writes: Option<bool>,
}
```

- Field-level merge like `approvals`/`guardian`; `installed_settings_fixture()`
  documents the section; `resources/settings.toml` gains `[safety]`.
- `OrchAgentRunRunner::new_with_mcp` gains an optional `SafetySettings`
  parameter and resolves a small `SafetyConfig { auto_approve_workspace_writes: bool }`
  (default `true`).

### 2. `piko-sandbox`: writable-root projection

`Policy` gains a public helper that resolves its `write` roots against a
working directory (same normalization used by `authorize`):

```rust
impl Policy {
    /// Absolute, canonicalized writable roots resolved against `cwd`.
    pub fn writable_roots(&self, cwd: &Path) -> Vec<PathBuf>;
}
```

The root resolution reuses the existing private `root()` canonicalization so
the projection cannot drift from what `authorize` actually enforces.

### 3. `piko-orchd-api`: evidence and decision

- `ToolProvider` gains a defaulted method so providers can advertise
  enforceable writable roots:

  ```rust
  fn writable_roots(&self) -> Option<Vec<std::path::PathBuf>> { None }
  ```

- `ToolApprovalRequest` gains `writable_roots: Option<Vec<String>>`
  (absolute paths; `skip_serializing_if = "Option::is_none"`).
- `ToolApprovalDecision` gains:

  ```rust
  /// The request was rejected by the deterministic safety assessment.
  SafetyRejected { reason: String },
  ```

  `is_approval_accepted` excludes it (fail closed).

### 4. `piko-orchd`: provider + registry

- `WorkspaceToolProvider` implements `writable_roots()`: resolve the owned
  `Policy.write` roots against the current working directory
  (`Policy::writable_roots`), return the absolute strings.
- `ToolRegistryImpl::execute_tool`, when building the approval request for a
  write-capable tool whose approval tier requires review, attaches
  `provider.writable_roots()` (only the workspace provider returns `Some`).
- The registry's decision mapping adds `SafetyRejected { reason }` → a
  non-retryable error:

  | Decision | Tool error |
  |---|---|
  | `SafetyRejected { reason }` | `safety_rejected` — "Write rejected by safety assessment: {reason}" |

### 5. hostd domain: `domain/safety/mod.rs`

Pure logic with unit tests:

```rust
pub enum WriteSafetyDecision {
    AutoApprove,
    AskUser,
    Reject { reason: String },
}

pub fn assess_write_safety(
    tool_name: &str,
    args: &serde_json::Value,
    writable_roots: &[String],
) -> WriteSafetyDecision;
```

- Target extraction: `edit`/`write` read the `path` argument (string). Any
  other tool name, a missing/non-string `path`, or empty roots →
  `AskUser` (unassessable requests keep the existing flow).
- Containment: resolve the target against the session cwd (passed in), then
  lexically normalize both target and roots (drop `.`, resolve `..` without
  touching the filesystem — mirrors codex-rs `normalize`); the target is
  inside a root when it starts with that root.
- All targets inside a root → `AutoApprove`; any target outside every root →
  `Reject { reason }` naming the offending path.

`domain/mod.rs` exports the module.

### 6. Runner gateway: `approval_gateway.rs`

`OrchAgentRunRunner` gains `safety_config: SafetyConfig`. In
`request_tool_approval`, after the store auto-accept check and before the
guardian branch:

```text
if safety_config.auto_approve_workspace_writes:
    match assess_write_safety(request.tool_name, request.tool_args,
                              request.writable_roots, cwd):
        AutoApprove -> log tool.approval (decision = safety_auto_approved);
                       return ToolApprovalDecision::Accept   // one-shot
        Reject { reason } -> log tool.approval (decision = safety_rejected);
                       return ToolApprovalDecision::SafetyRejected { reason }
        AskUser -> continue (guardian / user flow unchanged)
```

The runner reads `request.writable_roots` (new field) and passes the session
cwd (already resolved via `session_cwd`). Non-write tools return `AskUser`
from the domain helper, so `bash`/`process`/`read` behavior is untouched.

### 7. Security hardening (same slice)

- **Deny hostd state**: the default permissive policy's deny list gains
  `.piko/` (next to `.git/`), so `edit`/`write` cannot touch
  `<cwd>/.piko/approvals.json` or project settings — the F-12 gate would
  otherwise auto-approve those in-roots writes and let the model self-grant
  approvals.
- **Path-level fingerprints**: `compute_path_fingerprint` emits
  `{tool}:{path}` instead of the bare tool name. A grant for one file no
  longer covers all paths (and never covers `.piko/` state).
- **TOCTOU re-verification**: `Policy::verify_resolved(cwd, input, access,
  must_exist, expected)` re-resolves the original input right before the
  write and rejects it if it no longer maps to the authorized path (symlink
  swap between authorization and execution). Both `edit` and `write` call it
  immediately before `tokio::fs::write`.
- **Edit correctness**: `edit` rejects empty `oldText`
  (`edit_requires_old_text`), non-unique matches (`edit_not_unique`, with up
  to three match line numbers), and returns an actionable `edit_not_found`
  message. `execute_workspace_tool` now takes an explicit `cwd` parameter so
  the tool is testable against temp directories.

Known residual risk (documented, not solved here): a hard link inside the
writable roots pointing at a file outside is indistinguishable from a normal
file by path checks, so the OS-level sandbox (when enabled) is the defense
in depth for that case — the same limitation codex-rs accepts for its
path-based sandboxes. The default command allowlist does not include link
creation (`ln`), so the agent cannot manufacture this precondition through
its own tools.

## Files touched

| File | Change |
|---|---|
| `packages/sandbox/src/policy.rs` | `writable_roots()` public helper |
| `packages/sandbox/src/policy.rs` | `verify_resolved()` TOCTOU guard + tests |
| `packages/orchd-api/src/tools.rs` | `ToolProvider::writable_roots()` default method |
| `packages/orchd-api/src/approval.rs` | `writable_roots` field; `SafetyRejected`; `is_approval_accepted` |
| `packages/orchd/src/adapters/tools/workspace_provider.rs` | implement `writable_roots()` |
| `packages/orchd/src/adapters/tools/registry.rs` | attach roots to approval request; map `SafetyRejected` |
| `packages/orchd/src/adapters/tools/registry_tests.rs` | decision-mapping tests |
| `packages/orchd/src/adapters/tools/workspace_handlers.rs` | explicit `cwd` param; edit uniqueness/not-found errors; `verify_resolved` before writes; `.piko` denial tests |
| `packages/orchd/src/runtime/utils.rs` | default deny gains `.piko/` |
| `packages/hostd/src/adapters/turns/approval.rs` | path-level fingerprints + tests |
| `packages/hostd/src/domain/safety/mod.rs` | assessment, normalization, tests |
| `packages/hostd/src/domain/mod.rs` | export `safety` |
| `packages/hostd/src/domain/config/settings.rs` | `SafetySettings`, merge, defaults template |
| `packages/hostd/resources/settings.toml` | `[safety]` section |
| `packages/hostd/src/adapters/turns/orch_runner/mod.rs` | field + constructor param |
| `packages/hostd/src/adapters/turns/orch_runner/approval_gateway.rs` | assessment branch |
| `packages/hostd/src/adapters/turns/orch_runner/tests.rs` | gateway acceptance tests |
| `packages/hostd/src/protocol/orch_factory.rs` | pass `settings.safety` |
| `docs/features/F-12-safety.md`, `docs/agent-runtime-roadmap.md`, `docs/features/README.md` | status updates |
| `docs/verification/V-12-safety.md` | acceptance evidence |

## Verification

- Unit tests: containment (in-roots, out-of-roots, `..` traversal, missing
  path, non-string path, empty roots, non-write tool); settings merge for
  `[safety]`; defaults template.
- Registry tests: `SafetyRejected` maps to a distinct non-retryable
  `safety_rejected` error; `is_approval_accepted` is false.
- Sandbox tests: `verify_resolved` accepts stable paths and rejects a
  swapped symlink target.
- Approval tests: path-level fingerprints (`edit:<path>`), one-path grants
  never match another path.
- Workspace handler tests: unique edit applies; empty `oldText` rejected;
  non-unique edit rejected with line numbers; not-found message guides the
  model; `write`/`edit` into `.piko/` denied and file untouched.
- Hostd gateway tests with a real runner:
  - in-roots `edit` returns `Accept` without publishing a pending approval or
    writing a grant; an identical second request is assessed again;
  - out-of-roots `write` returns `SafetyRejected { reason }`;
  - no writable roots → user flow (pending entry created, user decision
    resolves);
  - `auto-approve-workspace-writes = false` → user flow for in-roots writes;
  - `bash` approval request is unaffected (user flow).
- `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test -p piko-sandbox -p piko-orchd-api -p piko-orchd -p piko-hostd`.
