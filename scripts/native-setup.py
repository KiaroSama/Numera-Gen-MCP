"""Install verified official Rust toolchains only inside a hosted project workspace."""
from __future__ import annotations

import hashlib
import os
from pathlib import Path
import platform
import sys
import urllib.request

import importlib.util

_spec = importlib.util.spec_from_file_location("native_ci", Path(__file__).with_name("native-ci.py"))
_ci = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_ci)
environment, run, emit, ROOT = _ci.environment, _ci.run, _ci.emit, _ci.ROOT


def fetch(url: str, limit: int) -> bytes:
    with urllib.request.urlopen(url, timeout=30) as response:
        data = response.read(limit + 1)
    if len(data) > limit:
        raise RuntimeError("Official download exceeds bound")
    return data


def main() -> None:
    if os.environ.get("GITHUB_ACTIONS") != "true":
        raise RuntimeError("Rust toolchain installation is hosted CI only")
    env = environment()
    machine = platform.machine().lower()
    arch = "aarch64" if machine in ["arm64", "aarch64"] else "x86_64" if machine in ["amd64", "x86_64"] else None
    system = {"Windows": "pc-windows-msvc", "Linux": "unknown-linux-gnu", "Darwin": "apple-darwin"}.get(platform.system())
    if arch is None or system is None:
        raise RuntimeError("Unsupported hosted target")
    target = arch + "-" + system
    name = "rustup-init.exe" if os.name == "nt" else "rustup-init"
    url = f"https://static.rust-lang.org/rustup/archive/1.29.1/{target}/{name}"
    data = fetch(url, 64 * 1024 * 1024)
    expected = fetch(url + ".sha256", 4096).decode("ascii").split()[0]
    if len(expected) != 64 or hashlib.sha256(data).hexdigest() != expected:
        raise RuntimeError("Official rustup checksum mismatch")
    download = ROOT / ".ci-work/native/toolchain" / name
    download.parent.mkdir(parents=True, exist_ok=True)
    download.write_bytes(data)
    download.chmod(0o700)
    run([str(download), "-y", "--no-modify-path", "--profile", "minimal", "--default-toolchain", "1.99.0"], wall=600, idle=180, env=env)
    rustup = str(Path(env["CARGO_HOME"]) / "bin" / ("rustup.exe" if os.name == "nt" else "rustup"))
    run([rustup, "component", "add", "rustfmt", "clippy", "--toolchain", "1.99.0"], wall=180, idle=90, env=env)
    if "--coverage" in sys.argv:
        run([rustup, "toolchain", "install", "nightly-2026-10-10", "--profile", "minimal", "--component", "llvm-tools-preview"], wall=600, idle=180, env=env)
    with open(os.environ["GITHUB_PATH"], "a", encoding="utf-8") as file:
        file.write(str(Path(env["CARGO_HOME"]) / "bin") + "\n")
    with open(os.environ["GITHUB_ENV"], "a", encoding="utf-8") as file:
        for key in ["CARGO_HOME", "RUSTUP_HOME", "CARGO_TARGET_DIR", "TEMP", "TMP", "TMPDIR", "npm_config_cache"]:
            file.write(f"{key}={env[key]}\n")
    emit("INFO", "Official Rust setup completed inside workspace.", target=target, rust="1.99.0", rustup="1.29.1")


if __name__ == "__main__":
    main()
