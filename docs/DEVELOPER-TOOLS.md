# Native developer tools

`numera-dev` is a separate optional developer executable. Numera Gen MCP runs directly as
`numera-gen-mcp bridge`; it does not launch this helper. The ten legacy image tools remain, with
additive typed media tools documented by the current source contract. Historical signed 0.2.0
archives retain their original executable name; the older execution evidence below applies only
to its recorded source, not the rebranded multimodal candidate.
At `6bffdf8`, all six targets compiled and passed release/SDK checks, and complete coverage passed
at 86.10% regions / 83.73% branches. Strict Clippy, macOS exited-group cleanup and a Windows x64
DPAPI fixture still failed. At `0dbba1f`, the macOS ARM and Windows x64 regressions passed, but
Windows ARM's new descendant observer failed and formatting still blocked the stable jobs. The
observer compares creation time as well as PID. At `a8a8912`, it confirmed an original Windows
descendant was still nonsignaled after empty-job accounting. At `c2846b9`, retained, membership-checked
handles fixed the observed failure: all 100 native targets passed on all six platforms, including both
Windows descendant observers and DPAPI cases, within the unchanged two-second cleanup ceiling. Strict
Clippy, release and SDK checks passed; complete coverage passed at 86.07% regions / 83.76% branches.
At `443afc9`, all ten hosted jobs passed, including all-six formatting, native tests, release/SDK,
notices, internal archives and extracted stdio/HTTP execution. These are unsigned internal intermediates;
full baseline acceptance subsequently passed at `b513884` / run `39519759155`, including all 470 cases,
50 suites and 103 harnesses per native context. Exact `80e8a61` / run `40037496390` subsequently
passed all sixteen jobs and six native signature/package checks. Final review then reproduced two
maintenance defects: detached-only activation and truncated Windows exit codes. Both corrections passed
at `f253557` / run `40244894089`, attempt2, with all sixteen jobs successful. Signed maintenance passed
all six targets; the original CI archives are published as0.2.0. Owner-host reconnection remains separate.

## Hosted maintenance workspace

The source candidate adds `tooling/`, a separate Cargo workspace package producing `numera-task`.
It reuses native supervision, logging and artifact verification; task-only archive dependencies are
not production server dependencies. It does not wrap Python or JavaScript. The five original
PowerShell scripts remain unchanged. The candidate has not yet been compiled or executed.

Every task command requires an explicit absolute root and hosted environment. For example, on an
already prepared hosted runner:

```bash
./target/debug/numera-task --root "$GITHUB_WORKSPACE" inventory
```

| Command group | Purpose and qualification boundary |
| --- | --- |
| `fixtures`, `evidence-capture`, `evidence-verify` | Materialize independently retained public expected inputs and bind their provenance; never generate expectations from the implementation under test. |
| `inventory`, `stable`, `coverage`, `coverage-gate` | Check original tooling dispositions and Cargo suite discovery; run workspace gates and enforce production-runtime 85/80 floors. |
| `ledger`, `case-execution`, `execution-matrix`, `retirement-gate` | Authenticate historical 470-case/50-suite evidence, propose current mappings and bind current exact names per context and across seven hosted contexts. Semantic review and release qualification remain separate; retirement intentionally refuses. |
| `licenses TARGET`, `prepare TARGET`, `stage TARGET` | Preserve actual dependency notices and create bounded allowlisted intermediates/prepared distributions. `prepare` is explicitly unsigned, not a completed app build. |
| `signing-tools TARGET`, `sign TARGET`, `package TARGET`, `signature-acceptance TARGET`, `extract-intermediate TARGET`, `extract-signed TARGET`, `activation TARGET` | Require their actual prerequisites, approved signer and existing protected authority; signature/extraction/isolated activation are independent gates, not authorization to edit a real host. |

