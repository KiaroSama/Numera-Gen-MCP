# Configuration

## Simple env setup (Windows, Linux)

Copy [.env.example](../.env.example) to .env in the extracted installation root (parent of `bin/`). Never
overwrite an existing private file. The native Rust parser preserves the established strict dotenv
contract; no Node runtime, dotenv dependency or process environment mutation is needed.

- API_ENDPOINT: any valid HTTP(S) API prefix, including hosted/custom deployment paths. Do not append
  /images/generations or /responses. No fixed localhost address or port is assumed.
- API_KEY: private nonempty key, without newline/NUL. Quotes are useful for values containing # or spaces.
- PROFILE: openai-images (default), omniroute, 9router, openai-responses, gemini,
  gemini-interactions, openrouter-images or fal. Gemini uses x-goog-api-key; fal uses Authorization: Key;
  other compact profiles use bearer auth.
- ORCHESTRATION_MODEL: required only for openai-responses, separate from the image model.
- MODEL_1_ID/NAME/ENABLED/OUTPUT_FORMAT/SIZE/QUALITY through MODEL_100_*: exact native ID, display name,
  true/false enabled flag and optional native output defaults. Paired MODEL_n_OUTPUT_TYPES/MODEL_n_MODES
  declare comma-separated media kinds/transformations; both empty/absent preserves legacy image slots.
  A single nonempty declaration requires the other; duplicates, spaces and empty tokens reject.
  Three consistent slots are provided.
- FAL_API_ENDPOINT, FAL_API_KEY and FAL_MODEL_1_* through FAL_MODEL_100_* (same fields as MODEL_n_*):
  optional second section, expanded to connection `fal` (adapter `fal-queue`). `https://queue.fal.run`
  selects direct provider queue mode with Authorization: Key; an HTTP/HTTPS `/v1` or `/api/v1` gateway prefix
  on another host (for example `http://localhost:20100/v1`) selects `gateway: "omniroute"` with Authorization:
  Bearer against `/v1/fal/run` and `/v1/fal/jobs/{id}`. Both FAL_API fields empty disable it, even with filled
  slots; one of them alone rejects. There is no FAL_PROFILE, FAL_EDIT_* or FAL_ORCHESTRATION_MODEL. The main
  section may stay empty when fal is configured, which makes `fal` the default connection; EDIT_* and
  ORCHESTRATION_MODEL then reject. PROFILE=fal together with any FAL_* key rejects.

With two connections, a call that omits `connection` is routed to the one whose enabled models list the
requested exact ID. An ID on both, or on neither, rejects with invalid_input before storage, credential
use or network I/O. Without a model the default connection is used; an explicit `connection` always
wins. Job polling and recovery select the stored connection name and never re-route.

At least one model must be enabled. First enabled slot is default; disabled IDs are denied and omitted
from configured listing. Empty output settings mean provider defaults; explicit tool arguments win.
OUTPUT_FORMAT accepts png/jpeg/webp/gif (where supported by the selected model). SIZE and QUALITY are native provider strings up to 100 characters,
not local resizing instructions. IDs are unique; labels are never routing aliases. Fully empty slots
are omitted. Nonempty slots need ID/name even when disabled.

Duplicate/unknown keys, malformed assignments, invalid flags, invalid UTF-8 and files above1MiB reject
with safe errors. Comments/quoted Unicode and multiline names retain the Node util.parseEnv contract;
API keys cannot contain newlines. No raw tools arrays or hidden extra submissions.

Plaintext keys are not encrypted: keep .env ignored, out of archives/packages/sync, private0600 on
Linux and current-user ACL on Windows. Outputs/state/logs/inputs sit beside the selected file,
independent of the caller CWD. [Responses example](../examples/simple-responses.env.example).

## Save without restarting

Before each MCP tool call Numera reads the selected file, validates a complete candidate and publishes
its connection/model snapshot atomically. Call list_models with refresh=false after saving to display
new configured IDs/names. No provider call or paid operation is triggered by saving.

Invalid/partial saves keep the last good snapshot and log one sanitized warning per changed file.
In-flight operations retain their original configuration. Poll/cancel of existing jobs requires the
persisted endpoint/account identity; unavailable or ambiguous transport settings refuse network I/O.
At most64 snapshots are retained per process, including the initial one; further changes require restart.
Storage roots, logging, file/security limits and global concurrency policy also require restart.

