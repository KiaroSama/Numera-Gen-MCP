"""Pinned owner signature verification and explicitly provisioned hosted-only signing.

No credentials are discovered, uploaded, installed into trust stores, or passed in argv.
"""
from __future__ import annotations

import argparse
import base64
from contextlib import contextmanager, nullcontext
import datetime as dt
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import stat
import sys
import tempfile
import uuid

ROOT = Path(__file__).resolve().parent.parent
KEY_PIN = "31ccb588ae7e529535dcccd94f06d8d39ae1b8847b7003e884ad8f44cd340c80"
CERT_PIN = "657dd7869e90e2bf9a49942d5098e1e3a82e22cf84add51aa958e3dfd0af937f"
TARGETS = {arch + suffix for arch in ("x86_64", "aarch64") for suffix in
           ("-pc-windows-msvc", "-unknown-linux-gnu", "-apple-darwin")}
MAX_BYTES = 256 * 1024 * 1024
_RUNNER_SPEC = importlib.util.spec_from_file_location("signing_ci_runner", ROOT / "scripts/native-ci.py")
RUNNER = importlib.util.module_from_spec(_RUNNER_SPEC)
_RUNNER_SPEC.loader.exec_module(RUNNER)


class SigningError(RuntimeError):
    """Fixed diagnostic code, never a private process output or path."""


def pairs(items: list[tuple[str, object]]) -> dict[str, object]:
    result = {}
    for key, value in items:
        if key in result:
            raise SigningError("duplicate_metadata_field")
        result[key] = value
    return result


def read(file: Path, limit: int) -> bytes:
    before = file.lstat()
    if (not stat.S_ISREG(before.st_mode) or before.st_nlink != 1
            or getattr(before, "st_file_attributes", 0) & 0x400 or before.st_size > limit):
        raise SigningError("regular_signing_input_required")
    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_NONBLOCK", 0) | getattr(os, "O_BINARY", 0)
    with os.fdopen(os.open(file, flags), "rb") as stream:
        info = os.fstat(stream.fileno())
        if (not stat.S_ISREG(info.st_mode) or info.st_nlink != 1
                or (info.st_dev, info.st_ino) != (before.st_dev, before.st_ino)):
            raise SigningError("linked_or_changed_signing_input")
        value = stream.read(limit + 1)
    if len(value) > limit:
        raise SigningError("signing_input_bound")
    return value


def public_identity() -> dict:
    identity = json.loads(read(ROOT / "docs/signing/publisher.json", 8192).decode("utf-8"),
                          object_pairs_hook=pairs)
    key = read(ROOT / "docs/signing/publisher-key.der", 8192)
    cert = read(ROOT / "docs/signing/publisher-cert.der", 16384)
    if (hashlib.sha256(key).hexdigest() != KEY_PIN or hashlib.sha256(cert).hexdigest() != CERT_PIN
            or identity.get("public_key_sha256") != KEY_PIN or identity.get("certificate_sha256") != CERT_PIN
            or identity.get("publisher") != "KiaroSama" or identity.get("algorithm") != "rsa-pkcs1-sha256"):
        raise SigningError("approved_public_identity_mismatch")
    now = dt.datetime.now(dt.timezone.utc)
    if not dt.datetime.fromisoformat(identity["not_before"]) <= now <= dt.datetime.fromisoformat(identity["not_after"]):
        raise SigningError("signer_outside_validity")
    return identity


def tool(name: str) -> str:
    configured = os.environ.get("NUMERA_" + name.upper())
    path = Path(configured) if configured else Path(shutil.which(name) or "")
    if not path.is_absolute() or not path.is_file():
        raise SigningError("required_signing_tool_unavailable")
    return str(path.resolve())


def codesign_failure(code: int, output: bytes) -> None:
    diagnostic = output.lower()
    for messages, reason in (
        ((b"no identity found", b"specified item could not be found in the keychain"), "identity_not_found"),
        ((b"unable to build chain to self-signed root",), "chain_unavailable"),
        ((b"user interaction is not allowed", b"interaction with the security server is not allowed"), "interaction_required"),
        ((b"errsecinternalcomponent",), "internal_component"),
        ((b"resource fork, finder information, or similar detritus not allowed",), "extended_attributes"),
    ):
        if any(message in diagnostic for message in messages):
            raise SigningError("macos_signing_" + reason)
    raise SigningError("macos_signing_rejected")


