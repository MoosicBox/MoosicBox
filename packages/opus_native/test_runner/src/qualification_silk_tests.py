"""SILK serial-runner tests: subprocesses and provenance mocked; no codec execution."""

import json
from pathlib import Path
import subprocess
import tempfile
from unittest.mock import patch
import unittest

import qualification_silk as silk


def passing(case, interleaved=False):
    prefix = f"test {silk.TEST} ... " if interleaved else ""
    success = "ok" if interleaved else f"test {silk.TEST} ... ok"
    return (
        f"running 1 test\n{prefix}internal SILK CASE START {case}\n"
        f"internal SILK matrix executed=1 failures=0\n{success}\n"
        "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 900 filtered out; finished in 1s\n"
    )


class SilkTests(unittest.TestCase):
    def test_public_interface_uses_separate_exact_test_and_markers(self):
        self.assertEqual(silk.test_name("internal"), silk.TEST)
        self.assertEqual(silk.test_name("public"), silk.PUBLIC_TEST)
        with self.assertRaises(ValueError):
            silk.test_name("invalid")
        case = silk.cases()[0]
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "public.log"
            path.write_text(
                passing(case)
                .replace(silk.TEST, silk.PUBLIC_TEST)
                .replace("internal SILK", "public SILK")
            )
            self.assertTrue(silk.log_passes(path, case, "public"))
            self.assertFalse(silk.log_passes(path, case, "internal"))
            path.write_text(passing(case))
            self.assertFalse(silk.log_passes(path, case, "public"))

    def test_public_child_command_and_filter_are_exact(self):
        case = silk.cases()[0]

        class Child:
            def wait(self, timeout):
                return 0

        def launch(command, **kwargs):
            self.assertEqual(
                command[1:], [silk.PUBLIC_TEST, "--exact", "--ignored", "--nocapture"]
            )
            self.assertEqual(kwargs["env"]["MOOSICBOX_OPUS_SILK_CASE"], case)
            kwargs["stdout"].write(
                passing(case)
                .replace(silk.TEST, silk.PUBLIC_TEST)
                .replace("internal SILK", "public SILK")
                .encode()
            )
            kwargs["stdout"].flush()
            return Child()

        with tempfile.TemporaryDirectory() as directory, patch.object(
            silk.subprocess, "Popen", side_effect=launch
        ):
            result = silk.run_case(
                Path("unit"),
                Path("oracle"),
                Path("media"),
                case,
                Path(directory),
                1,
                "identity",
                interface="public",
            )
        self.assertEqual(result["status"], "pass")

    def test_public_receipt_binds_actual_source_image_and_reference(self):
        identity = dict(
            source_sha256="source", executable_sha256="unit", reference_sha256="oracle"
        )
        with self.assertRaises(ValueError):
            silk.verify_public_receipt(None, identity)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "receipt.json"
            receipt = dict(
                schema="moosicbox.opus.silk-unit-build.v1",
                profile="release",
                source_before_sha256="source",
                source_after_sha256="source",
                executable_sha256="unit",
                reference_sha256="oracle",
                command=["cargo", "test", "--release", "--no-run"],
                rustc_version="observed compiler",
            )
            path.write_text(json.dumps(receipt))
            self.assertEqual(silk.verify_public_receipt(path, identity), receipt)
            for key in (
                "source_before_sha256",
                "source_after_sha256",
                "executable_sha256",
                "reference_sha256",
                "profile",
            ):
                path.write_text(json.dumps(dict(receipt, **{key: "wrong"})))
                with self.subTest(key=key), self.assertRaises(ValueError):
                    silk.verify_public_receipt(path, identity)

    def test_enumeration_is_192_exact_filters(self):
        cases = silk.cases()
        self.assertEqual(len(cases), 192)
        self.assertEqual(len(set(cases)), 192)
        self.assertIn("speech/8000/1/10/16000/1", cases)
        self.assertIn("music/16000/2/60/64000/0.01", cases)
        self.assertEqual(
            {case.split("/")[3] for case in cases}, {"10", "20", "40", "60"}
        )

    def test_log_requires_one_test_and_one_internal_case(self):
        case = silk.cases()[0]
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "log"
            for interleaved in (False, True):
                path.write_text(passing(case, interleaved))
                self.assertTrue(silk.log_passes(path, case))
            for text in (
                passing(case).replace("running 1 test", "running 0 tests"),
                passing(case).replace("executed=1", "executed=192"),
                passing(case).replace("failures=0", "failures=1"),
                passing(case) + f"internal SILK CASE START {case}\n",
                passing(case).replace(case, "different-case"),
                passing(case).replace("1 passed", "0 passed"),
            ):
                path.write_text(text)
                self.assertFalse(silk.log_passes(path, case))

    def test_resume_pass_requires_hash_exit_identity_and_log(self):
        case = silk.cases()[0]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "case.log"
            path.write_text(passing(case))
            attempt = dict(
                case=case,
                log=path.name,
                status="pass",
                exit_code=0,
                timed_out=False,
                provenance_sha256="identity",
                log_sha256=silk.digest(path),
            )
            self.assertTrue(silk.reusable(attempt, case, root, "identity"))
            for key, value in (
                ("exit_code", 1),
                ("timed_out", True),
                ("provenance_sha256", "different"),
                ("log", "../case.log"),
            ):
                self.assertFalse(
                    silk.reusable(dict(attempt, **{key: value}), case, root, "identity")
                )
            path.write_text(passing(case) + "tampered")
            self.assertFalse(silk.reusable(attempt, case, root, "identity"))

    def test_timeout_kills_reaps_and_retains_attempt(self):
        class Child:
            def __init__(self):
                self.killed = False
                self.waits = 0
                self.returncode = None

            def wait(self, timeout):
                self.waits += 1
                if self.waits == 1:
                    raise subprocess.TimeoutExpired("fake", timeout)
                self.returncode = -9
                return -9

            def poll(self):
                return self.returncode

            def kill(self):
                self.killed = True

        child = Child()

        def terminate(process):
            process.kill()
            process.wait(timeout=5)

        with tempfile.TemporaryDirectory() as directory, patch.object(
            silk.subprocess, "Popen", return_value=child
        ), patch.object(silk, "terminate_tree", side_effect=terminate):
            result = silk.run_case(
                Path("unit"),
                Path("oracle"),
                Path("media"),
                silk.cases()[0],
                Path(directory),
                1,
                "identity",
            )
            self.assertTrue(child.killed)
            self.assertEqual(child.waits, 2)
            self.assertEqual(result["status"], "timeout")
            self.assertEqual(result["exit_code"], -9)
            self.assertTrue((Path(directory) / result["log"]).exists())

    def test_subset_resume_and_failures_continue_without_false_completion(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            image, reference, media = root / "unit", root / "oracle", root / "media"
            image.write_bytes(b"test")
            reference.write_bytes(b"oracle")
            media.mkdir()
            output = root / "evidence"
            identity = {"snapshot": "same", "executable": "test"}
            count = 0

            def attempt(
                executable,
                reference,
                media_directory,
                case,
                directory,
                timeout,
                provenance_id,
            ):
                nonlocal count
                count += 1
                path = directory / f"{count}.log"
                path.write_text(passing(case))
                return dict(
                    case=case,
                    status="pass",
                    exit_code=0,
                    timed_out=False,
                    log=path.name,
                    log_sha256=silk.digest(path),
                    provenance_sha256=provenance_id,
                )

            with patch.object(silk, "observation", return_value=identity), patch.object(
                silk, "run_case", side_effect=attempt
            ):
                report = silk.run(image, reference, media, output, silk.cases()[:2], 1)
                self.assertFalse(report["complete"])
                self.assertEqual(report["verified_passes"], 2)
                self.assertEqual(report["required_cases"], 192)
                silk.run(
                    image, reference, media, output, silk.cases()[:2], 1, resume=True
                )
                self.assertEqual(count, 2)
                (output / "1.log").write_text("tampered")
                silk.run(
                    image, reference, media, output, silk.cases()[:2], 1, resume=True
                )
                self.assertEqual(count, 3)
            with patch.object(
                silk, "observation", return_value={"snapshot": "changed"}
            ), patch.object(silk, "run_case") as run:
                with self.assertRaises(ValueError):
                    silk.run(
                        image,
                        reference,
                        media,
                        output,
                        silk.cases()[:2],
                        1,
                        resume=True,
                    )
                run.assert_not_called()
            persisted = json.loads((output / "report.json").read_text())
            self.assertEqual(persisted["verified_passes"], 2)

    def test_full_completion_requires_all_192_verified_logs(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            identity = "same"
            attempts = []
            for index, case in enumerate(silk.cases()):
                log = root / f"{index}.log"
                log.write_text(passing(case))
                attempts.append(
                    dict(
                        case=case,
                        log=log.name,
                        status="pass",
                        exit_code=0,
                        timed_out=False,
                        provenance_sha256=identity,
                        log_sha256=silk.digest(log),
                    )
                )
            report = dict(attempts=attempts, provenance_stable=True)
            silk.summarize(report, root, identity)
            self.assertTrue(report["complete"])
            self.assertEqual(report["verified_passes"], 192)
            report["provenance_stable"] = False
            silk.summarize(report, root, identity)
            self.assertFalse(report["complete"])
            report["provenance_stable"] = True
            (root / "191.log").write_text("changed")
            silk.summarize(report, root, identity)
            self.assertFalse(report["complete"])
            self.assertEqual(report["verified_passes"], 191)

    def test_failed_attempt_does_not_stop_remaining_cases(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            image = root / "unit"
            image.write_bytes(b"unit")
            media = root / "media"
            media.mkdir()
            seen = []

            def attempt(
                executable,
                reference,
                media_directory,
                case,
                directory,
                timeout,
                provenance_id,
            ):
                seen.append(case)
                return dict(
                    case=case,
                    status="timeout",
                    exit_code=-9,
                    timed_out=True,
                    log="unavailable",
                    provenance_sha256=provenance_id,
                )

            with patch.object(
                silk, "observation", return_value={"snapshot": "same"}
            ), patch.object(silk, "run_case", side_effect=attempt):
                report = silk.run(
                    image, image, media, root / "evidence", silk.cases()[:3], 1
                )
            self.assertEqual(seen, list(silk.cases()[:3]))
            self.assertEqual(len(report["attempts"]), 3)
            self.assertEqual(report["verified_passes"], 0)
            self.assertFalse(report["complete"])


if __name__ == "__main__":
    unittest.main()