Selection: explicit --config > NUMERA_CONFIG > adjacent .env > adjacent legacy config.local.json >
per-user config.json. Windows per-user root: %LOCALAPPDATA%/Numera/ImageGen; Linux:
$XDG_CONFIG_HOME/numera-image-gen (fallback ~/.config/numera-image-gen). These legacy per-user roots
remain intentional compatibility locations for Numera Gen MCP; existing private config is not moved
or renamed. Explicit missing/invalid file fails rather than selecting a fallback. Packed installs should use their own absolute --config path,
not a private file inside node_modules. Selected .env/.env.* files use env parsing; JSON remains supported.

## Editing contract

Generic Images does not infer input forwarding from an endpoint/model ID. Enable only a verified
contract using EDIT_MODE (multipart/json/generation), EDIT_ENCODING
(image/image[]/images/image_urls/input_references), EDIT_MAX_REFERENCES (1-32), EDIT_MASKS (true/false)
and EDIT_MASK_POLARITY (transparent-edit/white-edit/black-edit). Nonempty edit fields require EDIT_MODE.

For an actual OpenAI Images edit contract, a typical configuration is:

```dotenv
EDIT_MODE=multipart
EDIT_ENCODING=image
EDIT_MAX_REFERENCES=1
EDIT_MASKS=true
EDIT_MASK_POLARITY=transparent-edit
```

Multipart supports repeated `image` or `image[]` parts. JSON/generation `image` is scalar and
must have `maxReferences=1`; a model override cannot raise that serializer capacity. For multiple
JSON references select a verified `images`, `image_urls` or `input_references` array contract.
Incompatible mode/encoding/capacity combinations reject before paid submission, never drop references.

Do not paste this into a gateway whose edit/mask route is unverified. Native Gemini image references
use its documented protocol without these Images fields; native masks are not implemented there.
[Editing and translation](IMAGE-EDITING.md) explains region coordinates and route limitations.

## Advanced JSON compatibility

[examples/config.json](../examples/config.json), schemaVersion1 and
[config schema](../schemas/config.schema.json) remain supported. Compact legacy JSON input uses
[its schema](../schemas/simple-config.schema.json). Named connections have independent adapter/base/
prefix/auth/model/defaults/policy, absolute roots, explicit edit contracts and ComfyUI workflow bindings.

Precedence: supported CLI flags > namespaced process environment > selected file > defaults.
CLI: --config, --default-connection, --output-dir, --state-dir, --log-dir, --log-level,
--request-timeout-ms, --max-concurrent-requests, --return-mode, --openai-compat.
Equivalent process environment: NUMERA_CONFIG, NUMERA_DEFAULT_CONNECTION, NUMERA_OUTPUT_DIR,
NUMERA_STATE_DIR, NUMERA_LOG_DIR, NUMERA_LOG_LEVEL, NUMERA_REQUEST_TIMEOUT_MS,
NUMERA_MAX_CONCURRENT_REQUESTS, NUMERA_RETURN_MODE. These are not compact .env keys.
OPENAI_BASE_URL/OPENAI_API_KEY/OPENAI_MODEL apply only with --openai-compat.

Auth is none for explicitly trusted local backends, bearer or header with exactly one private apiKey,
secretEnv, absolute/private/bounded secretFile or Windows current-user secretDpapiFile.
auth.origin binds credentials to the destination. Environment variables are not encrypted.
The 0.3.0 source adds `numera-dev protect-secret --path ABSOLUTE_FILE`; its own hosted qualification is
pending. On Windows it reads the secret from the attached console without echo, encrypts UTF-16LE
with CurrentUser DPAPI and writes lowercase hexadecimal UTF-8. The existing native reader accepts
that format. `--dry-run`, `--what-if` and `-WhatIf` validate without reading a secret or writing output.
The explicit destination may be outside the project. Existing ancestors must be unlinked directories;
missing parent directories are created sequentially only after valid secret input and native encryption.
WhatIf and input failure create no parents. Creation is exclusive with a protected current-user-only
ACL; existing files and parent ACLs remain unchanged. Plaintext arguments, environment input and non-Windows plaintext fallbacks
are unsupported. The published 0.2.0 developer binary does not acquire this new command.
Missing credentials never authorize reading another application's store.

Per-model maxCount/maxReferences/supportedParameters/capabilities overrides need verified contracts.
Defaults use snake_case common names. providerOptionKeys permits bounded provider options, never
credentials/destination/model/prompt/image overrides. Pixels (size), shape (aspect_ratio) and tiers
(image_size) are distinct; unsupported parameters fail, never disappear. Dedicated OpenRouter endpoint
descriptors pin compatible provider.only with allow_fallbacks=false; failed discovery means unknown.

