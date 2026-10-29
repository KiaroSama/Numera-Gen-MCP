# Claude clients

One-time host registration is required. After registration Claude launches the native executable
itself with `args: ["bridge"]`; no launcher, daemon or manual server startup is needed. Numera is
stdio MCP in `bridge` mode. The optional separate [direct HTTP mode](HTTP.md) listens locally on
configurable port1969; it is not required for Desktop's process entry. Numera does not replace Claude's chat model.

All 470 frozen cases/50 suites and all six real signature/package/extracted SDK checks passed at
`f253557` / run `40244894089`, attempt2, all sixteen jobs successful. [Signed delivery](releases/0.2.0.md)
records exact archives and deliberate self-signed trust limits. CI and isolated native readiness are
not actual Desktop reconnection. Install the verified binary before using the examples below;
retain the previous runtime until the actual host connection is verified.

The current examples use Numera Gen MCP's `numera-gen-mcp` executable and `numera-gen` host entry.
The immutable published 0.2.0 archives retain their legacy executable name; use that actual name for
those archives. A saved `numera-image-gen` entry is not automatically renamed or removed: back it up
and explicitly migrate the selected Numera entry rather than running two copies against shared state.

## Claude Desktop

Merge only `mcpServers.numera-gen`, preserving every other server and setting. Back up the
actual `claude_desktop_config.json` before editing. Windows packaged/MSIX installations can keep
that file under the package's LocalCache/Roaming/Claude; use the app's Edit Config or an explicitly
selected existing file, never guess or overwrite the whole settings file.

```json
{
  "mcpServers": {
    "numera-gen": {
      "command": "C:/tools/numera-gen-mcp/bin/numera-gen-mcp.exe",
      "args": ["bridge"]
    }
  }
}
```

Replace the example command with the absolute installed binary path. On macOS the command is an
absolute POSIX path ending in `bin/numera-gen-mcp`; the arguments remain `["bridge"]`.
Server support on Linux does not establish Claude Desktop application availability on Linux.

Adjacent `.env` or `config.local.json` discovery is relative to the executable's installation root
(parent of `bin/`), not the host CWD. To retain a config elsewhere, add only Numera's `env` member
with an absolute `NUMERA_CONFIG` path. Never put the API key in host configuration or arguments.
[The complete explicit-config example](../examples/claude-desktop.json) is distinct from Claude Code's
project format.

Fully quit/reopen Desktop after changing the command. Actually call `health_check` and
`list_connections` to confirm the new native connection. A terminal environment-variable change
does not affect an already-running Desktop process. SDK/CI stdio tests are not actual Claude GUI tests.

## Claude Code

From the extracted installation root on Linux/macOS:

```bash
claude mcp add --transport stdio --scope user numera-gen -- "$PWD/bin/numera-gen-mcp" bridge
```

Windows PowerShell, replacing the example path:

```powershell
claude mcp add --transport stdio --scope user numera-gen -- 'C:/tools/numera-gen-mcp/bin/numera-gen-mcp.exe' bridge
```

Project registration can instead merge [the project `.mcp.json` entry](../examples/claude-project.json).
These are instructions, not host registration performed by this project task.
Current official documentation: https://code.claude.com/docs/en/mcp .

After registration only endpoint/key/models and explicit media declarations in the private application
configuration need changing; call `list_models` after saving. Invalid saves retain the last valid snapshot. Hosted/cloud clients
cannot inherently run your local binary or see local output paths; preview rendering is host-dependent.
