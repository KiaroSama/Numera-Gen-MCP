"""Preserve actual locked runtime dependency notices from the hosted Cargo registry."""
from __future__ import annotations

import json
import os
from pathlib import Path
import importlib.util
import sys

ROOT = Path(__file__).resolve().parent.parent


def main() -> None:
    if os.environ.get("GITHUB_ACTIONS") != "true":
        raise RuntimeError("License resolution is hosted CI only")
    command = ["cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", os.environ["NUMERA_NATIVE_TARGET"]]
    spec = importlib.util.spec_from_file_location("native_ci", ROOT / "scripts/native-ci.py")
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    response = runner.run(command, cwd=ROOT, env=runner.environment(), wall=120, idle=45,
                          capture=True, capture_limit=16 * 1024 * 1024)
    metadata = json.loads(response.decode("utf-8"))
    packages = {item["id"]: item for item in metadata["packages"]}
    nodes = {item["id"]: item for item in metadata["resolve"]["nodes"]}
    root = metadata["resolve"]["root"]
    pending = [root]
    selected = set()
    while pending:
        item = pending.pop()
        if item in selected:
            continue
        selected.add(item)
        for dependency in nodes[item]["deps"]:
            if any(kind["kind"] in [None, "build"] for kind in dependency["dep_kinds"]):
                pending.append(dependency["pkg"])
    output = ROOT / ".ci-work/native/licenses"
    output.mkdir(parents=True, exist_ok=True)
    inventory = []
    for identity in sorted(selected - {root}):
        package = packages[identity]
        directory = Path(package["manifest_path"]).parent
        if not directory.resolve().is_relative_to(ROOT / ".ci-cache/cargo"):
            raise RuntimeError("Resolved dependency is not in the workspace Cargo registry")
        if not package.get("license") and not package.get("license_file"):
            raise RuntimeError("Resolved dependency has no declared license")
        names = []
        for file in sorted(directory.rglob("*")):
            if not file.is_file() or file.is_symlink():
                continue
            name = file.name.upper()
            if not name.startswith(("LICENSE", "LICENCE", "COPYING", "COPYRIGHT", "NOTICE", "UNLICENSE")):
                continue
            if file.stat().st_size > 1024 * 1024:
                raise RuntimeError("Dependency notice is unexpectedly large")
            relative = file.relative_to(directory)
            target = output / (package["name"] + "-" + package["version"]) / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(file.read_bytes())
            names.append(relative.as_posix())
        if not names:
            raise RuntimeError("Actual dependency notice files are missing")
        inventory.append({"name": package["name"], "version": package["version"], "license": package.get("license"),
                          "repository": package.get("repository"), "notices": names})
    (output / "inventory.json").write_text(json.dumps(inventory, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"component": "native-licenses", "resolved_packages": len(inventory)}))


if __name__ == "__main__":
    try:
        main()
    except Exception:
        print("Resolved native dependency notices could not be verified.", file=sys.stderr)
        raise SystemExit(1) from None