def command(args: list[str], *, secret: bytes | None = None, capture: bool = False) -> bytes:
    # Only installed, explicitly selected cryptographic tools are executed. Capture is never logged.
    env = {key: value for key, value in os.environ.items()
           if not key.startswith("NUMERA_SIGNING_") and key not in {"GH_TOKEN", "GITHUB_TOKEN"}}
    try:
        options = {"private_error": codesign_failure} if args[0] == "/usr/bin/codesign" else {}
        return RUNNER.run(args, input_data=secret, private=True, capture=capture,
                          capture_limit=1024 * 1024, wall=30, idle=15, cwd=ROOT, env=env, **options)
    except SigningError:
        raise
    except (OSError, RuntimeError, TimeoutError):
        raise SigningError("signing_tool_failed_or_rejected") from None


def pem(label: str, data: bytes) -> bytes:
    encoded = base64.b64encode(data)
    return (f"-----BEGIN {label}-----\n".encode("ascii")
            + b"\n".join(encoded[i:i + 64] for i in range(0, len(encoded), 64))
            + f"\n-----END {label}-----\n".encode("ascii"))


def distribution_name(name: str) -> str:
    parts = name.split("/")
    allowed = {"bin", "docs", "schemas", "examples", "licenses", "README.md", "LICENSE",
               "THIRD_PARTY_NOTICES.md", "SECURITY.md", ".env.example", "manifest.json", "manifest.sig"}
    private = {"secrets.md", "config.local.json", "state", "outputs", "logs", "node_modules", "http-token", "http.json"}
    devices = {"con", "prn", "aux", "nul"} | {f"{prefix}{number}" for prefix in ("com", "lpt") for number in range(1, 10)}
    if (not name or "\\" in name or parts[0] not in allowed
            or any(not part or part in {".", ".."} or part.casefold() in private
                   or part.startswith(".") and name != ".env.example"
                   or re.search(r'[\x00-\x1f:<>"|?*]', part) or part.endswith((".", " "))
                   or part.split(".")[0].casefold() in devices for part in parts)):
        raise SigningError("unsafe_distribution_member")
    return name


def member(root: Path, name: str) -> Path:
    distribution_name(name)
    path = root
    parts = name.split("/")
    for part in parts:
        path = path / part
        if path.is_symlink() or (hasattr(path.lstat(), "st_file_attributes")
                                and path.lstat().st_file_attributes & 0x400):
            raise SigningError("linked_distribution_member")
    if not path.resolve().is_relative_to(root):
        raise SigningError("escaping_distribution_member")
    return path


