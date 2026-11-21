# Numera Gen MCP

<div align="center">

<img src="docs/assets/logo.png" alt="Numera Gen MCP logo" width="240" />

[![CI runs on GitHub Actions across four native jobs plus coverage](https://img.shields.io/badge/CI-GitHub%20Actions-2563eb)](.github/workflows/ci.yml)
[![License GPL-3.0-or-later, the first-party grant](https://img.shields.io/badge/license-GPL--3.0--or--later-16a34a)](LICENSE)
[![Latest release v1.0.0](https://img.shields.io/badge/release-v1.0.0-ea580c)](CHANGELOG.md)
[![Measured CI coverage: 85.6% regions and 80.0% branches](https://img.shields.io/badge/coverage-regions%2085.6%25%20%7C%20branches%2080.0%25-059669)](docs/testing)
[![Rust 1.100.0 pinned toolchain](https://img.shields.io/badge/rust-1.100.0-8b5cf6)](rust-toolchain.toml)
[![Runs on Windows and Linux, each built for x64 and ARM64](https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%C2%B7%20x64%20%7C%20ARM64-0891b2)](.github/workflows/ci.yml)
[![Distributed as a native binary](https://img.shields.io/badge/distribution-native%20binary-dc2626)](#installation-and-setup)
[![Documentation in Markdown](https://img.shields.io/badge/docs-Markdown-4f46e5)](docs/CONFIGURATION.md)
[![Tracked repository tree is 14.2 MB](https://img.shields.io/badge/repo%20size-14.2%20MB-ca8a04)](.)
[![Built with the rmcp 3.10.0 MCP SDK](https://img.shields.io/badge/built%20with-rmcp%203.10.0-be185d)](Cargo.toml)
[![Job receipts stored in SQLite](https://img.shields.io/badge/receipts-SQLite-0f766e)](native/jobs/store.rs)
[![MCP transports: stdio and local HTTP](https://img.shields.io/badge/MCP-stdio%20%7C%20HTTP-7651a8)](docs/ARCHITECTURE.md)
[![Media formats: PNG, JPEG, WebP, animated GIF, MP4 video, WAV audio and GLB 3D](https://img.shields.io/badge/media%20formats-PNG%20JPEG%20WebP%20GIF%20%C2%B7%20MP4%20%C2%B7%20WAV%20%C2%B7%20GLB-65a30d)](#what-numera-can-generate)
[![Image and video modes: text-to-image, image-to-image with editing and masks, text-to-video, image-to-video, video-to-video and video-to-audio](https://img.shields.io/badge/image%20%26%20video%20modes-text%E2%86%92image%20%C2%B7%20image%E2%86%92image%20%28edit%2C%20mask%29%20%C2%B7%20text%E2%86%92video%20%C2%B7%20image%E2%86%92video%20%C2%B7%20video%E2%86%92video%20%C2%B7%20video%E2%86%92audio-3068b7)](#video-audio-and-3d)
[![Audio and 3D modes: text-to-audio, audio-to-audio, video-to-audio, text-to-3D, image-to-3D and 3D-to-3D](https://img.shields.io/badge/audio%20%26%203D%20modes-text%E2%86%92audio%20%C2%B7%20audio%E2%86%92audio%20%C2%B7%20video%E2%86%92audio%20%C2%B7%20text%E2%86%923D%20%C2%B7%20image%E2%86%923D%20%C2%B7%203D%E2%86%923D-475569)](#video-audio-and-3d)
[![Support donations](https://img.shields.io/badge/Support-donations-d04a9a)](#donate)

</div>

A local provider-independent MCP server for explicitly selected image, video, audio and 3D work.
Keep your host chat model unchanged. Named connections separate native image protocols, gateway
forwarding, configured GPU ComfyUI workflows and exact fal queue endpoint contracts. No paid POST
retries, hidden uploads, prompt enhancement or automatic provider/model fallback.

**Version 1.0.0** ships 13 MCP tools (adding `list_jobs`), animated GIF generation and editing
(`output_format: "gif"`, `frame_count`, `frame_duration_ms`), 11 typed media modes (adding
`video-to-audio` via `fal-ai/mmaudio-v2`), muxed MP4 AAC/Opus audio validation,
`WAVE_FORMAT_EXTENSIBLE` WAV, safe Khronos GLB 2.0 extensions, opt-in custom `falContract`
endpoints, OmniRoute `/v1/fal` support, MCP `notifications/progress` updates, non-blocking
`wait: false` background creation (`openai-responses` and `gemini-interactions`), DNS/redirect SSRF
hardening and SQLite WAL/lease concurrency hardening. One `.env` can still configure both a main
endpoint and a `fal` section (`FAL_API_ENDPOINT`, `FAL_API_KEY`, `FAL_MODEL_n_*`) routed by model ID.
Supported targets are Windows and Linux, each on x64 and ARM64 (macOS was removed in 0.4.0).
First-party code is **GPL-3.0-or-later**; source builds pin **Rust 1.100.0**, while released binaries
need no compiler or language runtime.

Install only the signed [1.0.0 release](https://github.com/KiaroSama/Numera-Gen-MCP/releases/tag/v1.0.0)
with its [verification record](docs/releases/1.0.0.md). Source or an unsigned CI intermediate is not
an installable final release.

The runtime, developer utility, CI driver and maintained tests are Rust. All 51 replaced
Python/JavaScript/PowerShell source files have been removed; historical oracle archives remain
immutable test data. One explicit manual qualification runs tests, Clippy, formatting, release
builds and extracted-binary checks on Windows and Linux, each on x64 and ARM64, plus
production coverage and the authenticated execution collector. The commit's check list links to
those actual jobs; source preparation is separate and never a test pass. Main-only signing promotes
the same qualified binaries without rerunning the heavy suites. The workflow badge is an aggregate,
not proof that preparation, tests and signatures all ran; [test evidence](docs/TEST-RESULTS.md) records
the exact source/run/attempt. [Rust migration](docs/RUST-MIGRATION.md) records scope and dispositions.

The released 1.0.0 source at `f3e3165` passed all eleven active qualification and protected signing
jobs in [run 44942765654](https://github.com/KiaroSama/Numera-Gen-MCP/actions/runs/44942765654),
attempt 1, with **85.5671% region** (20,922 / 24,451) and **80.0267% branch** (4,800 / 5,998)
coverage against unchanged 85% / 80% floors. The [1.0.0 verification record](docs/releases/1.0.0.md)
identifies the exact source, run, targets, checksums and live E2E evidence. Historical
[0.5.0](docs/releases/0.5.0.md), [0.3.0](docs/releases/0.3.0.md) and [0.2.0](docs/releases/0.2.0.md)
keep their own records. Later edits are not covered by these results.

[Historical 0.2.0](docs/releases/0.2.0.md) retains its original signed bytes and image-only template.
Preserve a working installation until the replacement is signed and the actual host connection is
verified. [Signing and trust](docs/SIGNING.md) distinguishes the pinned self-signed publisher from
public OS trust, Apple Developer ID and notarization. CI/SDK readiness does not prove actual GUI
reconnection, live provider entitlement or GPU generation. Independently generated oracle bytes
remain immutable test inputs, not expectations regenerated by the implementation under test.
See [migration acceptance](docs/testing/MIGRATION-MATRIX.md) and [test evidence](docs/TEST-RESULTS.md).

The native target also adds an optional direct HTTP MCP endpoint at `http://127.0.0.1:1969/mcp`,
a configurable port and a cross-platform native menu. It reuses Numera's own tools, not a remote MCP
proxy. Local authentication is separate from provider keys; `bridge` remains launcher-free.
[HTTP and menu setup](docs/HTTP.md) has provider-free hosted SDK/CLI verification; actual owner
registration and interactive GUI use remain unverified.

## Installation and setup

**Use a signed released binary, not a source build or unsigned CI intermediate.** The five steps
below target **1.0.0** and its actual `numera-gen-mcp` executable. Require the published release's
exact-source qualification, signatures and matching checksums before installation; the
[verification record](docs/releases/1.0.0.md) holds the full run and digests. The immutable image-only **0.2.0** release
retains `numera-image-gen-mcp` and its own template; never rename those signed bytes or copy media
settings into that old version. [Historical installation and trust
evidence](docs/releases/0.2.0.md) remains available.

You need a local MCP-capable client, a writable installation folder, and an endpoint/key/model
that actually supports your intended image operation. Hosted generation also needs network access;
ComfyUI needs its own configured installation. Nothing in setup authorizes a paid request.

### 1. Download the ready native build

Download [native 1.0.0](https://github.com/KiaroSama/Numera-Gen-MCP/releases/tag/v1.0.0) from this
repository's Releases page and verify it against the published `.sha256` sidecar; an unavailable or
unverified archive is not permission to use an unsigned artifact.
Normal usage needs **no Node.js, npm, Rust toolchain, Python, compiler or local build**.
Your MCP client runs the executable directly. ComfyUI remains an optional separately installed backend,
not a bundled runtime.

Choose the archive for your actual OS/CPU:

| OS      | CPU                       | Archive suffix                                                         |
| ------- | ------------------------- | ---------------------------------------------------------------------- |
| Windows | Intel/AMD 64-bit | `numera-gen-mcp-1.0.0-x86_64-pc-windows-msvc.zip` |
| Windows | ARM64 | `numera-gen-mcp-1.0.0-aarch64-pc-windows-msvc.zip` |
| Linux GNU | Intel/AMD 64-bit | `numera-gen-mcp-1.0.0-x86_64-unknown-linux-gnu.tar.gz` |
| Linux GNU | ARM64 | `numera-gen-mcp-1.0.0-aarch64-unknown-linux-gnu.tar.gz` |

Published compatibility and minimum OS versions must match that Release's actual native CI evidence.
A Linux binary is not proof that Claude Desktop itself supports Linux.

### 2. Verify and extract

Download the archive **and its `.sha256` sidecar** from the same Release. Match the SHA-256 before
extracting. The archive contains `bin/numera-gen-mcp` (Windows: `.exe`), license/notices,
configuration templates, schemas and examples. Extract into a writable installation folder you own;
do not overwrite an existing private configuration or outputs.

Windows PowerShell, replacing the downloaded filename:

```powershell
$archive = './numera-gen-mcp-1.0.0-x86_64-pc-windows-msvc.zip'
$expected = ((Get-Content -LiteralPath "$archive.sha256" -Raw -Encoding utf8).Trim() -split '\s+')[0]
if ($expected -notmatch '^[a-fA-F0-9]{64}$' -or (Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash -ine $expected) { throw 'Release checksum mismatch; do not extract or run.' }
```

Use the sidecar for that exact version and target. Check the Release's source and approved signer
pins too; a matching checksum does not authenticate an untrusted replacement sidecar.

Linux, from the download folder:

```bash
sha256sum -c numera-gen-mcp-1.0.0-x86_64-unknown-linux-gnu.tar.gz.sha256
```

Extract into a **new** directory using your OS archive extractor. Archives contain one top-level
`numera-gen-mcp-1.0.0-<target>/` folder. That folder is the **installation root**: it contains
`bin/`, `.env.example`, `docs/` and `manifest.json`. Open a terminal there for the relative commands
below. You may give that extracted folder a stable installation name before registering it, for
example `C:/tools/numera-gen-mcp` or `$HOME/tools/numera-gen-mcp`; keep its contents together.

On Linux, `tar -xzf ARCHIVE.tar.gz` preserves executable permissions. Confirm the binary before
adding credentials (Windows PowerShell, from the installation root):

```powershell
& './bin/numera-gen-mcp.exe' --version
```

Linux:

```bash
./bin/numera-gen-mcp --version
```

Expect version `1.0.0`, publisher `KiaroSama` and the exact source SHA recorded in that Release.
Do not accept a different build source merely because its version string matches.
Check [signature verification and self-signed trust limits](docs/SIGNING.md) before running a download;
a checksum alone is not a public OS-trust certificate. Do not blindly bypass security warnings or
install a new trust root. Managed `.runtime/temp` and `.runtime/cache` stay inside the installation
root; it must remain writable. No launcher, admin install or global cache is required.

### 3. Create your private configuration

Copy the **extracted release's own** `.env.example` to `.env` in the installation root, not `bin/`
and not your client's working directory. Edit it in a UTF-8 text editor. Existing installation?
Back up and retain its selected private config and data instead of replacing them with a template.

Windows PowerShell:

```powershell
[IO.File]::Copy((Join-Path $PWD '.env.example'), (Join-Path $PWD '.env'), $false)
```

Linux:

```bash
(set -o noclobber; umask 077; cat .env.example > .env)
```

Edit `.env`: set `API_ENDPOINT`, `API_KEY` and `MODEL_1_ID` / `MODEL_1_NAME`. The 0.3.0-and-later template has
three slots with `ENABLED`, image defaults and optional paired `OUTPUT_TYPES` / `MODES` declarations.
Leave both declaration fields empty for existing image configurations.
Extra slots are disabled; fill their exact
IDs/names before setting ENABLED=true. At least one model must be enabled; the first enabled slot
is the default. Names are display labels, never routing aliases. Empty output fields preserve provider
defaults; explicit tool arguments override them. Use only native values your selected model supports.

```dotenv
API_ENDPOINT=https://gateway.example/v1
API_KEY=replace-with-your-private-key
PROFILE=openai-images
MODEL_1_ID=provider/exact-image-model
MODEL_1_NAME="My image model"
MODEL_1_ENABLED=true
MODEL_1_OUTPUT_FORMAT=
MODEL_1_SIZE=
MODEL_1_QUALITY=
```

The endpoint is any valid HTTP(S) API prefix: hosted, localhost or reverse-proxy, not a fixed port.
Do not include /responses or /images/generations. Generic openai-images is the default; explicitly
choose PROFILE=omniroute or PROFILE=9router for gateway-specific contracts. An endpoint/key pair
alone cannot turn a chat-only API into an image API.

For Responses use [simple-responses.env.example](examples/simple-responses.env.example):
PROFILE=openai-responses plus ORCHESTRATION_MODEL, separate from the selected image model.
Native Gemini/Gemini Interactions/OpenRouter Images profiles are also available; see
[configuration](docs/CONFIGURATION.md). Editing is enabled only by its verified native contract,
not guessed from an endpoint/model ID.

Outputs/state/logs/inputs sit beside the selected config. This plaintext-key file is ignored and
excluded from packages: never share, sync or commit it. On Linux restrict access:

```bash
chmod 600 .env
```

**Save and call list_models again: valid model/endpoint settings reload without restarting.**
Invalid or partially saved files keep the last valid configuration and emit a redacted warning.
Active jobs retain their original connection; reload never submits or retries an image request.
Storage/logging/input-root/global-policy changes and the bounded 64-snapshot limit require restart.
Legacy compact and advanced JSON remain supported for existing installations.

#### What each .env field means

Values may be quoted as in [.env.example](.env.example). An empty value (`""`) means unset, not
`auto` or zero. Boolean fields accept exactly `"true"` or `"false"`; `yes`, `1` and `0` are invalid.
Keep key names unchanged: unknown keys and duplicate assignments reject the saved configuration.

**Connection and API protocol**

| Field                 | Purpose                                                                                                                                                                                                                         | Required / default                                                                  |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| `API_ENDPOINT`        | Your image API's HTTP(S) base prefix, such as `https://gateway.example/v1`. Numera appends the operation path. Do not include `/images/generations`, `/images/edits` or `/responses`.                                           | Required. No fixed host or port.                                                    |
| `API_KEY`             | Private credential for that endpoint. Gemini sends `x-goog-api-key`; other released compact profiles send bearer authentication. Never paste a real key into host config, prompts or public files.                               | Required, nonblank, without newline/NUL.                                            |
| `PROFILE`             | Selects the actual API contract. Accepted: `openai-images`, `omniroute`, `9router`, `openai-responses`, `gemini`, `gemini-interactions`, `openrouter-images`. Gateway profiles use Images with their specific forwarding rules. | Empty/omitted: `openai-images`. Choose the protocol your endpoint actually exposes. |
| `ORCHESTRATION_MODEL` | Exact conversation-model ID that drives a Responses request. It is separate from the image model selected through `MODEL_n_ID`.                                                                                                 | Required for `openai-responses`; leave empty for every other profile.               |

**Model slots: replace `n` with 1, 2, 3, ... up to 100**

The same image-default fields and optional paired media declarations apply to every slot. Slots are ordered numerically; the first enabled model is the
default. A nonempty slot needs both ID and NAME, including disabled placeholders. At least one model
must be enabled, and IDs must be unique. A configured model is not proof of account entitlement.

| Field                   | Purpose                                                                                                                                             | Required / default                                                  |
| ----------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| `MODEL_n_ID`            | Exact provider model ID sent to the API, including any required provider prefix.                                                                    | Required for a nonempty slot; up to 300 characters.                  |
| `MODEL_n_NAME`          | Friendly name the agent displays through `list_models`. It does not change routing.                                                                 | Required for a nonempty slot; up to 100 characters.                  |
| `MODEL_n_ENABLED`       | Enables the slot. `false` hides it from configured listing and denies its ID for submission.                                                        | Empty/omitted: `true`. Template slots2 and3 explicitly use `false`. |
| `MODEL_n_OUTPUT_FORMAT` | Default output format: `png`, `jpeg` or `webp`, only if the selected route supports it.                                                             | Empty: provider default, nothing sent.                              |
| `MODEL_n_SIZE`          | Default native pixel dimensions, for example `1024x1024`, if accepted by that API/model. Numera does not resize the returned original to match.     | Empty: provider default; nonblank values up to 100 characters.       |
| `MODEL_n_QUALITY`       | Default native quality value, for example `auto`, `low`, `medium` or `high` on models supporting those values. It is not a universal quality scale. | Empty: provider default; nonblank values up to 100 characters.       |

The optional `MODEL_n_OUTPUT_TYPES` / `MODEL_n_MODES` pair requires **0.3.0 or later** and is documented in
[Video, audio and 3D](#video-audio-and-3d). Both fields must be nonempty together or empty together.
Adding them to an old 0.2.0 configuration rejects it; retain that release's own template until upgrading.

Explicit image-tool arguments override these model defaults. Native Gemini uses `aspect_ratio` and
`image_size` tool arguments instead of Images-style SIZE/QUALITY; generateContent does not accept a
requested output format. Unsupported settings fail rather than silently disappearing.

**Optional reference-editing contract**

These fields describe a verified Images input contract, not a switch that grants the model editing
capability. Leave all five empty for generation-only use or when your route has not been verified.
Native protocols may already carry references independently; these fields cannot add native mask
support to an adapter that does not implement it. Any nonempty edit setting requires `EDIT_MODE`.

| Field                 | Purpose                                                                                                                                                                                                                            | Required / default                                      |
| --------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------- |
| `EDIT_MODE`           | `multipart`: upload reference bytes to `/images/edits`; `json`: inline references in JSON to `/images/edits`; `generation`: inline references through `/images/generations`. Explicit gateway contracts may override this mapping. | Empty: no explicit Images edit contract.                |
| `EDIT_ENCODING`       | Reference field shape expected by the route: `image`, `image[]`, `images`, `image_urls` or `input_references`. For multipart, use `image` or `image[]`; JSON/generation must match the provider's documented field.                | Required when EDIT_MODE is set.                         |
| `EDIT_MAX_REFERENCES` | Maximum reference count declared for the edit contract; other route/model limits can be stricter.                                                                                                                                  | Integer1 to 32; empty:1.                                   |
| `EDIT_MASKS`          | Declares verified mask forwarding. `true` alone cannot bypass an adapter/gateway restriction.                                                                                                                                      | Empty: `false`.                                         |
| `EDIT_MASK_POLARITY`  | Which mask pixels are editable: `transparent-edit` = transparent pixels; `white-edit` = white pixels; `black-edit` = black pixels. Must match the provider.                                                                        | Needed for mask/rectangle editing; no inferred default. |

For a verified standard Images edit/mask route, a typical setup is:

```dotenv
EDIT_MODE="multipart"
EDIT_ENCODING="image"
EDIT_MAX_REFERENCES="1"
EDIT_MASKS="true"
EDIT_MASK_POLARITY="transparent-edit"
```

Do not enable this example blindly on a gateway. `edit_region` and `target_language` belong to
`edit_image` tool arguments, not .env fields. [Editing examples](docs/IMAGE-EDITING.md) and
[advanced configuration](docs/CONFIGURATION.md) explain route limits and additional JSON settings.
Save .env and call `list_models` again to apply a valid change; no image is submitted just by saving.

#### Optional second provider: fal

The same `.env` can hold a separate fal section beside the main provider. It becomes a second
connection named `fal`, with its own endpoint, key and model list; the main section stays connection
`default`. Fill both fal fields to enable it, or leave them empty to disable it:

```dotenv
FAL_API_ENDPOINT=https://queue.fal.run
FAL_API_KEY=replace-with-your-private-fal-key
FAL_MODEL_1_ID=fal-ai/flux/dev
FAL_MODEL_1_NAME="FLUX dev (text-to-image)"
FAL_MODEL_1_ENABLED=true
FAL_MODEL_1_OUTPUT_TYPES=image
FAL_MODEL_1_MODES=text-to-image
```

`FAL_MODEL_n_*` accepts the same fields as `MODEL_n_*`. `FAL_API_ENDPOINT` selects direct queue mode
(`Key` authentication) for `https://queue.fal.run` or OmniRoute's `/v1/fal` gateway (`Bearer`
authentication) for an HTTP(S) `/v1` or `/api/v1` prefix, so the fal section has no `PROFILE`; `EDIT_*`
and `ORCHESTRATION_MODEL` apply to the main section only. Either section may be left empty, but not both; with only fal
configured, `fal` is the default connection. A tool call without `connection` goes to the section that
lists the requested model ID. An ID listed in both sections, or in neither, is refused before anything
is sent, so pass `connection` explicitly in that case.

### 4. Register with your MCP client

Registration is one-time. Preserve unrelated settings and back up the host file before merging only
Numera's entry. The exact current client entry is `numera-gen-mcp`; use that name consistently. If you
already have `numera-image-gen`, update that selected entry instead of adding a second server against
the same state. The registration command (0.3.0 and later) manages exactly `numera-gen-mcp`; it never removes
older entries implicitly. Use either manual configuration or explicit entry-scoped registration, not both. After registration **no launcher needs to be opened**: the host runs the executable
with `args: ["bridge"]`, which starts MCP over stdin/stdout. No background daemon or listening port
must be started first.

#### Claude Desktop

Merge this into the actual `claude_desktop_config.json`; the paths are examples, not your machine's
installation. Keep every other `mcpServers` member unchanged:

```json
{
  "mcpServers": {
    "numera-gen-mcp": {
      "command": "C:/tools/numera-gen-mcp/bin/numera-gen-mcp.exe",
      "args": ["bridge"]
    }
  }
}
```

The executable finds `.env` first, then `config.local.json`, in its installation root (parent of `bin/`),
not Claude's working directory. Existing user-config fallback and an explicit absolute `NUMERA_CONFIG`
remain supported. If private configuration is elsewhere, add only this entry's `env`:

```json
{
  "NUMERA_CONFIG": "C:/private/numera/.env"
}
```

Never put the API key in host settings or arguments; `args` stays `["bridge"]`.
Use Desktop's Edit Config to locate its actual settings, fully quit/reopen the app and call
`health_check` plus `list_connections` to confirm the selected configuration.
[Claude clients](docs/CLAUDE.md) has the registration and reconnect details.

#### Claude Code

Linux, from the extracted installation root:

```bash
claude mcp add --transport stdio --scope user numera-gen-mcp -- "$PWD/bin/numera-gen-mcp" bridge
```

Windows PowerShell, with your actual executable:

```powershell
claude mcp add --transport stdio --scope user numera-gen-mcp -- 'C:/tools/numera-gen-mcp/bin/numera-gen-mcp.exe' bridge
```

For project scope, use the same command with `--scope project` instead of `--scope user`; review
and preserve the existing `.mcp.json`. Current [project JSON examples](examples/claude-project.json)
use the current `numera-gen-mcp` binary name (0.3.0 and later); keep the immutable legacy name only for historical 0.2.0. CLI user
scope is distinct from Claude Desktop chat's config. Restart/reconnect the selected client after
registration; endpoint/model saves then reload through `list_models`, without another submission.

#### Codex and other local MCP clients

Merge only this table into the actual `~/.codex/config.toml` (or trusted project's `.codex/config.toml`),
backing it up first and replacing example paths with your installation. A 360-second tool timeout
allows the server's ordinary long-operation timeout to report its receipt instead of the client's
60-second default ending first. Do not add duplicate tables.

```toml
[mcp_servers.numera-gen-mcp]
command = "C:/tools/numera-gen-mcp/bin/numera-gen-mcp.exe"
args = ["bridge"]
startup_timeout_sec = 10
tool_timeout_sec = 360
env = { NUMERA_CONFIG = "C:/tools/numera-gen-mcp/.env" }
```

On Linux use actual absolute POSIX paths. The alternative CLI command below adds the stdio
entry; set `tool_timeout_sec = 360` in its resulting table afterward, then restart/reconnect Codex:

```bash
codex mcp add numera-gen-mcp -- "$PWD/bin/numera-gen-mcp" bridge
```

Cursor/Windsurf use their local `mcpServers` configuration. Kiro uses `mcpServers` with its `disabled`
flag. Hermes uses `mcp_servers`. VS Code-native `.vscode/mcp.json` uses `servers` and `type: "stdio"`.
For those clients set `command` to the native binary and `args` to `["bridge"]`; optionally add the
absolute private `NUMERA_CONFIG` in that member's `env`. Do not translate top-level keys between
clients blindly. OpenCode uses `mcp` with `type: "local"` and a command array containing the binary
and `"bridge"`. Existing host trust/approval and installation support remain the host's responsibility.

Optional native entry-scoped registration (never automatic at startup):

```bash
./bin/numera-gen-mcp register --client ClaudeDesktop --host-config /absolute/claude_desktop_config.json --config /absolute/numera/.env --dry-run
```

Review the dry run first. Removing `--dry-run` deliberately updates only Numera with an exact-byte
backup and concurrent-change protection. `--remove` removes only that member, never user images or
application data. Ambiguous TOML, links, active registration locks and invalid host JSON are rejected.
These instructions are not registration performed by this repository task.

#### Current-name replacement (0.3.0 and later)

The new build is `numera-gen-mcp.exe` on Windows and `numera-gen-mcp` on Linux; its exact
host entry is `numera-gen-mcp`. **Do not switch until that build is signed, qualified and installed.**
The commands above use the real current executable (0.3.0 and later). An old 0.2.0 binary cannot be upgraded by renaming it.
After the qualified new installation is verified, replace only the selected Numera entry:

```json
{
  "mcpServers": {
    "numera-gen-mcp": {
      "command": "C:/tools/numera-gen-mcp/bin/numera-gen-mcp.exe",
      "args": ["bridge"],
      "env": {
        "NUMERA_CONFIG": "C:/tools/numera-gen-mcp/.env",
        "TEMP": "C:/tools/numera-gen-mcp/.runtime/temp",
        "TMP": "C:/tools/numera-gen-mcp/.runtime/temp",
        "TMPDIR": "C:/tools/numera-gen-mcp/.runtime/temp"
      }
    }
  }
}
```

Replace every example root with the same actual project directory. The binary's installation root
(parent of `bin/`) owns `.runtime/temp` and `.runtime/cache`, regardless of the config path or host
working directory; there is no extra cache environment key. [Desktop](examples/claude-desktop.json),
[project JSON](examples/claude-project.json) and [Codex TOML](examples/codex.toml) use the same new name.
Back up and explicitly migrate a selected old `numera-gen`/`numera-image-gen` entry rather than running
two processes against the same state. Registration manages only the exact current entry; it does not
remove legacy entries automatically. Keep the working runtime and data until actual reconnection.

### 5. Verify the connection before approving an image

First check local configuration/storage/declared credentials without contacting a provider. Windows
PowerShell, replacing the absolute config path:

```powershell
& './bin/numera-gen-mcp.exe' readiness --config 'C:/tools/numera-gen-mcp/.env'
```

Linux:

```bash
./bin/numera-gen-mcp readiness --config "$PWD/.env"
```

Expect `local_ready: true` and exit 0 for a valid writable installation. Missing credentials or
connection blockers return exit 2; invalid arguments/configuration return exit 1. This is not a
successful generation or actual host reconnection.

After restarting the MCP client, ask it to call `health_check` with `probe=false`, `list_connections`
and `list_models` with `refresh=false`. Inspect `get_model_capabilities` for the exact selected model.
Expect thirteen tools (1.0.0 and later), the selected connection and your configured model name. Only if you
want to test the provider's read-only catalog, add `--probe` to the readiness command; it performs
provider GETs but never submits a generation. Readiness does not prove model entitlement or quality.
Logs go to stderr/files, never MCP stdout.

When you explicitly approve potentially billable work, ask: **Use Numera generate_image once to create
a blue mug beside an orange. count=1; request_id=my-first-image-001.** Reuse that ID for the same
logical request only. Never give an unknown outcome a new ID just to retry it.

Outputs keep the provider's original bytes and verified MIME/dimensions/hash. A bounded preview is
optional; a remote/cloud host cannot inherently read local paths or launch a local stdio executable.

### Update and troubleshoot

Download the next verified Release and checksum into this project's own download/work directory.
Preserve private config, originals, receipts/journals and migration snapshots. Quit/reconnect the MCP
host around replacing the executable; never replace data folders with a fresh empty installation.
Keep the old executable until the new host connection actually succeeds. Only then remove reviewed
obsolete project-local runtime/dependency/temp/cache paths. Never clear another project's or a
system/global cache.

| Problem              | Check                                                                                                    |
| -------------------- | -------------------------------------------------------------------------------------------------------- |
| Server not visible   | Actual absolute binary path, `args: ["bridge"]`, correct host file and trust/reconnect                   |
| Startup exits        | Private config validity, writable project-contained scratch and configured storage; inspect redacted log |
| No configured models | At least one enabled model with exact ID and readable name                                               |
| Catalog/probe fails  | Selected route, API prefix, credentials and account; this is not generation failure proof                |
| Unknown outcome      | Inspect the same receipt with `get_job`; do not submit a replacement automatically                       |

If a 0.2.0 installation reports unknown `OUTPUT_TYPES` / `MODES` or `PROFILE=fal`, restore its own
release template; these belong to 0.3.0 and later. A missing `numera-gen-mcp` filename is the same version
mix-up: use `numera-image-gen-mcp` for 0.2.0. A manually launched `bridge` waiting on stdin is normal;
use readiness for diagnosis and let the MCP host own bridge startup/shutdown.

## Protocols and tools

### OpenAI-compatible vs Anthropic-compatible API

Numera currently submits images through evidenced image APIs, including OpenAI-compatible Images
and Responses. Using Claude as the MCP host does not require an Anthropic image endpoint: Claude
calls Numera over MCP, and Numera calls your selected image backend independently.

The standard Anthropic POST /v1/messages contract accepts images for vision but does not define a
native generated-image output or OpenAI-style image_generation tool. A generic Anthropic-compatible
chat endpoint is therefore not sufficient for image generation. Numera does not claim such an adapter.

The examined OmniRoute3.8.51 Messages path delegates to chat; image-only models are rejected and its
Claude response translation does not preserve generated image content. Use its supported
/v1/images/generations route instead. A future Messages image-output adapter needs an explicit
gateway extension defining request options, authentication and final image bytes; no protocol
inference, silent parameter dropping or automatic fallback is permitted.

Sources: [Anthropic Messages](https://platform.claude.com/docs/en/api/messages/create),
[vision input](https://platform.claude.com/docs/en/build-with-claude/vision), and
[pinned OmniRoute response conversion](https://github.com/diegosouzapw/OmniRoute/blob/c1e30b7676975feb298b49eff6ff58923c04b89e/open-sse/handlers/responseTranslator.ts#L736).

Generic Images, OmniRoute, 9router, Gemini generateContent, Gemini Interactions, OpenAI Responses image
tools, dedicated OpenRouter Images, opt-in chat image output and explicitly bound ComfyUI API graphs.
[Compatibility/limitations](docs/COMPATIBILITY.md) distinguish protocol evidence from account/model
availability. Antigravity refs are blocked at the examined gateway versions that drop input images.

13 source-defined tools: health_check, list_connections, list_models, get_model_capabilities,
generate_image, edit_image, get_job, list_jobs, cancel_job, list_outputs, get_output_info, generate_media and
get_media_output_info. The published 0.2.0 has only the original ten. Use one request_id per logical operation
and reuse it on host retries. Unknown outcomes never automatically resubmit. Native batches require
verified limits. Edit sources are typed output_id/path/url/data_url objects; no ambiguous strings.
Long-running requests emit MCP `notifications/progress` updates when the client supplies a `progressToken`,
and `openai-responses` / `gemini-interactions` accept `wait: false` for background job creation while
keeping `store: false`.

Saved originals provide owned IDs, paths, actual MIME/dimensions/bytes/alpha/SHA256. Optional bounded
previews are derivatives. Local paths are not automatically accessible to cloud hosts. Prompts/input
bytes preserved unless explicit in-image translation or upscale instructions are requested; provider policy/terms still apply. Read [privacy](docs/PRIVACY.md).

## What Numera can generate

| Output type | Tools | Modes / inputs | Verified formats and limits |
| --- | --- | --- | --- |
| Still images | `generate_image`, `generate_media` | `text-to-image` (OpenAI Images/Responses, Gemini, OpenRouter, OmniRoute, 9router, ComfyUI, `fal-ai/flux/dev`) | `PNG`, `JPEG`, `WebP` original bytes preserved |
| Animated GIFs | `generate_image`, `edit_image` | Text-to-GIF and still-reference-to-GIF (`output_format: "gif"`, `frame_count` 2-16, `frame_duration_ms` 20-2000) | Single-frame and multi-frame `GIF87a`/`GIF89a` bytes preserved |
| Image edits | `edit_image`, `generate_media` | `image-to-image`, multi-reference, `edit_region` masks, `target_language` comic translation, native `upscale: true` | `PNG`, `JPEG`, `WebP`, `GIF` (native model output only; no local resize) |
| Video | `generate_media` | `text-to-video` (`kling-video` v2.5 turbo), `image-to-video` (`kling-video` v2.1), `video-to-video` (`cogvideox-5b`, `mmaudio-v2`) | Self-contained non-fragmented `MP4` (visual + optional `mp4a`/`Opus` audio) |
| Audio and soundtracks | `generate_media` | `text-to-audio` (`stable-audio`), `audio-to-audio` (`stable-audio-25`), `video-to-audio` (`fal-ai/mmaudio-v2`) | `WAV` (PCM, IEEE-float, `WAVE_FORMAT_EXTENSIBLE`) and muxed `MP4` (`mmaudio-v2`) |
| 3D meshes | `generate_media` | `text-to-3d` (`hunyuan3d-v3`, `hyper3d/rodin`), `image-to-3d` (`trellis`, `hyper3d/rodin`), `3d-to-3d` (`trellis-2/retexture`) | Self-contained `GLB 2.0` (including safe Khronos material/texture extensions) |
| Custom fal endpoints | `generate_media` | Opt-in `modelOverrides.<id>.falContract` definitions | Validated against the declared `image`, `video`, `audio`, or `3d` format rules |

## Animated GIF generation

Models that support OpenAI animated GIF output (such as `gpt-image-2.6-sunburst` and
`gpt-image-2.6-flare`) accept `output_format: "gif"` on `generate_image` (text prompt) and
`edit_image` (still reference images guiding the animation):

- `output_format`: `"gif"`
- `frame_count`: optional integer from `2` to `16` (upstream default `9`)
- `frame_duration_ms`: optional integer from `20` to `2000` milliseconds per frame (upstream default `200`, rounded to 10 ms)
- `size`: `"auto"` or `WIDTHxHEIGHT` with dimensions divisible by 16 within the model's GIF pixel budget

```json
{
  "model": "openai/gpt-image-2.6-flare",
  "prompt": "A glowing origami lantern pulsing gently in a dark room, seamless loop",
  "size": "auto",
  "quality": "low",
  "output_format": "gif",
  "frame_count": 4,
  "frame_duration_ms": 250,
  "request_id": "lantern-gif-001"
}
```

Numera validates the returned `GIF87a`/`GIF89a` header and saves the original animated bytes untouched
with `.gif` and `image/gif`. Do not combine `output_format: "gif"` with streaming, partial images,
masks, or `output_compression`. When routing through a gateway (such as OmniRoute), the gateway must
forward `output_format`, `frame_count`, and `frame_duration_ms` upstream; if the upstream returns PNG
instead, Numera preserves those bytes and records a format deviation with `status: "partial"`. See
[Compatibility](docs/COMPATIBILITY.md) and [OmniRoute](docs/OMNIROUTE.md).

## Video, audio and 3D

Declare both output kinds and exact transformations for each model. Full JSON uses
`modelOverrides.<exact-id>.outputTypes` and `modes`; compact JSON uses `models[].output_types` and
`modes`; dotenv uses comma-separated `MODEL_n_OUTPUT_TYPES` and `MODEL_n_MODES`. Kinds are
`image`, `video`, `audio`, `3d`. Omitting both declarations preserves legacy image configuration;
it does not silently enable a new media route. A configured mode is not account entitlement.

The [full fal example](examples/fal.json) declares every implemented endpoint's output kinds and modes,
including image sidecars and the GLB geometry selection for Rodin. It reads the credential from `FAL_KEY`;
no real credential belongs in a public example. Configuration alone does not submit a request.

For compact dotenv configuration, use the fal section with `https://queue.fal.run` (or an OmniRoute
`/v1` gateway prefix) and an exact implemented endpoint:

```dotenv
FAL_API_ENDPOINT=https://queue.fal.run
FAL_API_KEY=replace-with-your-private-fal-key
FAL_MODEL_1_ID=fal-ai/stable-audio
FAL_MODEL_1_NAME="Audio generation"
FAL_MODEL_1_ENABLED=true
FAL_MODEL_1_OUTPUT_TYPES=audio
FAL_MODEL_1_MODES=text-to-audio
```

A pre-0.5.0 file that selects `PROFILE=fal` on the main `API_*`/`MODEL_n_*` keys still loads as
connection `default`; it cannot be combined with `FAL_*` keys.

A tool request for that explicit configuration is:

```json
{
  "request_id": "my-audio-001",
  "model": "fal-ai/stable-audio",
  "mode": "text-to-audio",
  "prompt": "Gentle rain on a wooden roof",
  "parameters": {},
  "wait": false
}
```

Submit through `generate_media` only after authorizing the destination and cost. Use the same
`request_id` with `get_job`; cancellation does not verify a refund. Typed references carry `kind`,
`role` and exactly one `output_id`, `path`, `url` or `data_url`. Numera validates original bytes and
forwards inline data only where that exact endpoint implements the role; it makes no hidden upload.

Implemented endpoint bindings cover text/image-to-image, text/image/video-to-video, text/audio/video-to-audio
(including `fal-ai/mmaudio-v2`), and text/image/3D-to-3D paths individually, plus opt-in `falContract`
definitions for custom fal endpoints. Unsupported endpoint/mode, reference, parameter or format fails
explicitly. Original-byte validation covers PNG/JPEG/WebP/GIF, PCM/IEEE-float/WAVE_FORMAT_EXTENSIBLE WAV,
self-contained nonfragmented MP4 (visual track plus optional `mp4a`/`Opus` audio track), and a bounded
self-contained GLB 2.0 mesh subset (including safe Khronos material/texture extensions). Fragmented video,
compressed/animated/skinned GLB and other media formats are not supported by these validators. Structural
validation is not full decode, rendering, semantic quality or live-provider proof. Provider artifacts outside
these limits can yield partial or failed storage after generation; inspect the receipt, never resubmit automatically.
For the exact Flux dev image endpoints, explicit effective `num_images`, `output_format` and numeric
`image_size` requirements are saved with the request. Mismatching originals remain unchanged and
produce `deviations` with overall `partial` status; successful storage remains `completed`.
Local recovery uses that saved requirement snapshot, not later configuration defaults. Omitted
provider defaults, named size presets and other media parameters are not conformance guarantees.

`get_media_output_info` checks owned original bytes, actual MIME, kind, role, format, byte count,
SHA-256 and available kind-specific metadata. No invented image dimensions for audio or meshes.
[Configuration](docs/CONFIGURATION.md) records the exact endpoint and declaration contracts.

## Image-to-image, comic and selected-region editing

Use edit_image with approved reference_images for image-to-image/comic edits and authorized watermark removal. Add target_language
for translation replacing text inside the image; add edit_region={x,y,width,height} for an absolute
pixel rectangle on the first reference. A verified native mask route is required for region editing.
Use upscale=true with one reference and a larger numeric size to ask the same image model for a
higher-resolution edit; verified reference/size support is required, and actual size deviations remain
visible. Unsupported masks/parameters fail before submission. No local output resizing is performed.
[Examples, coordinate rules and limitations](docs/IMAGE-EDITING.md). Actual translation quality needs
a real authorized sample; offline wire/mask tests are not semantic proof.

## Ratios, resolutions and custom dimensions

Ask the agent to call `get_model_capabilities` for the exact model. Its `dimensions` result includes
built-in ratio presets, sourced model-native ratios/tiers/pixel sizes, custom-size support status and
separate selected-route parameter acceptance. Presets alone are suggestions, not support claims.
Unknown model IDs have unknown native dimensions rather than invented universal 4K support.

For documented Gemini 3.1 Flash Image, 14 ratios include 1:1, 16:9, 9:16, 3:2, 2:3 and 21:9; native tiers
are 0.5K/1K/2K/4K. The tool returns the complete per-ratio pixel table. 4K 16:9 maps to 5504x3072,
not 3840x2160. The examined OmniRoute path accepts 1K/2K/4K but not 0.5K; generic Images does not accept
undeclared `aspect_ratio`/`image_size`. Arbitrary custom pixels are not a native Gemini contract:
select a documented ratio+tier, or use an independently verified custom-size model. Numera never
silently resizes output to match. [Dimension selection details](docs/CONFIGURATION.md#dimension-discovery).

## Documentation

[Configuration](docs/CONFIGURATION.md) · [OmniRoute](docs/OMNIROUTE.md) · [9router](docs/9ROUTER.md) ·
[Claude](docs/CLAUDE.md) · [Codex](docs/CODEX.md) · [Windows/WSL](docs/WINDOWS-WSL.md) · [Linux](docs/LINUX.md) ·
[Architecture](docs/ARCHITECTURE.md) · [Testing](docs/TESTING.md) · [Requirements](docs/REQUIREMENTS.md) ·
[References](docs/REFERENCES.md)

## Native private maintenance

Version 0.3.0 adds Windows `numera-dev protect-secret --path ABSOLUTE_FILE` with hidden
CurrentUser DPAPI input and exclusive owner-only output. `--dry-run`/`--what-if` reads no secret and
creates no file. Native `identity inspect` and `identity create --dry-run`, both with explicit
`--root ABSOLUTE_ROOT`, inspect local conditions without creating custody or logs. Actual identity
creation/checking requires separately selected qualified native crypto-library inputs; no command
runs automatically or rotates the approved signer. [Credential details](docs/CONFIGURATION.md) and
[identity/custody limits](docs/SIGNING.md) describe the exact boundaries.

These commands and the all-51-file Rust tooling migration require a qualified 0.3.0 or later release;
they are not features of the historical 0.2.0 developer binary. Standard OS/native third-party tools remain permitted;
substantive first-party bootstrap/preparation logic belongs to the Rust driver, not hidden scripts.

## Logs and troubleshooting

New UTF8 JSON log per execution: `numera-gen-mcp_YYYY-MM-DD_HH-mm-ss_UTC-<pid>-<uuid>.log`, UTC
ISO8601 timestamp, INFO/WARNING/ERROR/DEBUG and component. Default directory configured/per-user;
14-day own runtime-log retention, initialization falls back stderr. Prompts/keys/auth/cookies/Base64/
signed query data/original paths not logged. Every accepted tool call records start/completion/failure,
run_id/call_id correlation, elapsed milliseconds and safe error code/stage; never arguments or raw
provider bodies. Event timestamp/level/component/run_id cannot be overwritten by context. DEBUG never
relaxes redaction. Attach only sanitized logs.

Catalog failure is not generation failure. Check route/base prefix/key/model before new authorization.
A text-only gateway cannot edit just because its model has native vision. Distinguish provider rejection,
partial stream, host timeout, OAuth/project/account problems, uncertain billing and storage permissions.
Never retry unknown generation automatically. Cancel waiting does not prove upstream cancellation/refund.

## License

Numera Gen MCP is licensed under **GPL-3.0-or-later**. You may redistribute it and/or modify it
under the GNU General Public License as published by the Free Software Foundation,
either version 3 of the License, or (at your option) any later version.
It is distributed without any warranty; see the complete [GPLv3 text](LICENSE).

## Donate

If this project helps you, donations are appreciated.

| Currency              | Network | Address                                            |
| --------------------- | ------- | -------------------------------------------------- |
| Bitcoin (BTC)         | Bitcoin | `bc1qmth5m03pu5hujw5xw5jmywam3jj3sqwqupesdt`       |
| USDT, BNB, USDC, etc. | BEP20   | `0x0Bd0BA443a8B9cf15922bf7f0Bb0a4b495fD06Ef`       |
| USDT, TRX, USDC, etc. | TRC20   | `TWBA3xFTqgZAeAYMxqo85xWnzvty3DcAhw`               |
| Ethereum (ETH)        | ERC20   | `0x0Bd0BA443a8B9cf15922bf7f0Bb0a4b495fD06Ef`       |
| TON                   | TON     | `UQCN8Umo_OfOWqImZetQsrNStPcmLkMAKajFyiCOhso23NDb` |
| Litecoin (LTC)        | LTC     | `ltc1qntqnnrunadurnw4cshv3qgspywrueyyeyngwuy`      |
| Solana (SOL)          | Solana  | `7B2wkczUjmkDhETwQuknBL8sUsbuV7nErxc317TmQuwR`     |
| Polygon (POL)         | Polygon | `0x0Bd0BA443a8B9cf15922bf7f0Bb0a4b495fD06Ef`       |
