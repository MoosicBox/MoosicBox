"""Portable selection tests: no Cargo, codec executable, network or baseline writes."""

import argparse
from pathlib import Path, PurePosixPath, PureWindowsPath
import tempfile
import unittest
from unittest.mock import Mock, patch
import wave

import qualification_ci as ci


def named_pass(execution):
    def run(argv, **kwargs):
        name = argv[argv.index("--") - 1]
        return f"test {name} ... ok\n" + execution.run.return_value
    execution.run.side_effect = run


class SelectionTests(unittest.TestCase):
    def test_source_keys_are_portable_without_changing_case(self):
        for root in (PureWindowsPath('C:/work'), PurePosixPath('/work')):
            with self.subTest(root=root):
                self.assertEqual(ci.source_key(root / 'packages/Opus/File.rs', root),
                                 'packages/Opus/File.rs')
    def test_shards_cover_required_inventory_once(self):
        shards = [ci.selected_live_tests(shard) for shard in ci.LIVE_SHARDS]
        flat = [tuple(row) for shard in shards for row in shard]
        self.assertEqual(len(flat), len(set(flat)))
        required = {
            row
            for row in ci.acceptance_inventory()
            if row[2] not in (ci.PUBLIC_MEDIA_TEST, ci.LAYOUT_TEST, ci.HYBRID_TEST)
        }
        self.assertEqual(set(flat), required)
        cases = [case for shard in ci.PUBLIC_SHARDS for case in ci.public_cases(shard)]
        self.assertEqual(len(cases), 192)
        self.assertEqual(len(set(cases)), 192)
        self.assertTrue(
            all(len(ci.public_cases(shard)) == 8 for shard in ci.PUBLIC_SHARDS)
        )
        self.assertLessEqual(max(map(len, shards)), 8)
        with self.assertRaises(ValueError):
            ci.selected_live_tests("live-99")

    def test_compare_requires_every_coherent_shard(self):
        import json

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for pid in ci.PLATFORMS:
                for mode, profiles, shards in (
                    ("native", ("debug", "release"), (None,)),
                    ("quality", ("release",), ci.QUALITY_SHARDS),
                ):
                    for profile in profiles:
                        for shard in shards:
                            dest = root / f"{pid}-{profile}-{mode}-{shard}"
                            dest.mkdir()
                            row = dict(
                                schema="moosicbox.opus.ci.v2",
                                platform=pid,
                                profile=profile,
                                mode=mode,
                                quality_shard=shard,
                                accepted=True,
                                revision="synthetic",
                                source_sha256={"test": "source"},
                                rustc=f"release: test\nhost: {pid}",
                            )
                            if mode == "native":
                                (dest / "data").mkdir()
                                (dest / "data/test.pcm").write_bytes(b"\\x01\\x00")
                                (dest / "probe").write_bytes(pid.encode())
                                row["products"] = [dict(package=p, features=f, target=t, passed=1)
                                                   for p, f, t in ci.PRODUCT_TESTS]
                                row.update(
                                    executable_file="probe",
                                    executable_sha256=ci.digest(dest / "probe"),
                                    fingerprints=ci.probe_files(dest / "data"),
                                )
                            elif shard == "core":
                                ci.write_json(
                                    dest / "quality.json",
                                    {"accepted": True, "failures": 0},
                                )
                                row["quality_sha256"] = ci.digest(dest / "quality.json")
                            else:
                                ci.write_json(
                                    dest / "media-transform.json", {"synthetic": True}
                                )
                                row.update(
                                    required_tests=ci.selected_live_tests(shard),
                                    media_transform_sha256=ci.digest(
                                        dest / "media-transform.json"
                                    ),
                                )
                                if shard in ci.HYBRID_SHARDS:
                                    row["hybrid_cells"] = ci.hybrid_cells(shard)
                                if shard in ci.LAYOUT_SHARDS:
                                    row["layout_cell"] = ci.layout_cell(shard)
                                if shard in ci.PUBLIC_SHARDS:
                                    row["public_cases"] = ci.public_cases(shard)
                            ci.write_json(dest / "ci.json", row)
            with patch("builtins.print"):
                ci.compare(argparse.Namespace(root=root))
            summary = json.loads((root / "platform-acceptance.json").read_text())
            self.assertEqual(len(summary["executions"]), 496)
            public_report = root / "linux-x64-release-quality-public-0/ci.json"
            public_original = public_report.read_text()
            forged_public = json.loads(public_original)
            forged_public["public_cases"] = forged_public["public_cases"][:-1]
            ci.write_json(public_report, forged_public)
            with self.assertRaisesRegex(RuntimeError, "public 192-case shard coverage"):
                ci.compare(argparse.Namespace(root=root))
            public_report.write_text(public_original)
            report = root / "windows-x64-release-quality-live-7/ci.json"
            original = report.read_text()
            report.unlink()
            with self.assertRaisesRegex(RuntimeError, "missing real executions"):
                ci.compare(argparse.Namespace(root=root))
            report.write_text(original)
            forged = json.loads(original)
            forged["required_tests"] = []
            ci.write_json(report, forged)
            with self.assertRaisesRegex(RuntimeError, "coverage changed"):
                ci.compare(argparse.Namespace(root=root))
            forged = json.loads(original)
            forged["source_sha256"] = {"test": "different"}
            ci.write_json(report, forged)
            with self.assertRaisesRegex(RuntimeError, "common source"):
                ci.compare(argparse.Namespace(root=root))

    def test_public_shard_exact_filters_and_environment_mask(self):
        with tempfile.TemporaryDirectory() as directory:
            execution = Mock(output=Path(directory))
            named_pass(execution)
            execution.run.return_value = (
                "test result: ok. 1 passed; 0 failed; 0 ignored;\n"
            )
            env = {
                "MOOSICBOX_OPUS_SILK_CASE": "inherited-wrong-case",
                "MOOSICBOX_OPUS_QUALIFICATION_MEDIA": "verified-media",
            }
            ci.live_acceptance(execution, "release", env, "public-0")
            self.assertEqual(execution.run.call_count, 8)
            observed = []
            for call in execution.run.call_args_list:
                self.assertIn(ci.PUBLIC_MEDIA_TEST, call.args[0])
                self.assertIn("--exact", call.args[0])
                self.assertEqual(
                    call.kwargs["env"]["MOOSICBOX_OPUS_QUALIFICATION_MEDIA"],
                    "verified-media",
                )
                observed.append(call.kwargs["env"]["MOOSICBOX_OPUS_SILK_CASE"])
            self.assertEqual(observed, list(ci.public_cases("public-0")))
            execution.reset_mock()
            ci.live_acceptance(execution, "release", env, "live-0")
            for call in execution.run.call_args_list:
                self.assertNotIn("MOOSICBOX_OPUS_SILK_CASE", call.kwargs["env"])

    def test_layout_cells_are_exact_complete_and_proved(self):
        cells = [ci.layout_cell(shard) for shard in ci.LAYOUT_SHARDS]
        self.assertEqual(len(set(cells)), 40)
        self.assertEqual(
            set(cells),
            {
                f"{r}:{c}:{d}:{s}"
                for r in (24000, 48000)
                for c in (1, 2)
                for d in (10, 20)
                for s in range(5)
            },
        )
        with tempfile.TemporaryDirectory() as directory:
            execution = Mock(output=Path(directory))
            named_pass(execution)
            execution.run.return_value = (
                "test result: ok. 1 passed; 0 failed; 0 ignored;\n"
                "layout qualification executed=1 filtered=true\n"
            )
            ci.live_acceptance(
                execution,
                "release",
                {"MOOSICBOX_OPUS_LAYOUT_CELL": "wrong"},
                "layout-0",
            )
            self.assertEqual(execution.run.call_count, 1)
            call = execution.run.call_args
            self.assertIn(ci.LAYOUT_TEST, call.args[0])
            self.assertEqual(call.kwargs["env"]["MOOSICBOX_OPUS_LAYOUT_CELL"], cells[0])
            self.assertIn("--show-output", call.args[0])
            execution.run.return_value = (
                "test result: ok. 1 passed; 0 failed; 0 ignored;"
            )
            with self.assertRaisesRegex(RuntimeError, "exactly one filtered cell"):
                ci.live_acceptance(execution, "release", {}, "layout-0")

    def test_product_suites_native_only_and_zero_tests_fail(self):
        execution = Mock(env={})
        execution.run.return_value = "test result: ok. 3 passed; 0 failed; 0 ignored;"
        results = ci.product_tests(execution, "release")
        self.assertEqual(len(results), len(ci.PRODUCT_TESTS))
        for call, (package, features, target) in zip(execution.run.call_args_list, ci.PRODUCT_TESTS):
            argv = call.args[0]
            self.assertEqual(argv[argv.index("-p")+1], package)
            self.assertEqual(argv[argv.index("--features")+1], features)
            self.assertEqual(argv[argv.index("--test")+1], target)
            self.assertIn("--no-default-features", argv)
            self.assertIn("--release", argv)
            self.assertNotIn("--skip", argv)
        execution.run.return_value = "test result: ok. 0 passed; 0 failed;"
        with self.assertRaisesRegex(RuntimeError, "did not execute"):
            ci.product_tests(execution, "debug")

    def test_required_product_oracle_tests_are_exact_and_fail_closed(self):
        execution = Mock(env={})
        named_pass(execution)
        execution.run.return_value = "test result: ok. 1 passed; 0 failed; 0 ignored;"
        ci.product_live_tests(execution, Path("frozen-reference"))
        self.assertEqual(execution.run.call_count, 2)
        for call in execution.run.call_args_list:
            self.assertIn("--exact", call.args[0])
            self.assertIn("--ignored", call.args[0])
            self.assertEqual(call.kwargs["env"]["MOOSICBOX_OPUS_REFERENCE_ORACLE"], "frozen-reference")
        execution.run.return_value = "test result: ok. 0 passed; 0 failed;"
        with self.assertRaisesRegex(RuntimeError, "did not execute once"):
            ci.product_live_tests(execution, Path("frozen-reference"))

    def test_full_acceptance_clears_diagnostics_and_rejects_partial(self):
        dirty = {"MOOSICBOX_OPUS_PRODUCT_CELL":"0", "MOOSICBOX_OPUS_BOUNDED_PREFIX":"1",
                 "MOOSICBOX_OPUS_PUBLIC_CELL":"x", "retained":"yes"}
        with tempfile.TemporaryDirectory() as directory:
            with patch.dict(ci.os.environ, dirty):
                self.assertNotIn("MOOSICBOX_OPUS_BOUNDED_PREFIX", ci.Execution(Path(directory)).env)
        execution = Mock(env=dirty)
        execution.run.return_value = "test result: ok. 1 passed; 0 failed; PARTIAL_DIAGNOSTIC"
        with self.assertRaisesRegex(RuntimeError,"partial diagnostic"):
            ci.product_tests(execution,"release")
        self.assertNotIn("MOOSICBOX_OPUS_PRODUCT_CELL", execution.run.call_args.kwargs["env"])
        self.assertEqual(execution.run.call_args.kwargs["env"]["retained"], "yes")
        with self.assertRaisesRegex(RuntimeError,"partial diagnostic"):
            ci.product_live_tests(execution,Path("reference"))
        self.assertIn("bounded_reviewed_music_layout",ci.DEVELOPMENT_ONLY)
        self.assertIn("packages/opus_native/src/encoder/silk/bounded.rs",ci.sources())

    def test_source_identity_includes_scored_and_product_tests(self):
        source = ci.sources()
        self.assertIn("packages/opus_native/src/encoder/silk_scored.rs", source)
        self.assertIn("packages/audio_encoder/tests/opus_public_acceptance.rs", source)
        self.assertIn("packages/player/tests/opus_encode_modes.rs", source)

    def test_public_hybrid_matrix_exact_shards_and_proof(self):
        import itertools
        cells = [cell for shard in ci.HYBRID_SHARDS for cell in ci.hybrid_cells(shard)]
        expected = {':'.join(map(str, row)) for row in itertools.product(
            (24000,48000),(1,2),(10,20),(32000,64000,128000),(0,1),(0,1,2),(0,1),range(6))}
        self.assertEqual(len(cells),1728)
        self.assertEqual(set(cells),expected)
        self.assertEqual(len(set(cells)),len(cells))
        self.assertTrue(all(len(ci.hybrid_cells(s))==36 for s in ci.HYBRID_SHARDS))
        self.assertLessEqual(len(ci.HYBRID_SHARDS)*len(ci.PLATFORMS),256)
        with tempfile.TemporaryDirectory() as directory:
            execution=Mock(output=Path(directory))
            named_pass(execution)
            execution.run.return_value=('test result: ok. 1 passed; 0 failed; 0 ignored;\n'
                                        'public coverage executed=1 failed=0 filtered=true\n')
            ci.live_acceptance(execution,'release',{'MOOSICBOX_OPUS_PUBLIC_CELL':'wrong'},'hybrid-0')
            self.assertEqual(execution.run.call_count,36)
            observed=[call.kwargs['env']['MOOSICBOX_OPUS_PUBLIC_CELL'] for call in execution.run.call_args_list]
            self.assertEqual(observed,ci.hybrid_cells('hybrid-0'))
            execution.run.return_value='test result: ok. 1 passed; 0 failed; 0 ignored;'
            with self.assertRaisesRegex(RuntimeError,'exactly one successful'):
                ci.live_acceptance(execution,'release',{},'hybrid-0')

    def test_current_ignored_inventory_is_explicitly_classified(self):
        required = ci.acceptance_inventory()
        self.assertGreater(len(required), 40)
        self.assertIn(
            "encoder::silk::public::public_floor_synthetic_matrix",
            [r[2] for r in required],
        )
        self.assertIn(
            "encoder::silk::public::public_floor_real_speech_matrix",
            [r[2] for r in required],
        )
        self.assertNotIn(
            "internal_practical_floor_media_matrix", [r[2] for r in required]
        )
        self.assertIn("internal_practical_floor_media_matrix", ci.DEVELOPMENT_ONLY)
        self.assertIn(
            "encoder::adaptive::tests::reference_public_practical_rd_quality",
            [r[2] for r in required],
        )
        self.assertNotIn("reference_public_practical_rd_quality", ci.DEVELOPMENT_ONLY)
        self.assertIn(
            "reference_unredundant_closed_loop_energy_proof", ci.DEVELOPMENT_ONLY
        )
        self.assertNotIn(
            "reference_unredundant_closed_loop_energy_proof",
            [r[2].rsplit("::", 1)[-1] for r in required],
        )
        self.assertIn(
            "encoder::adaptive::tests::reference_practical_unredundant_transition_quality",
            [r[2] for r in required],
        )
        self.assertIn(
            "encoder::adaptive::tests::reference_practical_auto_steady_quality",
            [r[2] for r in required],
        )

    def test_required_exact_tests_execute_with_oracle_and_media(self):
        with tempfile.TemporaryDirectory() as directory:
            execution = Mock(output=Path(directory))
            named_pass(execution)
            execution.run.return_value = "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;\n"
            env = {
                "MOOSICBOX_OPUS_REFERENCE_ORACLE": "explicit-oracle",
                "MOOSICBOX_OPUS_HYBRID_REAL_PCM": "reviewed.pcm",
                "MOOSICBOX_OPUS_SPEECH_WAV": "reviewed.wav",
            }
            ci.live_acceptance(execution, "release", env, "live-0")
            required = ci.selected_live_tests("live-0")
            self.assertEqual(len(execution.run.call_args_list), len(required))
            for call, (_, target, name) in zip(execution.run.call_args_list, required):
                argv = call.args[0]
                self.assertIn(name, argv)
                self.assertIn("--exact", argv)
                self.assertIn("--ignored", argv)
                self.assertIn("--release", argv)
                self.assertIn("--offline", argv)
                self.assertEqual(call.kwargs["env"], env)
                if target:
                    self.assertEqual(argv[argv.index("--test") + 1], target)
                self.assertNotIn("internal_practical_floor_media_matrix", argv)
            self.assertTrue((Path(directory) / "acceptance-inventory.json").exists())

    def test_generated_decoder_families_require_complete_unique_list(self):
        family = "supported_output_fixtures_match_reference"
        row = ("--test", "decoder_completion", family)
        names = [family + "::case_" + str(i) for i in range(4)]
        execution = Mock()
        for listing in ([], names[:-1], names[:-1] + names[:1], names + [family + "::extra"]):
            execution.run.return_value = "\n".join(n + ": test" for n in listing)
            with self.assertRaisesRegex(RuntimeError, "decoder fixtures"):
                ci.expand_live_inventory(execution, [row], "release", {})
        execution.run.return_value = "\n".join(n + ": test" for n in reversed(names))
        self.assertEqual(ci.expand_live_inventory(execution, [row], "release", {}),
                         [(row[0], row[1], n) for n in names])

    def test_summary_cannot_substitute_for_named_pass(self):
        summary = "test result: ok. 1 passed; 0 failed; 0 ignored;"
        for output in (summary, "test wrong ... ok\n" + summary,
                       "test right ... ok\n" + summary + "\ntest result: ok. 0 passed;"):
            with self.assertRaisesRegex(RuntimeError, "did not execute once"):
                ci.require_exact_pass(output, "right")

    def test_zero_selected_test_is_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            execution = Mock(output=Path(directory))
            execution.run.return_value = (
                "test result: ok. 0 passed; 0 failed; 0 ignored;\n"
            )
            with self.assertRaisesRegex(RuntimeError, "did not execute once"):
                ci.live_acceptance(execution, "release", {}, "live-0")
            self.assertEqual(execution.run.call_count, 1)

    def test_required_failure_stops_instead_of_skipping(self):
        with tempfile.TemporaryDirectory() as directory:
            execution = Mock(output=Path(directory))
            execution.run.side_effect = RuntimeError("required test failed")
            with self.assertRaisesRegex(RuntimeError, "required test failed"):
                ci.live_acceptance(execution, "release", {}, "live-0")
            self.assertEqual(execution.run.call_count, 1)

    def test_native_regression_remains_unfiltered(self):
        execution = Mock()
        ci.native_tests(execution, "release")
        argv = execution.run.call_args.args[0]
        self.assertIn("--lib", argv)
        self.assertIn("--tests", argv)
        self.assertNotIn("--skip", argv)
        self.assertNotIn("--ignored", argv)
        self.assertNotIn("--exclude", argv)

    def test_pcm_transform_is_exact_and_identity_recorded(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            music, speech = root / "music.wav", root / "speech.wav"
            pcm = b"\x01\x00\xff\x7f" * (32 * 960)
            with wave.open(str(music), "wb") as output:
                output.setnchannels(2)
                output.setsampwidth(2)
                output.setframerate(48000)
                output.writeframes(pcm)
            speech.write_bytes(b"mock-reviewed-speech")
            manifest = {"synthetic_test_fixture": True}
            with patch(
                "qualification_inputs.verify_inputs",
                return_value=(manifest, {"music": music, "speech": speech}),
            ):
                env = ci.media_environment(
                    root, root, {"retained": "yes"}, Path("oracle")
                )
            self.assertEqual(
                Path(env["MOOSICBOX_OPUS_HYBRID_REAL_PCM"]).read_bytes(), pcm
            )
            self.assertEqual(env["MOOSICBOX_OPUS_SPEECH_WAV"], str(speech))
            self.assertEqual(env["retained"], "yes")
            import json

            receipt = json.loads((root / "media-transform.json").read_text())
            self.assertEqual(
                receipt["output_sha256"],
                ci.digest(Path(env["MOOSICBOX_OPUS_HYBRID_REAL_PCM"])),
            )
            self.assertEqual(receipt["output_bytes"], len(pcm))

    def test_unreviewed_media_rejected_before_transform(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with patch(
                "qualification_inputs.verify_inputs",
                side_effect=ValueError("checksum mismatch"),
            ):
                with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                    ci.media_environment(root, root, {}, Path("oracle"))
            self.assertFalse((root / "media-transform.json").exists())

    def test_strict_matrix_required_argv_and_acceptance(self):
        import json

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            execution = ci.Execution(root)

            def fake_child(argv, **kwargs):
                self.assertIn("--media-directory", argv)
                self.assertIn("--build-receipt", argv)
                self.assertNotIn("--diagnostic", argv)
                self.assertIn(str(ci.HERE / "qualification_matrix.py"), argv)
                self.assertEqual(kwargs["timeout"], 1800)
                (root / "quality.json").write_text(
                    json.dumps({"accepted": True, "failures": 0})
                )
                return Mock(returncode=0)

            with patch.object(ci.subprocess, "run", side_effect=fake_child):
                ci.strict_matrix(execution, Path("native"), Path("reference"))
            (root / "quality.json").write_text(
                json.dumps({"accepted": False, "failures": 1})
            )
            with patch.object(execution, "run"):
                with self.assertRaisesRegex(RuntimeError, "not accepted"):
                    ci.strict_matrix(execution, Path("native"), Path("reference"))

    def test_real_subprocess_arguments_mocked(self):
        with tempfile.TemporaryDirectory() as directory:
            execution = ci.Execution(Path(directory))
            with patch.object(ci.subprocess, "run") as child:
                child.return_value.returncode = 0
                ci.native_tests(execution, "release")
                self.assertEqual(child.call_args.kwargs["timeout"], 1800)
                self.assertEqual(child.call_args.kwargs["env"]["CARGO_BUILD_JOBS"], "2")
                self.assertEqual(
                    child.call_args.kwargs["env"]["CARGO_NET_OFFLINE"], "true"
                )


if __name__ == "__main__":
    unittest.main()
