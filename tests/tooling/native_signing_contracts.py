"""Offline signing-policy checks; platform signatures require separate real hosted evidence."""
from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import os
import sys
import threading
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("native_signing", ROOT / "scripts/native-signing.py")
signing = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(signing)


class NativeSigning(unittest.TestCase):
    def setUp(self):
        directory = ROOT / ".ci-work/native/signing-policy-tests"
        directory.mkdir(parents=True, exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(prefix="policy-", dir=directory)
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def test_only_approved_public_material_is_used(self):
        identity = signing.public_identity()
        self.assertEqual(identity["publisher"], "KiaroSama")
        self.assertEqual(identity["public_key_sha256"], signing.KEY_PIN)
        self.assertEqual(identity["certificate_sha256"], signing.CERT_PIN)
        with patch.object(signing, "KEY_PIN", "0" * 64):
            with self.assertRaisesRegex(signing.SigningError, "identity_mismatch"):
                signing.public_identity()

    def test_duplicate_metadata_and_unsafe_members_are_rejected(self):
        with self.assertRaises(signing.SigningError):
            json.loads('{"publisher":"KiaroSama","publisher":"other"}', object_pairs_hook=signing.pairs)
        for name in ("", "../outside", "/absolute", "bin/../file", "bin\\file", "bin/file:stream", "bin/file."):
            with self.subTest(name=name), self.assertRaises(signing.SigningError):
                signing.member(self.root, name)

    def test_private_inputs_are_not_read_without_explicit_hosted_authorization(self):
        for env in ({}, {"GITHUB_ACTIONS": "true"}, {"NUMERA_SIGNING_AUTHORIZED": "true"}):
            with patch.dict(signing.os.environ, env, clear=True), patch.object(signing, "read") as read:
                with self.assertRaisesRegex(signing.SigningError, "not_authorized"):
                    signing.signing_inputs()
                read.assert_not_called()

    def test_password_bound_includes_the_stdin_newline(self):
        private = self.root / ".ci-work/signing"
        private.mkdir(parents=True)
        pfx = private / "fixture.pfx"
        pfx.write_bytes(b"not a real private identity")
        password = private / "password.txt"
        env = {"GITHUB_ACTIONS": "true", "NUMERA_SIGNING_AUTHORIZED": "true",
               "NUMERA_SIGNING_PFX": str(pfx), "NUMERA_SIGNING_PASSWORD_FILE": str(password)}
        with patch.object(signing, "ROOT", self.root), patch.dict(signing.os.environ, env, clear=True):
            password.write_bytes(b"x" * 4095)
            self.assertEqual(len(signing.signing_inputs()[1]), 4096)
            password.write_bytes(b"x" * 4096)
            with self.assertRaisesRegex(signing.SigningError, "password_format"):
                signing.signing_inputs()

    def test_public_native_verifier_uses_pinned_leaf_and_never_changes_os_trust(self):
        identity = signing.public_identity()
        public = self.root / "publisher.pem"
        public.write_bytes(b"public key is not a CA certificate")
        calls = []
        def command(args, **kwargs):
            calls.append((args, kwargs))
            return b""
        with patch.object(signing, "tool", return_value="verified-tool"), patch.object(signing, "command", side_effect=command):
            signing.native_verify(self.root / "application.exe", "x86_64-pc-windows-msvc", self.root, identity)
        self.assertEqual(len(calls), 1)
        args, kwargs = calls[0]
        self.assertEqual(args[1], "verify")
        self.assertIn("sha256:" + signing.CERT_PIN, args)
        certificate = Path(args[args.index("-CAfile") + 1])
        self.assertNotEqual(certificate, public)
        self.assertTrue(certificate.read_bytes().startswith(b"-----BEGIN CERTIFICATE-----"))
        self.assertNotIn("secret", kwargs)
        self.assertNotIn("-ignore-crl", args)
        self.assertNotIn("-ignore-timestamp", args)

    def test_prepared_input_cannot_list_private_or_unapproved_runtime_members(self):
        for name in ("secrets.md", ".ai/memory.md", "state/private.db", "docs/config.local.json", "bin/con", "bin/com1.exe"):
            with self.subTest(name=name), self.assertRaises(signing.SigningError):
                signing.distribution_name(name)
        self.assertEqual(signing.distribution_name("bin/numera-dev.exe"), "bin/numera-dev.exe")
        self.assertEqual(signing.distribution_name(".env.example"), ".env.example")

    def test_real_runner_keeps_stdin_private_bounds_output_and_joins_readers(self):
        runner = signing.RUNNER
        errors = io.StringIO()
        before = set(threading.enumerate())
        env = {**os.environ, "TEMP": str(self.root), "TMP": str(self.root), "TMPDIR": str(self.root)}
        payload = b"synthetic-private-test-value"
        with contextlib.redirect_stderr(errors):
            result = runner.run([sys.executable, "-I", "-B", "-c",
                "import sys; value=sys.stdin.buffer.read(); sys.stderr.buffer.write(value); sys.stdout.buffer.write(b'verified')"],
                input_data=payload, private=True, capture=True, capture_limit=1024,
                wall=5, idle=2, env=env, cwd=self.root)
            with self.assertRaisesRegex(RuntimeError, "output exceeds bound"):
                runner.run([sys.executable, "-I", "-B", "-c", "import sys; sys.stderr.buffer.write(b'x'*4096)"],
                           private=True, capture_limit=128, wall=5, idle=2, env=env, cwd=self.root)
        self.assertEqual(result, b"verified")
        self.assertNotIn(payload.decode("ascii"), errors.getvalue())
        self.assertEqual(set(threading.enumerate()), before)
        self.assertIn("Owned command stopped", errors.getvalue())

    def test_unsigned_final_archive_is_rejected_before_any_output(self):
        spec = importlib.util.spec_from_file_location("signing_package", ROOT / "scripts/native-package.py")
        package = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(package)
        with patch.object(package, "ROOT", self.root):
            with self.assertRaisesRegex(RuntimeError, "Unsigned final"):
                package.archive({"manifest.json": b"{}"}, "x86_64-unknown-linux-gnu")
        self.assertFalse((self.root / ".ci-work/release").exists())

    def test_private_tool_output_and_password_never_enter_arguments_or_child_environment(self):
        password = b"synthetic-passphrase\n"
        with patch.object(signing, "RUNNER") as runner, patch.dict(signing.os.environ,
                {"NUMERA_SIGNING_PFX": "private-input", "NUMERA_SIGNING_PASSWORD_FILE": "private-password", "GITHUB_TOKEN": "fixture-token"}):
            runner.run.return_value = b"public-result"
            result = signing.command(["openssl", "dgst", "-passin", "stdin"], secret=password, capture=True)
        args, kwargs = runner.run.call_args
        self.assertEqual(args[0], ["openssl", "dgst", "-passin", "stdin"])
        self.assertEqual(kwargs["input_data"], password)
        self.assertTrue(kwargs["private"])
        self.assertEqual((kwargs["wall"], kwargs["idle"]), (30, 15))
        self.assertFalse(any(key.startswith("NUMERA_SIGNING_") for key in kwargs["env"]))
        self.assertNotIn("GITHUB_TOKEN", kwargs["env"])
        self.assertEqual(result, b"public-result")


if __name__ == "__main__":
    unittest.main()