def manifest(root: Path, target: str) -> tuple[dict, bytes]:
    if target not in TARGETS or not root.is_absolute() or root.is_symlink():
        raise SigningError("invalid_distribution_target")
    root = root.resolve(strict=True)
    data = read(member(root, "manifest.json"), 1024 * 1024)
    value = json.loads(data.decode("utf-8"), object_pairs_hook=pairs)
    if (set(value) != {"version", "target", "source_sha", "publisher", "binary_sha256", "files"}
            or value["target"] != target or value["publisher"] != "KiaroSama"
            or not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", value["version"])
            or not re.fullmatch(r"[0-9a-f]{40}", value["source_sha"])
            or not isinstance(value["files"], list) or not 1 <= len(value["files"]) <= 4096):
        raise SigningError("invalid_distribution_metadata")
    names, total = set(), 0
    for row in value["files"]:
        if (not isinstance(row, dict) or set(row) != {"path", "sha256", "bytes"} or not isinstance(row["path"], str)
                or type(row["bytes"]) is not int or not 0 <= row["bytes"] <= MAX_BYTES
                or not re.fullmatch(r"[0-9a-f]{64}", row["sha256"]) or row["path"].casefold() in names):
            raise SigningError("invalid_distribution_inventory")
        if row["path"] in {"manifest.json", "manifest.sig"}:
            raise SigningError("self_referencing_distribution_manifest")
        names.add(row["path"].casefold())
        total += row["bytes"]
        if total > MAX_BYTES:
            raise SigningError("distribution_bound")
        content = read(member(root, row["path"]), row["bytes"])
        if len(content) != row["bytes"] or hashlib.sha256(content).hexdigest() != row["sha256"]:
            raise SigningError("distribution_member_changed")
    binaries = ["bin/numera-image-gen-mcp", "bin/numera-dev"]
    if "windows" in target:
        binaries = [name + ".exe" for name in binaries]
    if any(name not in names for name in binaries) or "docs/assets/logo.png" not in names:
        raise SigningError("distribution_binary_or_logo_missing")
    if read(member(root, "docs/assets/logo.png"), MAX_BYTES) != read(ROOT / "docs/assets/logo.png", MAX_BYTES):
        raise SigningError("unapproved_distribution_logo")
    original = next(row for row in value["files"] if row["path"] == binaries[0])
    if original["sha256"] != value["binary_sha256"]:
        raise SigningError("distribution_binary_identity")
    expected = {row["path"] for row in value["files"]} | {"manifest.json"}
    if (root / "manifest.sig").exists():
        expected.add("manifest.sig")
    actual, visited = set(), 0
    for parent, directories, files in os.walk(root, followlinks=False):
        visited += len(directories) + len(files)
        if visited > 8192:
            raise SigningError("distribution_tree_bound")
        for name in directories + files:
            relative = (Path(parent) / name).relative_to(root).as_posix()
            member(root, relative)
        actual.update((Path(parent) / name).relative_to(root).as_posix() for name in files)
    if actual != expected:
        raise SigningError("unlisted_distribution_member")
    return value, data


def native_verify(file: Path, target: str, scratch: Path, identity: dict) -> None:
    if "windows" in target:
        certificate = scratch / "publisher-certificate.pem"
        if not certificate.exists():
            certificate.write_bytes(pem("CERTIFICATE", read(ROOT / "docs/signing/publisher-cert.der", 16384)))
        command([tool("osslsigncode"), "verify", "-CAfile", str(certificate), "-ignore-cdp",
                 "-require-leaf-hash", "sha256:" + CERT_PIN, "-in", str(file)])
    elif "apple" in target:
        if sys.platform != "darwin":
            raise SigningError("native_macos_verifier_required")
        certificate = ROOT / "docs/signing/publisher-cert.der"
        command(["/usr/bin/codesign", "--verify", "--strict", "--all-architectures", "-R",
                 f'anchor "{certificate}"', str(file)])
        prefix = scratch / (file.name + "-certificate-")
        command(["/usr/bin/codesign", "--display", "--extract-certificates", str(prefix), str(file)])
        if hashlib.sha256(read(Path(str(prefix) + "0"), 16384)).hexdigest() != CERT_PIN:
            raise SigningError("native_signer_mismatch")
    # Linux ELF bytes are authenticated by the verified manifest, not imaginary ELF OS enforcement.


def verify(directory: Path, target: str) -> dict:
    identity = public_identity()
    root = directory.resolve(strict=True)
    value, _ = manifest(root, target)
    work = ROOT / ".ci-work/signature-checks"
    work.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="verify-", dir=work) as temp:
        scratch = Path(temp)
        public = scratch / "publisher.pem"
        public.write_bytes(pem("RSA PUBLIC KEY", read(ROOT / "docs/signing/publisher-key.der", 8192)))
        read(member(root, "manifest.sig"), 1024)
        command([tool("openssl"), "dgst", "-sha256", "-verify", str(public), "-signature",
                 str(root / "manifest.sig"), str(root / "manifest.json")])
        for row in value["files"]:
            if row["path"].startswith("bin/"):
                native_verify(member(root, row["path"]), target, scratch, identity)
    return {"target": target, "source_sha": value["source_sha"], "publisher": "KiaroSama",
            "public_key_sha256": KEY_PIN, "verified": True, "public_os_trust": False}


