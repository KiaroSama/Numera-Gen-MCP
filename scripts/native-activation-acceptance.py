"""Actual signed prepared-artifact maintenance after private signing inputs are removed."""
from __future__ import annotations
import ctypes
import json
import os
from pathlib import Path
import tempfile


def check(prepared: Path, runner) -> None:
    root = Path(__file__).resolve().parent.parent
    if os.environ.get("GITHUB_ACTIONS") != "true" or (root / ".ci-work/signing").exists():
        raise RuntimeError("Signed activation acceptance requires hosted execution without publisher inputs")
    if any(key.startswith("NUMERA_SIGNING_") for key in os.environ):
        raise RuntimeError("Private signing environment reached activation acceptance")
    manifest = json.loads((prepared / "manifest.json").read_text(encoding="utf-8"))
    if manifest["source_sha"] != os.environ["GITHUB_SHA"]:
        raise RuntimeError("Activation source identity differs")
    work = root / ".ci-work/activation"
    work.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=work, prefix="signed-") as directory:
        scratch = Path(directory)
        target = scratch / "installation with spaces"
        target.mkdir()
        suffix = ".exe" if os.name == "nt" else ""
        developer = prepared / "bin" / ("numera-dev" + suffix)
        executable = target / "bin" / ("numera-image-gen-mcp" + suffix)
        env = runner.environment()
        temp = scratch / "temp"
        temp.mkdir()
        env.update(TEMP=str(temp), TMP=str(temp), TMPDIR=str(temp))
        private = target / "config.local.json"
        private.write_bytes(b"private fixture unchanged")

        def setup(action, source=prepared, failure=False, host=None, config=None):
            args = [str(developer), "setup", action, "--root", str(target)]
            if action in ("Install", "Update"):
                args += ["--artifact", str(source)]
            else:
                args += ["--client", "ClaudeDesktop", "--host-config", str(host), "--config", str(config)]
            try:
                output = runner.run(args, wall=20, idle=10, env=env, cwd=scratch, capture=True)
            except RuntimeError as error:
                if not failure or str(error) != "Owned command failed with exit code 1":
                    raise
            else:
                if failure:
                    raise RuntimeError("Invalid activation was accepted")
                return json.loads(output)

        def installed_bytes():
            return {row["path"]:(target/row["path"]).read_bytes() for row in manifest["files"]}

        assert setup("Install")["installed"] is True
        original = installed_bytes()
        assert all(original[row["path"]] == (prepared/row["path"]).read_bytes() for row in manifest["files"])
        assert private.read_bytes() == b"private fixture unchanged"
        if os.name == "nt":
            # A held second member forces rollback after the first member has been replaced.
            second = manifest["files"][1]["path"]
            kernel = ctypes.WinDLL("kernel32", use_last_error=True)
            kernel.CreateFileW.argtypes = [ctypes.c_wchar_p,ctypes.c_ulong,ctypes.c_ulong,ctypes.c_void_p,ctypes.c_ulong,ctypes.c_ulong,ctypes.c_void_p]
            kernel.CreateFileW.restype = ctypes.c_void_p
            kernel.CloseHandle.argtypes = [ctypes.c_void_p]
            held = kernel.CreateFileW(str(target/second),0x80000000,1,None,3,0,None)
            if held in (None, ctypes.c_void_p(-1).value):
                raise RuntimeError("Cannot hold isolated rollback fixture")
            try:
                setup("Update", failure=True)
                assert installed_bytes() == original
                assert not (target/".numera-install.lock").exists()
                assert not any(p.name.startswith(".numera-staging-") for p in target.iterdir())
            finally:
                if not kernel.CloseHandle(held):
                    raise RuntimeError("Rollback fixture handle did not close")
        assert setup("Update")["installed"] is True
        assert installed_bytes() == original and private.read_bytes() == b"private fixture unchanged"
        host = scratch / "host.json"
        host.write_text('{"retained":true,"mcpServers":{"other":{"command":"unchanged"}}}',encoding="utf-8")
        config = scratch / "config.json"
        config.write_text('{"api_endpoint":"https://example.invalid/v1","api_key":"fixture-not-used","models":[{"id":"fixture","name":"Fixture"}]}',encoding="utf-8")
        setup("Register",host=host,config=config)
        registered = json.loads(host.read_text(encoding="utf-8"))
        assert registered["retained"] and registered["mcpServers"]["other"] == {"command":"unchanged"}
        assert Path(registered["mcpServers"]["numera-image-gen"]["command"]).samefile(executable)
        assert registered["mcpServers"]["numera-image-gen"]["args"] == ["bridge"]
        setup("Remove",host=host,config=config)
        assert json.loads(host.read_text(encoding="utf-8")) == {"retained":True,"mcpServers":{"other":{"command":"unchanged"}}}
        assert installed_bytes() == original and private.read_bytes() == b"private fixture unchanged"
        assert not (target/".numera-install.lock").exists()
        assert not any(p.name.startswith(".numera-staging-") for p in target.iterdir())
    print(json.dumps({"component":"native-activation","target":manifest["target"],"source_sha":manifest["source_sha"],
        "signed_install_update_register_remove":True,"private_files_preserved":True,"fixture_cleanup":True,
        "owner_host_changed":False,"public_os_trust":False}),flush=True)
