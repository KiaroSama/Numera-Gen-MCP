# Rust migration

This change continues `chore/tooling-media` at `030297fbb9799e2b2a42501057e5e503d179ccc3`
in a separate PR against `main`. It does not merge or publish a release.

The MCP runtime, developer utility, CI/bootstrap tools and maintained tests use Rust 2024 with
Rust 1.99.1. The `numera-task` workspace package replaces all 51 Python, JavaScript and PowerShell
tooling sources. Original file hashes, behavior scenarios, test identities and independently
generated expected bytes remain in `docs/testing` and immutable oracle fixtures. Archived source
is evidence and is never a runtime fallback or an executable CI dependency.

The original baseline has 50 suites and 470 cases: 178 unit, 132 integration and 160 contract cases.
The Rust scenario inventory preserves those identities; Cargo discovers all current native and
tooling suites. No existing Rust test has been removed. Tests still check provider wire requests,
real SDK stdio/HTTP sessions, persistence/recovery, bounded child processes, packaging and signatures.
Production coverage floors remain 85% regions and 80% branches.

CI runs on every push and pull request, as well as manual dispatch. All six Windows/Linux/macOS
x64/ARM64 contexts run quality gates, tests, release builds, dependency notice collection and
extracted-binary SDK checks. Automatic CI cannot select source preparation or signing. Signing
still requires the protected environment and a manual invocation on `main`.

The current repository contains no standalone web-panel or Worker source tree. No external web
panel, gateway dashboard or Worker implementation is migrated by this PR. Their original languages
and deployment boundaries are preserved.

## Branch disposition

| Branch / PR | Disposition in this PR |
| --- | --- |
| `chore/tooling-media` | Full reference history is included as the new branch's ancestor. |
| Cargo updates #17–#21 | Superseded by RMCP 3.9.0, rusqlite 0.41.1, libc 0.2.191, SHA2 0.11.1 and UUID 1.29.2 already integrated from the reference branch. |
| Node updates #9, #10, #14–#16 | Obsolete: maintained Node tooling and setup-node have been replaced by Rust. Frozen historical inputs must retain their original versions. |

All ten superseded PRs are closed. The reference branch and all remaining Dependabot branches
have been removed after preserving the reference history and checking the superseding versions.
Only `main` and `rewrite/rust-ci-completion` remain. `main` is not merged.
Latest upstream review and the reasons for applying or declining each change are
recorded in [references](REFERENCES.md). Current execution results belong in [test results](TEST-RESULTS.md).

Repository topics and the About description were updated separately through GitHub settings and
verified through the repository API. Topics now include `rust` and `cross-platform`; `typescript`
was removed and other topics were preserved. About describes the native Rust image/video/audio/3D
MCP server. These metadata changes do not require merging this PR.