def signing_inputs() -> tuple[Path, bytes]:
    if os.environ.get("GITHUB_ACTIONS") != "true" or os.environ.get("NUMERA_SIGNING_AUTHORIZED") != "true":
        raise SigningError("protected_hosted_signing_not_authorized")
    paths = []
    for key in ("NUMERA_SIGNING_PFX", "NUMERA_SIGNING_PASSWORD_FILE"):
        raw = os.environ.get(key)
        path = Path(raw) if raw else Path()
        private = ROOT / ".ci-work/signing"
        if not path.is_absolute() or not path.resolve(strict=True).is_relative_to(private.resolve(strict=True)):
            raise SigningError("protected_signing_input_required")
        paths.append(path)
    read(paths[0], 1024 * 1024)
    password = read(paths[1], 4096)
    if not re.fullmatch(rb"[A-Za-z0-9+/=]{1,4095}", password):
        raise SigningError("protected_password_format")
    check_pfx_certificate(paths[0], password + b"\n", public_identity())
    return paths[0], password + b"\n"


def check_pfx_certificate(pfx: Path, password: bytes, identity: dict) -> None:
    output = command([tool("openssl"), "pkcs12", "-in", str(pfx), "-clcerts", "-nokeys",
                      "-passin", "stdin"], secret=password, capture=True)
    blocks = re.findall(rb"-----BEGIN CERTIFICATE-----\s*([A-Za-z0-9+/=\s]+?)-----END CERTIFICATE-----", output)
    try:
        if (len(blocks) != 1 or hashlib.sha256(base64.b64decode(re.sub(rb"\s", b"", blocks[0]), validate=True)).hexdigest()
                != identity["certificate_sha256"]):
            raise SigningError("selected_pfx_certificate_mismatch")
    except ValueError:
        raise SigningError("selected_pfx_certificate_mismatch") from None


def security_input(arguments: list[str]) -> None:
    # security -i returns only the last command's status: submit exactly one bounded command.
    if not arguments or len(arguments) > 32 or any(re.search(r"[\x00-\x1f\x7f]", item) for item in arguments):
        raise SigningError("invalid_private_keychain_command")
    line = " ".join('"' + item.replace("\\", "\\\\").replace('"', '\\"') + '"' for item in arguments)
    payload = (line + "\n").encode("utf-8")
    if len(payload) >= 4096:
        raise SigningError("private_keychain_command_bound")
    command(["/usr/bin/security", "-i"], secret=payload)


def keychain_state() -> tuple[bytes, bytes]:
    return tuple(command(["/usr/bin/security", action, "-d", "user"], capture=True)
                 for action in ("list-keychains", "default-keychain"))


def macos_identity_status(keychain: Path, identity: dict) -> dict[str, bool]:
    output = command(["/usr/bin/security", "find-identity", "-p", "codesigning", str(keychain)], capture=True)
    matching, separator, valid = output.partition(b"Valid identities only")
    pin = re.escape(identity["certificate_thumbprint"].encode("ascii"))
    pattern = rb"(?m)^\s*[0-9]+\)\s+" + pin + rb"\s"
    return {"matching_identity_present": bool(re.search(pattern, matching)),
            "policy_valid_identity_present": bool(separator and re.search(pattern, valid))}


