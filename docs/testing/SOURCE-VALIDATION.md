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

## Current assertion review

[The primary source-review receipt](current-assertion-review.json) covers all 606 original assertion
records (411 unique obligations): 385 Rust obligations grouped by 64 complete files and 26 explicit
SDK translations. It pins the original assertion aggregate, all 81 current assertion/helper source
files and exact SDK excerpt ranges. The reviewer is the primary implementer, not an independent
reviewer. Authorized executable/catalog adaptations are identified separately from source continuity.

`tooling/src/source_review.rs` rejects missing, duplicated, unknown or stale obligations, unsupported
review labels, changed source/helper bytes and mismatched SDK traceability. Its integration contract
loads the actual committed ledger, historical reference and review, rather than a synthetic substitute.
A review can omit its own future commit hash; its complete content hashes still bind the source, and
the authenticated rebind and execution reports must identify the same exact current revision.

The execution-matrix collector includes the receipt hash in tracked provenance and reconstructs each
of the seven named-case reports before joining it to the source review. Neither a review disposition
nor a successful named test alone authorizes retirement. Semantic qualification, migrated tooling
scenario coverage and signed delivery remain separate requirements; the current collector deliberately
reports `semantic_parity_claimed=false` and `qualification_verified=false`.

## Current tooling harness binding

`tooling/src/tooling_execution.rs` joins the exact 46-file disposition manifest and five retained
PowerShell identities to the built workspace report. It reuses the existing strict execution verifier
and checks original/replacement/test-helper hashes against authenticated source provenance. Every
applicable declared harness needs nonempty successful named results. Each original also declares
exact `named_tests`: attributed Rust tests must resolve uniquely in the correct harness, so an
unrelated passing test cannot satisfy the mapping. SDK child tests use their root harness. The five
source-verified platform-specific mappings retain explicit non-applicability, not fabricated passes.

Each original's `scenario_tests` also assigns every unchanged scenario to explicit tests from its
`named_tests`. The binder rejects missing, extra or duplicate scenarios, empty/duplicate/unknown test
assignments and named tests left unassigned. It records each scenario's exact named results, including
explicit platform non-applicability; it does not infer semantic equivalence from those assignments.

The matrix reconstructs this binding only for the six built contexts, not runtime-only coverage.
The binding records declared scenario and exact named-test execution: it does **not** prove that every
described scenario or assertion is equivalent. Synthetic contract reports test validator shape only. Scenario equivalence,
signing and retirement remain separate gates; the binder keeps `scenario_assertions_verified=false`,
`qualification_verified=false` and `retirement_authorized=false`.

## Formatting-failure diagnostics

A failed hosted formatting gate remains failed. After execution and source-provenance checks,
`tooling/src/formatting.rs` runs the bounded formatter and retains `formatting.zip` for review.
It contains only tracked Rust sources from the runtime, tests and tooling, with strict UTF-8,
regular-file/link checks, duplicate rejection and a 16MiB aggregate bound. The workflow uploads
that exact diagnostic archive only when it exists after failure. Diagnostic candidates are not
successful execution evidence, a completed build or permission to apply changes automatically.

## Current qualification limits

The 0.3.0 tooling/media candidate has not qualified. Exact `49a6288` failed all seven native/coverage
jobs in [qualification41526803432, attempt1](https://github.com/KiaroSama/Numera-Gen-MCP/actions/runs/41526803432/attempts/1).
Unix task/release compilation did not overcome failed SDK/tooling test compilation and format/Clippy
gates. Windows bootstrap rejected authenticated ZIP bytes through GNU tar before Rust setup.
The corrective tree repaired those diagnostics and rebound reviewed source hashes/ranges after applying
authenticated hosted formatting. Replacement41554639558 on exact `b72e20e` also failed: Windows now
compiled the task but deep fixture materialization overflowed its stack; Unix execution exposed
media-schema, example-path, numeric metadata, archive-prefix, deep fixture and SDK-race failures plus
remaining tooling quality gates. The next corrective tree retains iterative authoritative JSON
through fixture lifetime/relocation and exact original provenance, and still needs hosted execution. No Rust compiler,
formatter, build or test ran locally. The earlier isolated lock/format preparation is not qualification.
All discovered workspace suites, six native targets, instrumented runtime 85/80 floors, current
470-case assertion bindings, extracted SDK behavior and signatures must qualify on the final SHA.
The owner separately authorized the existing signing identity on `main`; workflow and native guards
require the signing job and authenticated qualification. The source-authored `sign-qualified` route
selects an exact successful original run/attempt at the identical SHA, retains its original binaries
and executes only outstanding signing/extracted-signed acceptance, not the earlier native/coverage
suites again. The final collector joins both execution identities. This route still needs actual
hosted proof and dispatch authority; it does not clear semantic or retirement gates. The isolated
source-preparation route receives no signing inputs and returns reviewed lock/format candidates only;
it proves no runtime acceptance.
The installed signed 0.2.0 runtime and private configuration/receipts/originals remain intact.

Official references: [Cargo locked builds](https://doc.rust-lang.org/cargo/commands/cargo-build.html),
[Cargo workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html),
[historical npm ci](https://docs.npmjs.com/cli/v11/commands/npm-ci),
[historical npm audit](https://docs.npmjs.com/cli/v11/commands/npm-audit),
[Node24 child lifetime](https://nodejs.org/download/release/v24.21.0/docs/api/child_process.html),
[Zod4.6.7](https://github.com/colinhacks/zod/releases/tag/v4.6.7) and
[bounded-collection repair](https://github.com/colinhacks/zod/pull/6652).
