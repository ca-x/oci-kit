"""Accept the real static CLI inside disposable roots; never initialize the host."""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import unittest


def expected_calls(*keyrings):
    return [
        action
        for keyring in keyrings
        for action in (f"--add /usr/share/pacman/keyrings/{keyring}.gpg", f"--populate {keyring}")
    ]


class InitRuntime(unittest.TestCase):
    binary: Path
    helper: Path
    runner: str

    def setUp(self):
        directory = tempfile.TemporaryDirectory(prefix="oci-kit-init-")
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        for name in (
            "usr/local/bin", "usr/bin", "usr/share/pacman/keyrings",
            "etc/oci-kit", "etc/lightos", "var/lib/lightos", "proc", "dev",
        ):
            (self.root / name).mkdir(parents=True, exist_ok=True)
        shutil.copy2(self.binary, self.root / "usr/local/bin/lighos-init")
        shutil.copy2(self.helper, self.root / "usr/bin/pacman-key")
        self.marker = self.root / "var/lib/lightos/initialized"
        self.marker.write_text("platform fixture\n")
        self.seed = self.root / "etc/lightos/seed.env"
        self.seed.write_text("opaque-fixture-only\n")
        self.state = self.root / "var/lib/oci-kit/lightos-init.json"
        self.config = self.root / "etc/oci-kit/lightos.toml"
        (self.root / "usr/share/pacman/keyrings/pond.gpg").write_bytes(b"fixture")
        self.profile("pond")

    def profile(self, *keyrings):
        self.config.write_text("schema_version=1\npacman_keyrings=" + json.dumps(keyrings))

    def command(self, apply=True):
        executable = shutil.which(self.runner)
        self.assertIsNotNone(executable, f"missing runner: {self.runner}")
        if self.runner == "bwrap":
            command = [
                executable, "--unshare-all", "--die-with-parent", "--new-session",
                "--uid", "0", "--gid", "0", "--bind", str(self.root), "/",
                "--proc", "/proc", "--dev", "/dev", "--",
            ]
        else:
            self.assertEqual(os.geteuid(), 0, "chroot runner requires root")
            command = [executable, str(self.root)]
        command += ["/usr/local/bin/lighos-init", "apply"]
        if apply:
            command.append("--apply")
        return command

    def run_init(self, apply=True, timeout=20):
        return subprocess.run(
            self.command(apply), capture_output=True, text=True, timeout=timeout,
            env={**os.environ, "PATH": "/fixture-untrusted-path"},
        )

    def success(self, result, changed):
        self.assertEqual(result.returncode, 0, result.stderr)
        document = json.loads(result.stdout)
        self.assertEqual(document["platform"], "lightos")
        self.assertEqual(document["apply"], True)
        self.assertEqual(document["changed"], changed)
        return document

    def failure(self, result, message):
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertIn(message, result.stderr)

    def calls(self):
        path = self.root / "calls"
        return path.read_text().splitlines() if path.exists() else []

    def test_plan_without_platform_marker_does_not_mutate(self):
        self.marker.unlink()
        result = self.run_init(apply=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(json.loads(result.stdout)["apply"])
        self.assertFalse(self.state.parent.exists())
        self.assertEqual(self.calls(), [])

    def test_missing_marker_refuses_before_commands_or_state(self):
        self.marker.unlink()
        self.failure(self.run_init(), "account setup has not completed")
        self.assertFalse(self.state.parent.exists())
        self.assertEqual(self.calls(), [])

    def test_all_keyrings_are_preflighted(self):
        self.profile("pond", "absent")
        self.failure(self.run_init(), "keyring is absent: absent")
        self.assertFalse(self.state.exists())
        self.assertEqual(self.calls(), [])

    def test_failed_import_is_not_hidden_by_populate_success(self):
        (self.root / "usr/share/pacman/keyrings/pond.gpg").write_bytes(b"invalid")
        self.failure(self.run_init(), "pacman-key failed")
        self.assertFalse(self.state.exists())
        self.assertEqual(self.calls(), ["--add /usr/share/pacman/keyrings/pond.gpg"])

    def test_untrusted_state_directory_is_refused(self):
        self.state.parent.mkdir(parents=True)
        self.state.parent.chmod(0o777)
        self.failure(self.run_init(), "state")
        self.assertFalse(self.state.exists())
        self.assertEqual(self.calls(), [])

    def test_nonregular_state_and_lock_are_refused_without_blocking(self):
        self.state.parent.mkdir(parents=True)
        for path in (self.state, self.state.with_name("lightos-init.lock")):
            with self.subTest(path=path.name):
                path.unlink(missing_ok=True)
                os.mkfifo(path)
                try:
                    self.failure(self.run_init(timeout=2), "regular file")
                    self.assertEqual(self.calls(), [])
                finally:
                    path.unlink()

    def test_symlinked_state_directory_is_refused(self):
        (self.root / "other-state").mkdir()
        self.state.parent.symlink_to("/other-state", target_is_directory=True)
        self.failure(self.run_init(), "state")
        self.assertEqual(list((self.root / "other-state").iterdir()), [])
        self.assertEqual(self.calls(), [])

    def test_symlinked_state_and_lock_files_are_refused(self):
        self.state.parent.mkdir(parents=True)
        target = self.root / "outside"
        target.write_text("retained")
        for path in (self.state, self.state.with_name("lightos-init.lock")):
            with self.subTest(path=path.name):
                path.unlink(missing_ok=True)
                path.symlink_to("/outside")
                try:
                    self.failure(self.run_init(), "regular file")
                    self.assertEqual(target.read_text(), "retained")
                    self.assertEqual(self.calls(), [])
                finally:
                    path.unlink()

    def test_writable_state_and_lock_files_are_refused(self):
        self.state.parent.mkdir(parents=True)
        for path in (self.state, self.state.with_name("lightos-init.lock")):
            with self.subTest(path=path.name):
                path.write_text("retained")
                path.chmod(0o666)
                try:
                    self.failure(self.run_init(), "not group/world-writable")
                    self.assertEqual(path.read_text(), "retained")
                    self.assertEqual(self.calls(), [])
                finally:
                    path.unlink()

    def test_success_is_one_json_and_repetition_preserves_platform_state(self):
        result = self.run_init()
        document = self.success(result, True)
        self.assertIn("fixture pacman-key output", result.stderr)
        self.assertEqual(json.loads(self.state.read_text())["fingerprint"], document["fingerprint"])
        self.assertEqual(self.calls(), expected_calls("pond"))
        self.assertEqual((self.root / "observed-path").read_text(), "/usr/bin:/usr/sbin")
        original = self.state.read_bytes()
        self.success(self.run_init(), False)
        self.assertEqual(self.state.read_bytes(), original)
        self.assertEqual(self.calls(), expected_calls("pond"))
        self.assertEqual(self.seed.read_text(), "opaque-fixture-only\n")
        self.assertEqual(self.marker.read_text(), "platform fixture\n")

    def test_failed_command_has_no_success_state_and_retry_works(self):
        (self.root / "fail").touch()
        self.failure(self.run_init(), "pacman-key failed for pond")
        self.assertFalse(self.state.exists())
        (self.root / "fail").unlink()
        self.success(self.run_init(), True)
        self.assertEqual(self.calls(), ["--add /usr/share/pacman/keyrings/pond.gpg"] + expected_calls("pond"))

    def test_changed_profile_retries_without_overwriting_old_success_on_failure(self):
        first = self.success(self.run_init(), True)
        (self.root / "usr/share/pacman/keyrings/extra.gpg").write_bytes(b"fixture")
        self.profile("pond", "extra")
        (self.root / "fail").touch()
        self.failure(self.run_init(), "pacman-key failed")
        self.assertEqual(json.loads(self.state.read_text())["fingerprint"], first["fingerprint"])
        (self.root / "fail").unlink()
        second = self.success(self.run_init(), True)
        self.assertNotEqual(first["fingerprint"], second["fingerprint"])
        self.assertEqual(self.calls(), expected_calls("pond") + ["--add /usr/share/pacman/keyrings/pond.gpg"] + expected_calls("pond", "extra"))

    def test_concurrent_process_refuses_without_stealing_the_lock(self):
        (self.root / "block").touch()
        first = subprocess.Popen(self.command(), stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        try:
            deadline = time.monotonic() + 10
            while not (self.root / "entered").exists():
                self.assertIsNone(first.poll(), "first initializer exited early")
                self.assertLess(time.monotonic(), deadline, "fixture never acquired lock")
                time.sleep(0.02)
            self.failure(self.run_init(), "another LightOS initialization is running")
            self.assertEqual(self.calls(), ["--add /usr/share/pacman/keyrings/pond.gpg"])
            self.assertFalse(self.state.exists())
            (self.root / "release").touch()
            stdout, stderr = first.communicate(timeout=10)
            self.success(subprocess.CompletedProcess(first.args, first.returncode, stdout, stderr), True)
        finally:
            if first.poll() is None:
                first.kill()
            first.communicate(timeout=10)

    def test_corrupt_or_unknown_state_fails_closed(self):
        self.state.parent.mkdir(parents=True)
        for state in ("not-json", '{"schema_version":2,"fingerprint":"unknown"}'):
            self.state.write_text(state)
            self.failure(self.run_init(), "state")
            self.assertEqual(self.state.read_text(), state)
            self.assertEqual(self.calls(), [])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--pacman-key-fixture", type=Path, required=True)
    parser.add_argument("--runner", choices=("bwrap", "chroot"), default="bwrap")
    arguments = parser.parse_args()
    InitRuntime.binary = arguments.binary.resolve(strict=True)
    InitRuntime.helper = arguments.pacman_key_fixture.resolve(strict=True)
    InitRuntime.runner = arguments.runner
    unittest.main(argv=[__file__], verbosity=2)