@contextmanager
def macos_identity(pfx: Path, password: bytes, identity: dict):
    if sys.platform != "darwin":
        raise SigningError("native_macos_signer_required")
    private = ROOT / ".ci-work/signing"
    if not private.resolve(strict=True).is_relative_to(ROOT.resolve(strict=True)):
        raise SigningError("owned_keychain_staging_required")
    if not pfx.is_absolute() or not pfx.resolve(strict=True).is_relative_to(private.resolve(strict=True)):
        raise SigningError("protected_signing_input_required")
    if not re.fullmatch(rb"[A-Za-z0-9+/=]{1,4095}\n", password):
        raise SigningError("protected_password_format")
    passphrase = password[:-1].decode("ascii")
    before = keychain_state()
    with tempfile.TemporaryDirectory(prefix="identity-", dir=private) as directory:
        scratch = Path(directory)
        scratch.chmod(0o700)
        keychain = scratch / "numera-signing.keychain-db"
        try:
            security_input(["create-keychain", "-p", passphrase, str(keychain)])
            security_input(["unlock-keychain", "-p", passphrase, str(keychain)])
            security_input(["import", str(pfx), "-k", str(keychain), "-f", "pkcs12", "-t", "agg",
                            "-x", "-T", "/usr/bin/codesign", "-P", passphrase])
            security_input(["set-key-partition-list", "-S", "apple-tool:,apple:", "-s", "-k",
                            passphrase, str(keychain)])
            certificates = command(["/usr/bin/security", "find-certificate", "-a", "-p", str(keychain)], capture=True)
            blocks = re.findall(rb"-----BEGIN CERTIFICATE-----\s*([A-Za-z0-9+/=\s]+?)-----END CERTIFICATE-----", certificates)
            if len(blocks) != 1 or hashlib.sha256(base64.b64decode(re.sub(rb"\s", b"", blocks[0]), validate=True)).hexdigest() != identity["certificate_sha256"]:
                raise SigningError("imported_native_identity_mismatch")
            if keychain_state() != before:
                raise SigningError("keychain_preferences_changed")
            RUNNER.emit("INFO", "Pinned native identity availability inspected without changing trust.",
                        **macos_identity_status(keychain, identity))
            yield keychain
        finally:
            try:
                if keychain.exists():
                    command(["/usr/bin/security", "delete-keychain", str(keychain)])
                    if keychain.exists():
                        raise SigningError("owned_keychain_cleanup_failed")
            finally:
                if keychain_state() != before:
                    raise SigningError("keychain_preferences_changed")


def sign(directory: Path, target: str) -> dict:
    identity = public_identity()
    pfx, password = signing_inputs()
    root = directory.resolve(strict=True)
    if not root.is_relative_to(ROOT / ".ci-work"):
        raise SigningError("owned_signing_staging_required")
    value, _ = manifest(root, target)
    if value["source_sha"] != os.environ.get("GITHUB_SHA") or (root / "manifest.sig").exists():
        raise SigningError("signing_candidate_identity")
    with macos_identity(pfx, password, identity) if "apple" in target else nullcontext() as keychain:
        for row in value["files"]:
            if not row["path"].startswith("bin/"):
                continue
            file = member(root, row["path"])
            if "windows" in target:
                signed = file.with_name(file.name + ".signed-" + uuid.uuid4().hex)
                command([tool("osslsigncode"), "sign", "-pkcs12", str(pfx), "-readpass", "-", "-h", "sha256",
                         "-n", "Numera Image-Gen MCP", "-in", str(file), "-out", str(signed)], secret=password)
                os.replace(signed, file)
            elif "apple" in target:
                command(["/usr/bin/codesign", "--force", "--sign", identity["certificate_thumbprint"],
                         "--keychain", str(keychain), "--timestamp=none", str(file)])
    for row in value["files"]:
        data = read(member(root, row["path"]), MAX_BYTES)
        row.update(bytes=len(data), sha256=hashlib.sha256(data).hexdigest())
    original = "bin/numera-image-gen-mcp" + (".exe" if "windows" in target else "")
    value["binary_sha256"] = next(row["sha256"] for row in value["files"] if row["path"] == original)
    (root / "manifest.json").write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")
    command([tool("openssl"), "dgst", "-sha256", "-keyform", "P12", "-sign", str(pfx), "-passin", "stdin",
             "-out", str(root / "manifest.sig"), str(root / "manifest.json")], secret=password)
    return verify(root, target)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=("sign", "verify"))
    parser.add_argument("--target", required=True, choices=sorted(TARGETS))
    parser.add_argument("--directory", required=True, type=Path)
    args = parser.parse_args()
    result = (sign if args.action == "sign" else verify)(args.directory, args.target)
    print(json.dumps({"timestamp": dt.datetime.now(dt.timezone.utc).isoformat(), "level": "INFO",
                      "component": "native-signing", **result}), flush=True)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, TypeError, StopIteration, SigningError) as error:
        print(json.dumps({"timestamp": dt.datetime.now(dt.timezone.utc).isoformat(), "level": "ERROR",
                          "component": "native-signing", "code": str(error) if isinstance(error, SigningError)
                          else "signing_input_or_capability_unavailable"}), file=sys.stderr, flush=True)
        raise SystemExit(1) from None