`stable` and `coverage` compile their selected targets once with `--no-run`, discover each actual
compiler-emitted test executable, list its names without running, then execute every harness once
under the original aggregate wall/idle limits and two-worker ceiling. Exact listed/passed names,
zero-failure/unfiltered summaries, source/binary/output hashes and owned cleanup are retained in
`test-execution.json`; compilation alone is not execution or semantic parity. Stable uses no
nightly-only JSON test flags. Compiler inventory must match every test-enabled target of the selected
Cargo packages. Harnesses retain their package working directory and explicit contained profile/target
overrides; unrelated compiler-command environment is not copied. Built, release and extracted SDK
contexts remain separate checks. Release and extracted SDK phases retain their own exact-name reports
and tested-binary hashes in `sdk-execution-<phase>.json`, using the same parser and owned runner.
Each report requires the exact manual repository/branch/source, numeric run/attempt, job and actual
native target identity. Before and after each phase, the same oracle-provenance bundle verifies all
tracked runtime, tooling, test/helper, schema and golden-input dependencies. Reports retain that
bundle's SHA-256, so a transitive SDK helper change cannot reuse old provenance. Exact report
validation also checks selected harnesses, successful names, source hashes and cleanup; it does not
authenticate a GitHub artifact download or promote semantic parity or final qualification.
`case-execution` reuses the authenticated ledger's exact proposal path/digest, binds all 470 original
case identities to successful current names and preserves original platform skips explicitly. Native
SDK mappings require built, release and extracted-intermediate reports; coverage requires its one
instrumented report. The per-context `current-case-execution.json` retains pending assertion review
and never rewrites historical evidence. The SDK migration manifest separately traces 82 historical
assertion records through 26 unique obligations to exact Rust ranges and excerpt hashes. The ledger
checks completeness, registered test mappings, module boundaries and current bytes, and binds the
manifest plus SDK helpers into its proposal. These are source-navigation bindings, not semantic
approval; formatting requires reviewed range rebinding. The SDK provider retains its task owner during
cleanup waits so cancellation cannot detach that owner; its event-driven cancellation contract is
source-authored, not yet executed. `execution-matrix` runs after all seven contexts succeed.
It sequentially reads same-repository GitHub run/attempt, artifact metadata and original job logs;
compares raw archive sizes/digests before bounded in-memory member decoding; verifies each target's
original golden bytes and permitted config-path relocation; and reconstructs every case binding
from its exact phase reports. The original job log must contain that case report's exact digest.
Every context must have the same repository, source, branch, run and attempt, with its exact supported
target, platform, phase and job. Self-consistent altered fixture hashes cannot redefine the independently
retained golden bytes. The resulting `execution-matrix.json` records named execution only, not semantic
parity, signing or release readiness. This collector and its rejection contracts still require hosted qualification.
`acceptance-prepare` compiles the actual SDK acceptance harnesses without running them, for a later
extracted artifact check. Target arguments use the exact six triples in the workflow. No command
silently installs an interpreter, changes a host entry, submits a generation or retries an unknown
paid outcome. Source coverage and command presence are not execution results.

Cargo owns the shared lockfile. The candidate lock still needs hosted preparation; removing
`--locked` from qualification or hand-writing dependency resolution is not a substitute. The authored
bootstrap downloads the exact host's published 0.2.0 archive, checks its source-pinned SHA-256, and
selects only its qualified `numera-dev` binary. That native owner supervises contained toolchain
installation and the first locked task build with wall/idle limits; no second shell supervisor or
interpreter is introduced. The download has native connection, wall, low-speed and size bounds.
Missing hosted Rustup/Cargo proxies fail rather than silently installing another bootstrap. This
new workflow still needs real hosted qualification. Its manual `operation=source-preparation` route
is separate: only the authenticated owner/toolchain setup, real `cargo generate-lockfile` and
`cargo fmt --all` run, without compiling the unresolved task crate, running suites or entering the
signing environment. It returns a source/ref/run/attempt-bound patch, receipt and checksums; review
the actual lock/format changes and rebind affected assertion ranges before committing them. Preparation
is not qualification; `operation=qualify` retains `--locked` and every native/coverage/artifact gate.
Do not use the candidate to replace the working installation. See [source validation](testing/SOURCE-VALIDATION.md)
for historical/current distinctions and [test results](TEST-RESULTS.md) for observed evidence.

## Bounded commands

```bash
numera-dev bounded 5000 500 --exec PROGRAM ARGUMENT
```

Run from the owning working directory. Wall and idle ceilings are positive decimal milliseconds,
at most 3600000. No shell expression is evaluated; stdin is closed. Output is buffered, at most
2 MiB combined, and returned after cleanup. Child output is not copied into diagnostic logs.
Meaningful child output resets idle time; watchdog activity does not.

