"""Small deterministic validation-tree seam; never modify the immutable parity oracle."""
from __future__ import annotations

import copy
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


def load():
    file = ROOT / "scripts/native-validation.py"
    assert file.is_file(), "Separate maintained validation preparation is missing"
    spec = importlib.util.spec_from_file_location("validation_contract", file)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def archive(files):
    data = io.BytesIO()
    with tarfile.open(fileobj=data, mode="w") as bundle:
        for name, payload in files.items():
            item = tarfile.TarInfo(name)
            item.size = len(payload)
            bundle.addfile(item, io.BytesIO(payload))
    return data.getvalue()


class ValidationTree(unittest.TestCase):
    def test_only_reviewed_dependencies_and_supervisor_change_in_separate_tree(self):
        validation = load()
        package = {"private": True, "dependencies": {"zod": "4.6.5", "undici": "8.11.2", "unchanged": "1.0.0"}}
        lock = {"lockfileVersion": 3, "packages": {
            "": copy.deepcopy(package),
            "node_modules/zod": {"version": "4.6.5", "resolved": "https://registry.npmjs.org/zod/-/zod-4.6.5.tgz",
                                 "integrity": "original", "license": "MIT"},
            "node_modules/undici": {"version": "8.11.2", "resolved": "https://registry.npmjs.org/undici/-/undici-8.11.2.tgz",
                                    "integrity": "original-undici", "license": "MIT", "engines": {"node": ">=22.19.0"}},
            "node_modules/unchanged": {"version": "1.0.0", "integrity": "same"},
        }}
        files = {"package.json": json.dumps(package).encode("utf-8"),
                 "package-lock.json": json.dumps(lock).encode("utf-8"),
                 "scripts/bounded.mjs": b"original supervisor\n",
                 "tests/integration/setup.test.ts": (
                     'const fixture = ' + json.dumps("import { spawnSync } from 'node:child_process';\nconst r=spawnSync('pwsh',['-NoProfile','-NonInteractive','-Command',process.argv[2]],{windowsHide:true,encoding:'utf8',timeout:10000,stdio:['ignore','pipe','pipe']});process.stdout.write(r.stdout??'');process.stderr.write(r.stderr??'');process.exitCode=r.status??1;")
                     + ';\nexpect(result.code, result.stderr).toBe(0);\n').encode("utf-8"),
                 "tests/exact.test.ts": 'const label = "تصویر";\n'.encode("utf-8"),
                 "src/exact.ts": b"export const fixture = 1;\n"}
        directory = ROOT / ".ci-work/native/tooling-tests"
        directory.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="validation ", dir=directory) as temp:
            root = Path(temp)
            (root / "scripts").mkdir()
            (root / "scripts/bounded.mjs").write_bytes(b"maintained supervisor\n")
            original = root / ".ci-work/baseline"
            original.mkdir(parents=True)
            for name, data in files.items():
                file = original / name
                file.parent.mkdir(parents=True, exist_ok=True)
                file.write_bytes(data)
            receipt = validation.prepare(root, archive(files))
            output = root / ".ci-work/validation"
            self.assertEqual({name: (original / name).read_bytes() for name in files}, files)
            self.assertEqual((output / "tests/exact.test.ts").read_bytes(), files["tests/exact.test.ts"])
            self.assertEqual((output / "src/exact.ts").read_bytes(), files["src/exact.ts"])
            expected = copy.deepcopy(lock)
            expected["packages"][""]["dependencies"]["zod"] = "4.6.7"
            expected["packages"]["node_modules/zod"].update(validation.ZOD)
            expected["packages"][""]["dependencies"]["undici"] = "8.12.3"
            expected["packages"]["node_modules/undici"].update({
                "version": "8.12.3", "resolved": "https://registry.npmjs.org/undici/-/undici-8.12.3.tgz",
                "integrity": "sha512-+fdndUTwBP86cVjihTp21hZHo7d6oj1c/47XPA3lL8mM0XHs+C4EkcjmgRsFnFk2f/UONGDy9ItBItmXsLOQHw=="})
            self.assertEqual(json.loads((output / "package-lock.json").read_text(encoding="utf-8")), expected)
            self.assertEqual(json.loads((output / "package.json").read_text(encoding="utf-8")),
                             {**package, "dependencies": {**package["dependencies"], "zod": "4.6.7", "undici": "8.12.3"}})
            self.assertEqual((output / "scripts/bounded.mjs").read_bytes(), b"maintained supervisor\n")
            self.assertEqual(set(receipt["changes"]), {"package.json", "package-lock.json", "scripts/bounded.mjs", "tests/integration/setup.test.ts"})
            before = files["tests/integration/setup.test.ts"].decode("utf-8")
            after = (output / "tests/integration/setup.test.ts").read_text(encoding="utf-8")
            self.assertEqual(after.replace(validation.STREAM_FIXTURE, validation.SYNC_FIXTURE), before)
            self.assertIn("setTimeout(()=>child.kill('SIGKILL'),10000)", after)
            self.assertEqual(after.count("clearTimeout(timer)"), 2)
            self.assertNotIn("spawnSync", after)
            self.assertIn("child.stdout.pipe(process.stdout);child.stderr.pipe(process.stderr)", after)
            self.assertNotIn("tests/integration/setup.test.ts", receipt["unchanged_tests"])
            self.assertIn("tests/integration/setup.test.ts", receipt["assertion_preserving_test_updates"])
            self.assertEqual(receipt["unchanged_tests"]["tests/exact.test.ts"], hashlib.sha256(files["tests/exact.test.ts"]).hexdigest())
            with self.assertRaises(RuntimeError):
                validation.prepare(root, archive(files))
            for label, changed in [("traversal", {"../outside": b"bad"}),
                                   ("duplicate", {"src/exact.ts": b"bad"}),
                                   ("unexpected-version", {"package.json": b'{"dependencies":{"zod":"4.6.6"}}'}),
                                   ("unexpected-undici", {"package.json": json.dumps({**package, "dependencies": {**package["dependencies"], "undici": "8.12.1"}}).encode("utf-8")}),
                                   ("unexpected-fixture", {"tests/integration/setup.test.ts": b"unreviewed fixture\n"})]:
                with self.subTest(label=label):
                    isolated = root / label
                    (isolated / "scripts").mkdir(parents=True)
                    (isolated / "scripts/bounded.mjs").write_bytes(b"maintained supervisor\n")
                    data = archive({**files, **changed})
                    if label == "duplicate":
                        data = archive({**files, "./src/exact.ts": b"bad"})
                    with self.assertRaises(RuntimeError):
                        validation.prepare(isolated, data)
                    self.assertFalse((isolated / ".ci-work/validation").exists())
                    self.assertFalse((isolated / ".ci-work/outside").exists())


if __name__ == "__main__":
    unittest.main()
