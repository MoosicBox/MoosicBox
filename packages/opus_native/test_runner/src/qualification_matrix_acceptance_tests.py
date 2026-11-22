"""Aggregator tests: synthetic evidence only; never codec acceptance evidence."""

import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import qualification_matrix_acceptance as acceptance
import qualification_performance_tests as performance_tests


class AcceptanceTests(unittest.TestCase):
    def test_pipeline_partial_diagnostic_rejected_even_with_pass_summary(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            log = root / "pipeline.log"
            log.write_text("PARTIAL_DIAGNOSTIC\ntest result: ok. 1 passed; 0 failed;\n")
            report = dict(schema="moosicbox.opus.pipeline-evidence.v1", source_sha256="source",
                          files_test_sources={}, suites=[dict(profile="debug", package="package",
                          features="feature", target="target", exit_code=0, timed_out=False,
                          log=log.name, log_sha256=acceptance.digest(log))])
            (root / "pipeline.json").write_text(json.dumps(report))
            with patch.object(acceptance.ci, "sources", return_value="source"), \
                 patch.object(acceptance.ci, "ROOT", root), \
                 patch.object(acceptance.ci, "PRODUCT_TESTS", (("package", "feature", "target"),)):
                with self.assertRaisesRegex(ValueError, "PARTIAL_DIAGNOSTIC"):
                    acceptance.verify_pipeline(root, dict(path="pipeline.json", sha256=acceptance.digest(root / "pipeline.json")))

    @staticmethod
    def core_report():
        rows = []
        for rate in (8000, 12000, 16000, 24000, 48000):
            for channels in (1, 2):
                for mode in (1000, 1001, 1002):
                    if mode == 1001 and rate < 24000:
                        continue
                    durations = (10, 20, 40, 60) if mode == 1000 else ((10, 20) if mode == 1001 else (2.5, 5, 10, 20))
                    for ms in durations:
                        rows.append(dict(rate=rate, channels=channels, mode=mode, duration_ms=ms,
                            status="pass", mismatched_samples=0, max_abs_error=0,
                            actual_modes=[mode], output_samples_per_channel=[int(rate*ms/1000)],
                            pcm_sha256="a"*64))
        return dict(schema="moosicbox.opus.native-qualification.v1", runtime_source_sha256="source",
            accepted=True, failures=0, decoder_matrix=rows,
            long_stream=dict(frames=3000, duration_seconds=60, loss_frames=93,
                             status="pass", replay_exact=True, sha256="b"*64),
            adversarial=dict(cases=128, seed=2457, accepted=80, rejected=48, crashes=0,
                             timeouts=0, status="pass", sha256="c"*64))

    def test_core_denominators_reconstructed_not_trusted(self):
        report = self.core_report()
        self.assertEqual(len(report["decoder_matrix"]), 88)
        with patch.object(acceptance.matrix, "native_source_identity", return_value="source"):
            acceptance.verify_core_matrix(report)
            for mutate in (
                lambda r: r["decoder_matrix"].pop(),
                lambda r: r["decoder_matrix"].append(r["decoder_matrix"][0]),
                lambda r: r["decoder_matrix"][0].update(actual_modes=[]),
                lambda r: r["decoder_matrix"][0].update(output_samples_per_channel=[81]),
                lambda r: r["decoder_matrix"][0].update(mismatched_samples=1),
                lambda r: r["long_stream"].update(frames=100),
                lambda r: r["long_stream"].update(loss_frames=0),
                lambda r: r["long_stream"].update(replay_exact=False),
                lambda r: r["adversarial"].update(cases=127),
                lambda r: r["adversarial"].update(rejected=47),
                lambda r: r["adversarial"].update(crashes=1, rejected=47),
                lambda r: r.update(runtime_source_sha256="stale"),
            ):
                changed = copy.deepcopy(report)
                mutate(changed)
                with self.assertRaises(ValueError):
                    acceptance.verify_core_matrix(changed)

    def test_core_artifact_hash_and_oracle_binding(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            quality = self.core_report()
            quality.update(native_sha256="native", reference_sha256="reference")
            (root / "quality.json").write_text(json.dumps(quality))
            report = dict(mode="quality", quality_shard="core", platform="linux-x64", profile="release",
                          native_sha256="native", reference_sha256="reference",
                          quality_sha256=acceptance.digest(root / "quality.json"))
            (root / "ci.json").write_text(json.dumps(report))
            with patch.object(acceptance.ci, "PLATFORMS", {"linux-x64": ()}), \
                 patch.object(acceptance.matrix, "native_source_identity", return_value="source"), \
                 patch.object(acceptance, "verify_combined_scoreboard") as combined:
                acceptance.verify_core_evidence(root)
                combined.assert_called_once_with(root)
                report["native_sha256"] = "other"
                (root / "ci.json").write_text(json.dumps(report))
                with self.assertRaisesRegex(ValueError, "oracle image"):
                    acceptance.verify_core_evidence(root)
                report["quality_sha256"] = "wrong"
                (root / "ci.json").write_text(json.dumps(report))
                with self.assertRaisesRegex(ValueError, "artifact changed"):
                    acceptance.verify_core_evidence(root)

    def test_missing_core_platforms_fail_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(ValueError, "missing core platforms"):
                acceptance.verify_core_evidence(Path(directory))

    def test_combined_requires_actual_complete_output_and_missing_receipt_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            command = dict(argv=["/original/moosicbox_opus_native_test_runner"], exit_code=0)
            commands = root / "commands.json"
            commands.write_text(json.dumps([command]))
            inventory = json.loads((acceptance.ci.ROOT / "packages/opus_native/test_vectors/frozen/corpus/inventory.json").read_text())
            header = dict(type="run", schema=3, mode="strict", build_profile="release_no_debug_assertions",
                          comparison_contract="raw_exact_pcm_packet_identity_v2", features=dict(celt=True, silk=True, hybrid=True))
            cases = [dict(type="case", id=row["id"], status="pass", exercised=True) for row in inventory["cases"]]
            summary = dict(type="summary", scope="all_cases",
                           success=True, selected_cases_pass=True, whole_corpus_pass=True)
            summary["statuses"] = {"pass":len(cases)}
            output = root / "command-0.stdout"
            def write(records):
                output.write_text("\n".join(json.dumps(r) for r in records))
            write([header, *cases, summary])
            with self.assertRaisesRegex(ValueError, "missing combined scoreboard compiled-image/source receipt"):
                acceptance.verify_combined_scoreboard(root)
            for records in ([header, *cases[:-1], summary],
                            [header, *cases, dict(summary, scope="focused")],
                            [dict(header, mode="diagnostic"), *cases, summary],
                            [header, summary]):
                write(records)
                with self.assertRaises(ValueError):
                    acceptance.verify_combined_scoreboard(root)
            command["timed_out"] = True
            commands.write_text(json.dumps([command]))
            with self.assertRaisesRegex(ValueError, "timed out"):
                acceptance.verify_combined_scoreboard(root)
            commands.write_text("[]")
            with self.assertRaisesRegex(ValueError, "missing unique"):
                acceptance.verify_combined_scoreboard(root)

    def test_full_suite_accepts_all_discovered_tests_without_per_test_processes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            log = root / "full.log"
            text = ("running 3 tests\ntest first ... ok\ntest tail ... ok\n"
                    "test diagnostic ... ignored, explicit oracle\n"
                    "test result: ok. 2 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 1s\n")
            log.write_text(text)
            suite = dict(log=log.name, log_sha256=acceptance.digest(log), exit_code=0, timed_out=False)
            self.assertEqual(acceptance.verify_full_suite(root, suite, ["first", "tail"], ["diagnostic"]), {"first", "tail"})
            for changed in (text.replace("test tail ... ok\n", ""),
                            text.replace("0 filtered out", "1 filtered out"),
                            text + "test tail ... ok\n"):
                log.write_text(changed)
                suite["log_sha256"] = acceptance.digest(log)
                with self.assertRaises(ValueError):
                    acceptance.verify_full_suite(root, suite, ["first", "tail"], ["diagnostic"])
            suite["timed_out"] = True
            with self.assertRaises(ValueError):
                acceptance.verify_full_suite(root, suite, ["first", "tail"], ["diagnostic"])

    def test_short_measurement_is_not_final_performance(self):
        report = performance_tests.PerformanceTests().valid_report()
        report.update(performance_accepted=True, peak_memory_complete=True,
                      build_receipt=dict(source_before_sha256="same", source_after_sha256="same",
                                         executable_sha256="image"), executable_sha256="image")
        for row in report["cases"]:
            row["peak_rss_bytes"] = 1000
        with self.assertRaisesRegex(ValueError, "500"):
            acceptance.verify_performance(report, "same")
        report["frames"] = 500
        for row in report["cases"]:
            row["frames"] = 500
            row["audio_ns"] = 10_000_000_000
            for phase in ("encode", "decode"):
                row[phase].update(total_ns=10_000_000_000, median_ns=20_000_000,
                                  p95_ns=20_000_000, max_ns=20_000_000)
        self.assertTrue(acceptance.verify_performance(report, "same")["accepted"])
        report["cases"][0]["peak_rss_bytes"] = None
        with self.assertRaisesRegex(ValueError, "RSS"):
            acceptance.verify_performance(report, "same")
        report["platform"] = dict(system="Windows")
        report["peak_memory_complete"] = False
        for row in report["cases"]:
            row.update(peak_rss_bytes=None, peak_rss_method="unavailable on this platform")
        self.assertTrue(acceptance.verify_performance(report, "same")["accepted"])
        report["cases"][0]["peak_rss_method"] = "not measured without reason"
        with self.assertRaisesRegex(ValueError, "limitation"):
            acceptance.verify_performance(report, "same")

    def test_missing_and_stale_bundle_fail_before_acceptance(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "bundle.json"
            path.write_text(json.dumps(dict(schema="moosicbox.opus.finish-bundle.v1", runtime_source_sha256="stale")))
            with self.assertRaises(ValueError):
                acceptance.aggregate(path)
            with patch.object(acceptance.matrix, "native_source_identity", return_value="stale"), \
                 patch.object(acceptance.ci, "acceptance_inventory", return_value={}):
                with self.assertRaises(KeyError):
                    acceptance.aggregate(path)

    def test_internal_and_incomplete_silk_never_satisfy_public(self):
        with self.assertRaisesRegex(ValueError, "internal"):
            acceptance.verify_public_silk(Path("report.json"), dict(schema=acceptance.silk.SCHEMA, interface="internal"))
        identity = dict(source_sha256="current", executable_sha256="unit", reference_sha256="oracle",
                        build_receipt=dict(schema="moosicbox.opus.silk-unit-build.v1", profile="release",
                                           source_before_sha256="current", source_after_sha256="current",
                                           executable_sha256="unit", reference_sha256="oracle",
                                           command=["actual-build"], rustc_version="compiler"))
        report = dict(schema=acceptance.silk.PUBLIC_SCHEMA, interface="public", identity=identity,
                      provenance_sha256=acceptance.silk.provenance_hash(identity), attempts=[], provenance_stable=True)
        with patch.object(acceptance.silk, "source_identity", return_value="current"):
            with self.assertRaisesRegex(ValueError, "192"):
                acceptance.verify_public_silk(Path("report.json"), report)

    def test_artifact_escape_and_mutation_reject(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "report.json"
            path.write_text('{}')
            with self.assertRaises(ValueError):
                acceptance.read_artifact(root, dict(path="report.json", sha256="wrong"))
            with self.assertRaises(ValueError):
                acceptance.local_path(root, "..")

    def test_unit_timeout_or_missing_tail_is_not_partial_success(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            def artifact(name, value):
                path = root / name
                path.write_text(json.dumps(value))
                return dict(path=name, sha256=acceptance.digest(path))
            (root / "list.log").write_text("first: test\ntail: test\nignored: test\n")
            (root / "ignored.log").write_text("ignored: test\n")
            (root / "unit").write_bytes(b"image")
            inventory = artifact("inventory.json", dict(ordinary_tests=["first", "tail"], ignored_tests=["ignored"],
                list_log="list.log", list_sha256=acceptance.digest(root / "list.log"),
                ignored_list_log="ignored.log", ignored_list_sha256=acceptance.digest(root / "ignored.log")))
            profile = dict(inventory=inventory, executable_file="unit", executable_sha256=acceptance.digest(root / "unit"),
                           build_receipt=dict(source_before_sha256="source", source_after_sha256="source",
                                              executable_sha256=acceptance.digest(root / "unit"), profile="debug", command=["actual-build"]),
                           cases=[])
            report = dict(schema="moosicbox.opus.unit-shards.v1", source_sha256="source",
                          test_source_manifest=acceptance.ci.sources(),
                          profiles=dict(debug=profile, release=copy.deepcopy(profile)))
            entry = artifact("units.json", report)
            with self.assertRaisesRegex(ValueError, "missing ordinary"):
                acceptance.verify_unit_shards(root, entry, "source")
            profile["cases"] = [dict(name="tail", exit_code=-9, timed_out=True)]
            entry = artifact("units.json", report)
            with self.assertRaisesRegex(ValueError, "timed-out"):
                acceptance.verify_unit_shards(root, entry, "source")


if __name__ == "__main__":
    unittest.main()