## Dimension discovery

`get_model_capabilities` returns `dimensions.preset_aspect_ratios` as a built-in suggestion catalog;
`presets_are_model_support=false` prevents treating every listed ratio as universally accepted.
`model_support` contains sourced native ratios, tiers, exact pixel sizes and `custom_dimensions.status`.
For unsupported/unknown native custom sizes it explains why; unknown model IDs have null native lists.
`route_support` separately reports request fields accepted by Numera, documented gateway tier limits
and the local max_pixels ceiling. These facts do not establish account entitlement or actual output.

The Gemini 3.1 Flash Image native table includes 14 ratios and 0.5K/1K/2K/4K. For 16:9 these produce
688x384/1376x768/2752x1536/5504x3072. Gemini 3 Pro Image (including its preview alias)
has10 documented ratios and1K/2K/4K, with the same corresponding pixel sizes, no0.5K or extreme
1:4/1:8/4:1/8:1 ratios. Documentation does not prove the selected Antigravity account can use Pro.
Aspect labels and native pixels are discrete; do not calculate
exact dimensions from a ratio label or interpret4K as a universal video size. OmniRoute Antigravity
accepts only1K/2K/4K tiers through `image_size`; `size` is a gateway aspect lookup. Generic Images
accepts `size` but not undeclared ratio/tier fields; selecting a model does not change its profile.
For custom widthxheight, use only a route/model with a verified pixel contract and stay within local
pixel limits. Always inspect actual saved dimensions/deviations; no hidden resize or provider fallback.
`documented_limits` exposes sourced maximum edge/pixel count separately from route safety limits.
For GPT Image2 and2.5 Sunburst/Flare the public API contract permits dimensions in multiples of16,
aspect between 1:3 and 3:1, edge at most 3840 and 655360 to 8294400 total pixels. Resolutions above
2560x1440 are experimental;3840x2160 fits that contract,4096x4096 and native8K do not.
Numera rejects numeric requests outside known model limits before submission. Opaque orchestration
IDs such as Sol have unknown limits; a catalog entry or Codex OAuth route does not establish the
underlying engine, size forwarding or account access. Always call capabilities before generation.

Aspect/tier/evidence requirements persist with the receipt. Normal completion and local recovery
compare original output dimensions identically; mismatches are deviations, not resized successes.
A tier without an exact model/aspect table is explicitly reported as unverifiable.
Official sources: https://ai.google.dev/gemini-api/docs/image-generation#aspect-ratios-and-image-size
and https://developers.openai.com/api/docs/guides/image-generation#customize-image-output .

api-prefix preserves reverse-proxy prefixes; origin mode requires an origin-only base and adds the
native prefix once. Full operation URLs, credentials/query/fragments/traversal reject. Proxy is explicit
connection-scoped. Asset URL trust is independent of API trust. Defaults: input20MiB, output50MiB,
aggregate100MiB, private result-journal budget256MiB,64M pixels,14 references,
preview512px/256KiB; route limits may be stricter.

## Explicit media models (0.3.0 source; qualification pending)

Full JSON declares `connections.<name>.modelOverrides.<exact-id>.outputTypes` and `modes`; compact
JSON declares `models[].output_types` and `modes`. Both are bounded, nonempty unique lists when supplied.
Kinds are `image`, `video`, `audio`, `3d`; each mode's output kind must be declared. Every exact fal
endpoint must declare every kind its binding may return, including image sidecars on Rodin/Hunyuan.
A declaration is not account access or successful generation evidence. An optional
`modelOverrides.<exact-id>.editModel` sends `operation=edit` requests to a different provider model id
while the requested id stays the public receipt identity. Capabilities, defaults, allow/deny policy,
parameter evidence and the request identity all resolve against the edit model id; `generate` requests
are unaffected. Legacy image models need no new fields. Other adapters can declare only their existing text-to-image/image-to-image paths.