Normal child exit codes propagate. Timeout, cancellation or excess output returns 124; an unsafe
cleanup or operation error returns 1. One two-second cleanup ceiling applies to the owned tree.
POSIX supervision uses a newly created process group anchored by an unreaped direct child; it is
not containment for a hostile child deliberately escaping its group. Windows starts the child
suspended, assigns it to a private non-breakaway Job Object, then resumes its owned snapshot threads.
Thread enumeration does not identify the primary by position: every opened handle's process identity
is rechecked, already-running auxiliary threads are harmless, and an unreleased suspension fails
closed. Unrelated processes are never selected by name or a recycled numeric PID.

A sanitized process ledger records start/bounds, termination reason, exit and cleanup evidence.
Windows start and termination logs additionally report measured startup milliseconds, resumed/running
snapshot-thread counts, query-only CPU milliseconds and child liveness. Unavailable observations are
null. Termination reports elapsed/idle time and captured byte counts, never child output. These are
one-time boundary observations, not watchdog heartbeats or a claim that a particular thread is primary.
At `af83bd0`, readiness regressions passed in all seven native contexts, but Windows x64's DPAPI script
failed before entry in both the original run and its single confirmation (8.14s / 8.42s). Its cause
remains unresolved. At `26d4224`, the child was alive after 8009ms with zero measured CPU and no
captured bytes, despite one recorded thread resume. A timeout-only thread snapshot now reports numeric
suspension counts or an explicit query error; it captures no memory, addresses or context. This new
observation passed its real current-process and layout tests on both Windows targets at `49dde41`;
the DPAPI case also passed. Historical startup failures remain unexplained. The Windows x64 case recurred at `6c0b980` with
8.19s/exit124, no worker output and no suspended threads in the timeout snapshot; its cause remains
unknown. Diagnostics never retry, increase deadlines or weaken ACLs.
Managed scratch and ledger stay under the owning root. Existing UTF-8 UTC structured logging uses
`logs/numera-gen-mcp_<date>_<time>_UTC-<pid>-<uuid>.log`, stderr, redaction and 14-day retention.
Historical `numera-image-gen-mcp_` logs retain their names and remain recognized by retention.
Credentials, private arguments, prompts and environment values are not logged. Failure to create a
log is reported on stderr. Share only reviewed, redacted logs for support.

## Prepared-artifact maintenance

```bash
numera-dev setup Install --root /absolute/installation --artifact /absolute/prepared --dry-run
```

The installation root must already exist. All paths are explicit and absolute; no host is discovered
or selected implicitly. Supported actions:

| Action   | Required additional options                        | Effect without dry-run                                                                 |
| -------- | -------------------------------------------------- | -------------------------------------------------------------------------------------- |
| Install  | `--artifact DIRECTORY`                             | Authenticate and stage an extracted prepared distribution.                             |
| Update   | `--artifact DIRECTORY`                             | Authenticate old/new distributions and replace distribution-owned files with rollback. |
| Diagnose | `--config FILE`                                    | Local configuration, storage and declared credential readiness; never generation.      |
| Register | `--client CLIENT --host-config FILE --config FILE` | Merge only the selected Numera entry, preserving unrelated settings.                   |
| Remove   | Same as Register                                   | Remove only that entry, never installation or application data.                        |

Clients are `ClaudeDesktop`, `ClaudeProject` or `Codex`. Register points to the selected root's
`bin/numera-gen-mcp[.exe]`, with `args: ["bridge"]`, not a neighboring unpacked helper.
Existing registration backup, lock, idempotence and concurrent-change checks are reused.

Every action supports `--dry-run`. It validates inputs but performs no build, runtime installation,
smoke/provider operation, state/output creation, host update, backup or installation lock; redacted
logs are the only allowed write. An empty prepared directory can be inspected in dry-run, but is
not accepted as an authenticated installation.

Actual Install/Update require the pinned approved signer, current identity validity, exact target,
version/source metadata, regular bounded members, hashes and approved logo. Windows/macOS activation
also independently verifies native executable signatures, not just the detached manifest. Synthetic text
fixtures reject on those systems; actual signed Install/Update/Register/Remove checks passed all six
targets after protected signing and extraction, including both Windows rollback controls. User configuration and
non-distribution files are not overwritten. A locked replacement fails and rolls back previous
replacements; incomplete rollback retains its owned staging for recovery instead of deleting backups.
Never delete another installer's lock or retained recovery data to force an update.

No action compiles locally, installs Node/Rust/Python, changes OS trust, creates a service or submits
a media generation request. [Signature and trust limits](SIGNING.md) are separate from a successful dry-run.
The existing working runtime must remain until the owner changes their host entry and verifies the
new native connection with real `health_check` and `list_connections` calls.
