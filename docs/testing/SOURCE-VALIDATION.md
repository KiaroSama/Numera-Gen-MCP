# Immutable oracle, historical validation and current Rust qualification

The native migration has separate evidence sources. Historical success never qualifies a changed
working tree, and a source mapping is not execution or assertion-equivalence evidence.

## Immutable oracle

The historical TypeScript baseline is exactly `fc7da25c3b470199949b1b4c67127d77afa0c34a`.
Its original source, lockfile, independently collected 470 cases/50 suites and source-generated
expected inputs remain immutable. It is an authenticated comparison fixture, not a supported Node
production distribution. Native end-user packages contain no Node, Zod or Undici runtime.

The current Rust tooling reads the retained public collections and independently hash-bound Git
archives. It does not run an interpreter or generate expected values from the target implementation.
Original same-user DPAPI ciphertext is not portable public evidence; its isolated Windows fixture
must be produced through the real native protection contract in hosted CI.

## Historical maintained-source validation

The released native runtime at `f2535577c2e3145da03e2d5c123d7de061656860` used a separate maintained
validation tree. The retained `scripts/native-validation.py` documents that historical procedure;
it is not a new supported Rust maintenance command. Exactly **four paths** changed from the baseline:

- `package.json` and `package-lock.json`: Zod `4.6.5` to `4.6.7` and Undici `8.11.2` to `8.12.3`,
  with exact registry resolutions/integrities. Other dependency metadata remained unchanged.
- `scripts/bounded.mjs`: the reviewed maintained supervisor joined owned child close rather than
  treating a recycled numeric PID as identity. Permission failures required independent cleanup proof.
- `tests/integration/setup.test.ts`: one buffered PowerShell child fixture became a streamed child.
  Its original assertions and 10000ms child ceiling remained; unrelated test/application bytes did not change.

The receipt records before/after hashes and unchanged tests. Historical validation retained the
coverage, stdio, clean packed-install, type, lint, build, format and audit gates without skips,
weaker limits or ignored exit codes. Its bounded Zod control used three elements against a maximum
of two, not a resource-exhaustion input. Zod `4.6.5` is affected by
[GHSA-mrhx-c7xc-m3w5](https://github.com/advisories/GHSA-mrhx-c7xc-m3w5); the frozen baseline is not
advertised as audit-clean. Those historical dependency pins are not claims about today's latest release.

The original macOS frozen supervisor reported unverified cleanup after a descendant was absent;
its exact failed syscall was not captured. Later hosted success does not establish that original cause.
[Signed delivery](../releases/0.2.0.md) records the final historical qualification, separate from
owner-host reconnection and the current candidate.

## Current Rust tooling

`tooling/src/validation.rs` prepares and verifies the same four-file transformation **as data**.
It rejects unexpected dependency versions, paths, duplicate names, fixture literals, changed bytes
and receipt drift. `tooling/src/ledger_inputs.rs` authenticates the frozen inputs and historical
maintained supervisor before using this transformation. This neither installs those packages nor
executes the archived JavaScript. The receipt explicitly reports `interpreted_execution=false`.

The hosted-only `numera-task --root ABSOLUTE_ROOT ledger` command checks historical identities and
emits current Rust mapping proposals. Its source report deliberately keeps `acceptance_verified=false`:
current assertion ranges/reviews and exact named executions remain separate required work.
The 46-file Python/JavaScript disposition manifest and five unchanged PowerShell files are tracked
in [the tooling migration inventory](tooling-migration.json). Counts and existing test files do not
qualify retirement; `retirement-gate` refuses until genuine current qualification is bound.

## Current qualification limits

The 0.3.0 tooling/media candidate is source-authored, not compiled, formatted or executed locally.
Its workspace lockfile requires hosted resolution and reviewed formatting before a final locked run.
All discovered workspace suites, six native targets, instrumented runtime 85/80 floors, current
470-case assertion bindings, extracted SDK behavior and signatures must qualify on the final SHA.
The owner separately authorized the existing signing identity on `main`; workflow and native guards
require a manual qualification and the signing job. The isolated source-preparation route receives no
signing inputs and returns reviewed lock/format candidates only; it proves no runtime acceptance.
The installed signed 0.2.0 runtime and private configuration/receipts/originals remain intact.

Official references: [Cargo locked builds](https://doc.rust-lang.org/cargo/commands/cargo-build.html),
[Cargo workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html),
[historical npm ci](https://docs.npmjs.com/cli/v11/commands/npm-ci),
[historical npm audit](https://docs.npmjs.com/cli/v11/commands/npm-audit),
[Node24 child lifetime](https://nodejs.org/download/release/v24.21.0/docs/api/child_process.html),
[Zod4.6.7](https://github.com/colinhacks/zod/releases/tag/v4.6.7) and
[bounded-collection repair](https://github.com/colinhacks/zod/pull/6652).
