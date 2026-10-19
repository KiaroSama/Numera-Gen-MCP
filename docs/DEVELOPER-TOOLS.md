# Native developer tools

`numera-dev` is a separate optional developer executable. The MCP application still has exactly ten
tools and runs directly as `numera-image-gen-mcp bridge`; it does not launch this helper.
The native tooling candidate failed its first hosted compilation at `5781c32`; the type/Apple-binding
and formatting corrections still require their own hosted build/test verification.
Do not replace a working installation or host entry with an unverified candidate.

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
suspended, assigns it to a private non-breakaway Job Object, then resumes its primary thread.
Unrelated processes are never selected by name or a recycled numeric PID.

A sanitized process ledger records start/bounds, termination reason, exit and cleanup evidence.
Managed scratch and ledger stay under the owning root. Existing UTF-8 UTC structured logging uses
`logs/numera-image-gen-mcp_<date>_<time>_UTC-<pid>-<uuid>.log`, stderr, redaction and 14-day retention.
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
`bin/numera-image-gen-mcp[.exe]`, with `args: ["bridge"]`, not a neighboring unpacked helper.
Existing registration backup, lock, idempotence and concurrent-change checks are reused.

Every action supports `--dry-run`. It validates inputs but performs no build, runtime installation,
smoke/provider operation, state/output creation, host update, backup or installation lock; redacted
logs are the only allowed write. An empty prepared directory can be inspected in dry-run, but is
not accepted as an authenticated installation.

Actual Install/Update require the pinned approved signer, current identity validity, exact target,
version/source metadata, regular bounded members, hashes and approved logo. User configuration and
non-distribution files are not overwritten. A locked replacement fails and rolls back previous
replacements; incomplete rollback retains its owned staging for recovery instead of deleting backups.
Never delete another installer's lock or retained recovery data to force an update.

No action compiles locally, installs Node/Rust/Python, changes OS trust, creates a service or submits
an image request. [Signature and trust limits](SIGNING.md) are separate from a successful dry-run.
The existing working runtime must remain until the owner changes their host entry and verifies the
new native connection with real `health_check` and `list_connections` calls.