For fal use the FAL_* dotenv section (or, in a single-section file, profile `fal`; full adapter
`fal-queue`). Direct mode requires `https://queue.fal.run`, Key authentication and an exact implemented
endpoint (owned loopback HTTP origins are allowed only for offline fixtures). OmniRoute gateway mode
(`gateway: "omniroute"`) requires an HTTP/HTTPS `/v1` or `/api/v1` prefix on a host other than
`queue.fal.run` and Bearer authentication, submitting queued work to `/v1/fal/run` with `Idempotency-Key`
and polling `/v1/fal/jobs/{id}` (`?result=true` for output, `DELETE` for cancellation, `/v1/fal/models`
for health probes). No automatic catalog API, file upload, account switch or SDK runtime is introduced.
The [complete public fal example](../examples/fal.json) uses `secretEnv: "FAL_KEY"` and declares every
implemented model explicitly. Replace its three `/absolute/numera-gen-mcp` storage/log paths with
absolute paths beneath your own installation before use. Copy it only to a new private configuration;
do not replace an existing file. Rodin selects `geometry_file_format: "glb"`; explicit USDZ/FBX/OBJ/STL requests reject before
submission because these original-byte validators are not implemented. An incompatible returned format
still fails validation without conversion.

| Exact endpoint | Modes | Output kinds |
| --- | --- | --- |
| `fal-ai/flux/dev` | `text-to-image` | image |
| `fal-ai/flux/dev/image-to-image` | `image-to-image` | image |
| `fal-ai/kling-video/v2.5-turbo/pro/text-to-video` | `text-to-video` | video |
| `fal-ai/kling-video/v2.1/standard/image-to-video` | `image-to-video` | video |
| `fal-ai/cogvideox-5b/video-to-video` | `video-to-video` | video |
| `fal-ai/stable-audio` | `text-to-audio` | audio |
| `fal-ai/stable-audio-25/audio-to-audio` | `audio-to-audio` | audio |
| `fal-ai/hunyuan3d-v3/text-to-3d` | `text-to-3d` | 3d, image |
| `fal-ai/trellis` | `image-to-3d` | 3d |
| `fal-ai/hyper3d/rodin` | `text-to-3d`, `image-to-3d` | 3d, image |
| `fal-ai/trellis-2/retexture` | `3d-to-3d` | 3d |
| `fal-ai/mmaudio-v2` | `video-to-audio`, `video-to-video` | video, audio |

Retexture requires both a `mesh` kind3d and `image` kindimage reference; `fal-ai/mmaudio-v2` requires a
`video` reference and accepts bounded `duration`, `num_steps`, `cfg_strength`, `mask_away_clip`, `seed`,
and `negative_prompt` parameters. Custom non-built-in fal endpoints can declare an opt-in
`modelOverrides.<id>.falContract` block (`outputField`, `outputArray`, `outputKind`, optional
`prompt`/`references`/`allowedParameters` with strict type/range/enum bounds); built-in endpoint IDs and
path-traversal IDs are rejected. Image/audio/video transforms require their exact input
role. Native `parameters` are endpoint-specific: for example Stable Audio uses `seconds_total`,
Audio25 uses `total_seconds`, and Kling uses string `duration`. Connection defaults, model defaults,
then explicit parameters merge before preflight and request fingerprinting. Unsupported parameters,
roles, cardinalities and model policy reject before source transfer or a paid POST.

`generate_media` returns a durable receipt. `list_jobs`, `get_job`, and `cancel_job` inspect or manage
owned handles; local wait cancellation is independent of provider cancellation or refund. Originals are
verified as PNG/JPEG/WebP/GIF (including multi-frame animated `GIF87a`/`GIF89a` via `generate_image`
and `edit_image` with `output_format: "gif"`, `frame_count` 2-16, and `frame_duration_ms` 20-2000),
WAV (PCM, IEEE-float, and `WAVE_FORMAT_EXTENSIBLE`), self-contained non-fragmented MP4 (visual track
plus optional `mp4a` AAC or `Opus` audio track), or a bounded self-contained GLB 2.0 mesh subset
(including safe Khronos material/texture extensions). Other containers/codecs/features fail explicitly
without conversion.
Release 0.3.0 retains up to 64 normalized media items through the shared schema v2 Store: legacy
indices 0-9 stay unchanged, media-only overflow 10-63 uses an additive table in the same database and
shares byte budgets, writer fences and stable publication. No second job database or old-binary
media-state downgrade is supported. Excessive responses fail, never silently truncate. Structural
validation is not full playback/rendering or live-provider proof. Storage can fail after a billable
generation; never resubmit automatically.

`get_media_output_info` verifies original kind, role, MIME, format, bytes, hash and available structural
metadata. New media resources use `numera-gen://outputs/<id>`; the old image URI remains a compatibility
alias. No arbitrary local path becomes an MCP resource.

Advanced `files.maxJournalBytes` caps outstanding
result reservations/retained payloads. Numera reserves the per-request aggregate budget before a paid
POST and refuses new work when capacity is unavailable; it never purges unresolved results to make room.
