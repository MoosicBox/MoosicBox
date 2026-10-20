"""Tiny offline analyzer regressions: no executable, Cargo, PTY or HTTP calls."""

import base64
import json
import tempfile
from pathlib import Path
import unittest

from bmux_perf_analyze import (
    KittyAnalyzer,
    analyze_capture,
    capture_provenance,
    compare_captures,
    phase_metrics,
    duration_histogram_metrics,
    fixture_metrics,
    replay_profile,
    stage_metrics,
    work_metrics,
)


def apc(header, data=b""):
    return b"\x1b_G" + header + b";" + base64.b64encode(data) + b"\x1b\\"


class KittyTests(unittest.TestCase):
    def analyze(self, data, block=1):
        analyzer = KittyAnalyzer()
        for begin in range(0, len(data), block):
            analyzer.feed(data[begin : begin + block])
        return analyzer.finish()

    def test_chunk_count_is_not_upload_count(self):
        data = (
            apc(b"a=t,i=1,f=100,s=4,v=4,m=1", b"abc")
            + apc(b"m=0", b"def")
            + apc(b"a=p,i=1")
            + apc(b"a=d,i=1")
        )
        result = self.analyze(data)
        self.assertEqual(result["transmit_chunks"], 2)
        self.assertEqual(result["completed_uploads"], 1)
        self.assertEqual(result["hashed_uploads"], 1)
        self.assertEqual(result["placements"], 1)
        self.assertEqual(result["deletions"], 1)
        self.assertFalse(result["incomplete_command"])
        self.assertFalse(result["incomplete_upload"])
        self.assertEqual(result, self.analyze(data, 65536))

    def test_repeat_hash_ignores_id_and_chunk_boundaries_not_geometry(self):
        data = (
            apc(b"a=t,i=1,f=100,s=4,v=4,m=1", b"abc")
            + apc(b"a=t,i=1,m=0", b"def")
            + apc(b"a=t,i=9,f=100,s=4,v=4", b"abcdef")
            + apc(b"a=t,i=10,f=100,s=5,v=4", b"abcdef")
        )
        result = self.analyze(data)
        self.assertEqual(result["completed_uploads"], 3)
        self.assertEqual(result["unique_source_hashes"], 2)
        self.assertEqual(result["repeat_uploads"], 1)
        self.assertEqual(result["repeat_encoded_payload_bytes"], 8)
        self.assertEqual(result["repeat_decoded_transport_bytes"], 6)

    def test_truncated_command_and_upload_are_not_complete(self):
        result = self.analyze(apc(b"a=t,i=1,m=1", b"abc") + b"\x1b_Gm=0;YWJj")
        self.assertEqual(result["completed_uploads"], 0)
        self.assertTrue(result["incomplete_command"])
        self.assertTrue(result["incomplete_upload"])

    def test_bad_base64_and_conflicting_id_not_hashed(self):
        data = b"\x1b_Ga=t,i=1,m=1;!!!!\x1b\\" + apc(b"a=t,i=2,m=0", b"abc")
        result = self.analyze(data)
        self.assertEqual(result["completed_uploads"], 1)
        self.assertEqual(result["invalid_base64_chunks"], 1)
        self.assertEqual(result["conflicting_chunk_ids"], 1)
        self.assertEqual(result["hashed_uploads"], 0)

    def test_oversized_command_resynchronizes(self):
        analyzer = KittyAnalyzer(max_command_bytes=40)
        for data in (
            b"\x1b_G" + b"x" * 100,
            b"x" * 100,
            b"\x1b\\" + apc(b"a=t,i=2", b"abc"),
        ):
            analyzer.feed(data)
        result = analyzer.finish()
        self.assertEqual(result["oversized_commands"], 1)
        self.assertEqual(result["completed_uploads"], 1)
        self.assertEqual(result["hashed_uploads"], 1)

    def test_default_action_and_single_read_oversize(self):
        result = self.analyze(apc(b"i=1,f=100", b"abc"))
        self.assertEqual(result["completed_uploads"], 1)
        analyzer = KittyAnalyzer(max_command_bytes=40)
        analyzer.feed(b"\x1b_G" + b"x" * 100 + b"\x1b\\" + apc(b"a=p,i=1"))
        self.assertEqual(analyzer.finish()["oversized_commands"], 1)
        self.assertEqual(analyzer.finish()["placements"], 1)

    def test_malformed_header_is_reported(self):
        result = self.analyze(b"\x1b_Ga=t,bad;YWJj\x1b\\")
        self.assertEqual(result["malformed_commands"], 1)
        self.assertEqual(result["completed_uploads"], 0)

    def test_non_direct_upload_not_hashed(self):
        result = self.analyze(apc(b"a=t,t=f,i=1", b"/tmp/source.png"))
        self.assertEqual(result["completed_uploads"], 1)
        self.assertEqual(result["unsupported_uploads"], 1)
        self.assertEqual(result["hashed_uploads"], 0)


class ReplayTests(unittest.TestCase):
    def test_identical_fixed_bytes_replayed_separately(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "terminal.bin"
            path.write_bytes(b"\x1b[2J\x1b[1;1HFixture text")
            result = replay_profile(path)
        first, second = result["stream_analyzer"], result["acceptance_screen"]
        self.assertEqual(first["sha256"], second["sha256"])
        self.assertEqual(first["bytes"], second["bytes"])
        self.assertGreater(first["feed_ns"], 0)
        self.assertGreater(second["feed_ns"], 0)


class StageTests(unittest.TestCase):
    def test_missing_stages_explicit_and_separate(self):
        result = stage_metrics(
            [
                "DEBUG bmux prepare elapsed_us=1000 layout_us=900",
                "DEBUG bmux prepare scroll-reuse elapsed_us=200",
                "DEBUG bmux presenter output present_us=3000 write_us=100 flush_us=1",
            ]
        )
        self.assertEqual(result["prepare"]["p95_ms"], 1)
        self.assertEqual(result["scroll_reuse"]["samples"], 1)
        self.assertEqual(result["write"]["p95_ms"], 0.1)
        self.assertEqual(result["resource_only"]["samples"], 0)
        self.assertIsNone(result["resource_only"]["p95_ms"])

    def test_malformed_timing_tokens_are_not_numeric_samples(self):
        result = stage_metrics(
            [
                "bmux prepare elapsed_us=1000oops layout_us=2.5",
                "bmux presenter output present_us=-1 write_us=nan flush_us=1e9",
            ]
        )
        for name in ("prepare", "layout", "present", "write", "flush"):
            self.assertEqual(result[name]["samples"], 0, name)

    def test_nearest_rank_percentiles(self):
        result = stage_metrics(
            f"bmux prepare elapsed_us={i * 1000}" for i in range(1, 21)
        )
        self.assertEqual(
            result["prepare"], dict(samples=20, p50_ms=10, p95_ms=19, max_ms=20)
        )


class HistogramTests(unittest.TestCase):
    def test_quantiles_are_inclusive_bucket_bounds_not_points(self):
        result = duration_histogram_metrics([1, 1, 18, 0, 0, 0, 0, 0], 15000, 999)
        self.assertEqual(result["samples"], 20)
        self.assertEqual(
            result["quantile_bounds"]["p50"], dict(lower_us=501, upper_us=1000)
        )
        self.assertEqual(
            result["quantile_bounds"]["p95"], dict(lower_us=501, upper_us=1000)
        )
        self.assertNotIn("p95_ms", result)
        self.assertEqual(result["input_causality"], "unknown")

    def test_zero_duration_and_empty_are_distinct(self):
        zero = duration_histogram_metrics([1, 0, 0, 0, 0, 0, 0, 0], 0, 0)
        empty = duration_histogram_metrics([0] * 8, 0, 0)
        self.assertEqual(zero["quantile_bounds"]["p95"], dict(lower_us=0, upper_us=100))
        self.assertEqual(zero["max_us"], 0)
        self.assertIsNone(empty["max_us"])
        self.assertIsNone(empty["quantile_bounds"]["p95"])

    def test_open_bucket_and_saturation_do_not_fabricate_percentile(self):
        result = duration_histogram_metrics([0] * 7 + [1], 500000, 500000)
        self.assertEqual(
            result["quantile_bounds"]["p95"], dict(lower_us=256001, upper_us=None)
        )
        for buckets, total, maximum in (
            ([(1 << 64) - 1] + [0] * 7, 0, 0),
            ([1] + [0] * 7, (1 << 64) - 1, 100),
        ):
            result = duration_histogram_metrics(buckets, total, maximum)
            self.assertEqual(result["status"], "saturated_unknown_rank")
            self.assertIsNone(result["quantile_bounds"]["p95"])

    def test_invalid_values_rejected(self):
        for buckets, total, maximum in (
            ([0] * 7, 0, 0),
            ([True] + [0] * 7, 0, 0),
            ([-1] + [0] * 7, 0, 0),
            ([0] * 8, 1.5, 0),
            ([0] * 8, 0, 1 << 64),
        ):
            with self.assertRaises(ValueError):
                duration_histogram_metrics(buckets, total, maximum)


class PhaseTests(unittest.TestCase):
    def test_outcomes_and_missing_are_not_success_or_zero(self):
        result = phase_metrics(
            [
                "bmux presenter output present_us=1000 success=true",
                "bmux presenter output present_us=9000 success=false",
                "bmux presenter output present_us=5000",
            ]
        )
        self.assertEqual(result["by_outcome"]["successful"]["present"]["samples"], 1)
        self.assertEqual(result["by_outcome"]["failed"]["present"]["max_ms"], 9)
        self.assertEqual(result["by_outcome"]["unknown"]["present"]["max_ms"], 5)
        self.assertEqual(result["coverage"]["paint"], "missing_or_invalid")
        self.assertEqual(
            result["input_frame_correlation"], "unavailable_no_explicit_batch_linkage"
        )
        self.assertEqual(result["telemetry_loss"], "unknown_no_drop_accounting")

    def test_admission_pairs_by_id_not_order(self):
        result = phase_metrics(
            [
                "bmux input admission_attempt sequence=Some(1) observed_us=10 enqueue_us=20",
                "bmux input admission_attempt sequence=Some(2) observed_us=30 enqueue_us=40",
                "bmux input admission_result sequence=Some(2) accepted=true completion_us=90",
                "bmux input admission_result sequence=Some(1) accepted=true completion_us=120",
            ]
        )["admission"]
        self.assertEqual(result["counts"]["matched"], 2)
        self.assertEqual(result["distributions"]["admission_wait"]["max_ms"], 0.1)
        self.assertEqual(result["distributions"]["pre_enqueue"]["samples"], 2)

    def test_incomplete_ambiguous_rejected_invalid_and_cancelled(self):
        lines = []
        for identity in range(1, 8):
            lines.append(
                f"bmux input admission_attempt sequence=Some({identity}) observed_us=10 enqueue_us=20"
            )
        lines += [
            "bmux input admission_attempt sequence=Some(1) observed_us=10 enqueue_us=20",
            "bmux input admission_result sequence=Some(2) accepted=false completion_us=30",
            "bmux input admission_result sequence=Some(3) accepted=true completion_us=5",
            "bmux input admission_cancelled sequence=Some(4)",
            "bmux input admission_result sequence=Some(5) accepted=true completion_us=30oops",
            "bmux input admission_result sequence=Some(6) accepted=true completion_us=30",
            "bmux input admission_cancelled sequence=Some(6)",
            "bmux input admission_result sequence=Some(8) accepted=true completion_us=30",
            "bmux input admission_result sequence=None accepted=true completion_us=30",
        ]
        result = phase_metrics(lines)["admission"]
        self.assertEqual(
            result["counts"],
            dict(
                matched=0,
                ambiguous=2,
                rejected=1,
                invalid=2,
                cancelled=1,
                missing_result=1,
                missing_attempt=1,
            ),
        )
        self.assertEqual(result["malformed_records"], 1)
        self.assertIsNone(result["distributions"]["admission_wait"]["p95_ms"])


class ProvenanceTests(unittest.TestCase):
    def test_missing_identity_never_inferred(self):
        provenance = capture_provenance(dict(sha256="a" * 64))
        self.assertEqual(provenance["missing"], ["source_id", "build_id", "geometry"])
        self.assertFalse(provenance["verified"])
        self.assertIn("valid_sha256", capture_provenance(dict(sha256="bad"))["missing"])

    def test_equal_binary_is_not_comparable_workload(self):
        provenance = capture_provenance(
            dict(sha256="a" * 64, source_id="s", build_id="b", geometry=[140, 40])
        )
        result = compare_captures([dict(provenance=provenance)] * 2)
        self.assertFalse(result["comparable"])
        self.assertEqual(result["identity_fields"]["sha256"], "equal")
        self.assertEqual(result["speedup"], "not_evaluated")

    def test_different_identity_and_bad_types_are_explicit(self):
        first = capture_provenance(
            dict(sha256="a" * 64, source_id="s", build_id="b", geometry=[140, 40])
        )
        second = capture_provenance(
            dict(sha256="b" * 64, source_id="t", build_id="b", geometry=[80, 24])
        )
        result = compare_captures([dict(provenance=first), dict(provenance=second)])
        self.assertEqual(
            result["identity_fields"],
            dict(
                sha256="different",
                source_id="different",
                build_id="equal",
                geometry="different",
            ),
        )
        self.assertFalse(result["comparable"])
        invalid = capture_provenance(
            dict(source_id=[], build_id=42, geometry="unknown")
        )
        self.assertEqual(
            invalid["missing"], ["sha256", "source_id", "build_id", "geometry"]
        )

    def test_saved_capture_integration(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)
            (path / "terminal.bin").write_bytes(b"text")
            (path / "app-0.log").write_text(
                "bmux presenter output present_us=1000 success=false\n"
            )
            (path / "result.json").write_text(json.dumps(dict(sha256="a" * 64)))
            result = analyze_capture(path)
        self.assertEqual(result["stages"]["present"]["samples"], 1)
        self.assertEqual(
            result["phases"]["by_outcome"]["successful"]["present"]["samples"], 0
        )
        self.assertEqual(result["provenance"]["status"], "incomplete")


class WorkTests(unittest.TestCase):
    def test_counts_are_not_ms_and_missing_is_not_zero(self):
        result = work_metrics(
            [
                "DEBUG bmux work visited_nodes=100 scroll_candidates=2",
                "DEBUG bmux work visited_nodes=200 scroll_candidates=0",
                "DEBUG bmux input drained=8 pending=0 barrier_seq=64",
                "DEBUG bmux work visited_nodes=-1 action_candidates=4oops",
                "DEBUG unrelated pending=999 visited_nodes=999",
            ]
        )
        self.assertEqual(
            result["work.visited_nodes"], dict(samples=2, p50=100, p95=200, max=200)
        )
        self.assertEqual(result["input.pending"]["max"], 0)
        self.assertEqual(result["input.barrier_seq"]["max"], 64)
        self.assertEqual(result["work.action_candidates"]["samples"], 0)
        self.assertIsNone(result["work.revealed_nodes"]["max"])

    def test_exact_program_stage_contract_and_raw_work(self):
        line = (
            "DEBUG bmux stages sequence=9 project_us=100 controls_us=200 "
            "reveal_us=300 resource_us=400 paint_us=500 project_visited=10 "
            "retained_nodes=8 controls_visited=7 reveal_visited=6 "
            "resource_visited=5 paint_visited=4"
        )
        timings = stage_metrics([line])
        for name, expected in (
            ("project", 0.1),
            ("controls", 0.2),
            ("reveal", 0.3),
            ("resource", 0.4),
            ("paint", 0.5),
        ):
            self.assertEqual(timings[name]["p95_ms"], expected)
        counts = work_metrics([line])
        for name, expected in (
            ("project_visited", 10),
            ("retained_nodes", 8),
            ("controls_visited", 7),
            ("reveal_visited", 6),
            ("resource_visited", 5),
            ("paint_visited", 4),
        ):
            self.assertEqual(counts[f"stages.{name}"]["max"], expected)

    def test_nested_stage_times_do_not_change_existing_distributions(self):
        result = stage_metrics(
            [
                "bmux prepare elapsed_us=2000 layout_us=1000 resources_us=300",
                "bmux prepare resource-only elapsed_us=400",
                "bmux presenter output present_us=5000 write_us=100 max_write_us=80 flush_us=20",
            ]
        )
        self.assertEqual(result["prepare"]["p50_ms"], 2)
        self.assertEqual(result["layout"]["p50_ms"], 1)
        self.assertEqual(result["resources"]["p50_ms"], 0.3)
        self.assertEqual(result["write"]["p50_ms"], 0.1)
        self.assertEqual(result["max_write"]["p50_ms"], 0.08)
        self.assertEqual(result["resource_only"]["p50_ms"], 0.4)


class FixtureTests(unittest.TestCase):
    def matrix(self):
        records = []
        for pages in (1, 3, 10):
            for mode in ("text", "cold", "warm"):
                identity = dict(pages=pages, mode=mode)
                records.append(
                    dict(
                        identity,
                        fixture="production_albums_v1",
                        source_id="dirty-source-sha256",
                        build_id="release-locked-dependencies",
                        wheel_count=64,
                        wheel_point=[105, 20],
                        viewport=[140, 40],
                        sink="in_memory_no_parser",
                        sink_timing_semantics="in_memory_copy_and_lock_not_output_blocking",
                    )
                )
                for frame in range(64):
                    records.append(
                        dict(
                            identity,
                            frame=frame,
                            direction="down" if frame < 32 else "up",
                            update_us=1,
                            prepare_present_us=2,
                            byte_range=[frame * 10, (frame + 1) * 10],
                            bytes=10,
                        )
                    )
        return records

    def analyze(self, records):
        return fixture_metrics(
            ["test harness diagnostic", "[]", "null"] + [json.dumps(r) for r in records]
        )

    def test_buffering_identity_and_mixed_rejection(self):
        records = self.matrix()
        for record in records:
            if "fixture" in record:
                record.update(buffering_mode="buffered_64k", buffer_capacity=65536)
        self.assertTrue(self.analyze(records)["structure_valid"])
        records[0].update(buffering_mode="unbuffered", buffer_capacity=0)
        result = self.analyze(records)
        self.assertIn("incompatible buffering modes within matrix", result["errors"])
        self.assertTrue(
            all(not v["complete"] for v in result["warm_work_scaling"].values())
        )
        for mode, capacity in (
            ("buffered_64k", 0),
            ("unknown", 65536),
            (None, 0),
            ("unbuffered", False),
        ):
            records[0].update(buffering_mode=mode, buffer_capacity=capacity)
            self.assertFalse(self.analyze(records)["structure_valid"])
        records = self.matrix()
        records[0]["buffer_capacity"] = 0
        self.assertFalse(self.analyze(records)["structure_valid"])

    def test_sink_modes_are_not_blindly_comparable(self):
        records = self.matrix()
        for record in records:
            if "fixture" in record and record["pages"] == 10:
                record.update(
                    sink_mode="counting",
                    sink_timing_semantics="in_memory_count_and_lock_not_output_blocking",
                    stream_offset_semantics="accepted_byte_counts_no_stream",
                )
        result = self.analyze(records)
        self.assertFalse(result["structure_valid"])
        self.assertIn("incompatible sink modes within matrix", result["errors"])
        self.assertTrue(
            all(not value["complete"] for value in result["warm_work_scaling"].values())
        )

    def test_known_legacy_recording_only(self):
        self.assertTrue(self.analyze(self.matrix())["structure_valid"])
        for semantics in (
            None,
            "unknown",
            "in_memory_count_and_lock_not_output_blocking",
        ):
            records = self.matrix()
            records[0]["sink_timing_semantics"] = semantics
            self.assertFalse(self.analyze(records)["structure_valid"])
        for mode in (None, "unknown", "counting"):
            records = self.matrix()
            records[0]["sink_mode"] = mode
            self.assertFalse(self.analyze(records)["structure_valid"])

    def test_counting_requires_no_fabricated_stream(self):
        records = self.matrix()
        for record in records:
            if "fixture" in record:
                record.update(
                    sink_mode="counting",
                    sink_timing_semantics="in_memory_count_and_lock_not_output_blocking",
                    stream_offset_semantics="accepted_byte_counts_no_stream",
                    prefix_locator=None,
                    delta_locator=None,
                )
        self.assertTrue(self.analyze(records)["structure_valid"])
        records[0]["prefix_locator"] = "pages-1-text.init.bin"
        self.assertFalse(self.analyze(records)["structure_valid"])
        records[0]["prefix_locator"] = None
        records[1]["stream_byte_range"] = [0, 10]
        self.assertFalse(self.analyze(records)["structure_valid"])

    def test_complete_matrix_is_structure_not_product_acceptance(self):
        result = self.analyze(self.matrix())
        self.assertTrue(result["structure_valid"])
        self.assertTrue(result["provenance_complete"])
        self.assertEqual(len(result["cases"]), 9)
        self.assertEqual(
            result["acceptance"], "not_evaluated_geometry_and_runtime_evidence_required"
        )

    def test_fallback_missing_unknown_not_zero(self):
        result = self.analyze(self.matrix())["cases"]["1/warm"]["fallback"]
        self.assertFalse(result["complete"])
        self.assertIsNone(result["full_layout_frames"])
        self.assertIsNone(result["zero_full_layout_frames"])
        self.assertIsNone(result["projection_frames"])
        self.assertIsNone(result["all_frames_projected_without_full_layout"])

    def test_fallback_requires_all_64_frames_not_good_p95(self):
        records = self.matrix()
        for record in records:
            if "frame" in record:
                record.update(full_layouts=0, projection_used=True)
        result = self.analyze(records)["cases"]["1/warm"]["fallback"]
        self.assertTrue(result["all_frames_projected_without_full_layout"])
        self.assertEqual(result["zero_full_layout_frames"], 64)
        self.assertEqual(result["projection_frames"], 64)
        frame = next(
            record
            for record in records
            if record.get("mode") == "warm" and "frame" in record
        )
        frame.update(full_layouts=1, projection_used=False)
        result = self.analyze(records)["cases"]["1/warm"]["fallback"]
        self.assertTrue(result["complete"])
        self.assertEqual(result["full_layout_frames"], 1)
        self.assertEqual(result["full_layouts"], 1)
        self.assertEqual(result["zero_full_layout_frames"], 63)
        self.assertFalse(result["all_frames_projected_without_full_layout"])
        frame.pop("full_layouts")
        result = self.analyze(records)["cases"]["1/warm"]["fallback"]
        self.assertFalse(result["complete"])
        self.assertIsNone(result["all_frames_projected_without_full_layout"])
        frame.update(full_layouts=True, projection_used=1)
        result = self.analyze(records)["cases"]["1/warm"]["fallback"]
        self.assertFalse(result["complete"])
        self.assertEqual(result["full_layout_samples"], 63)
        self.assertEqual(result["projection_samples"], 63)

    def test_prefix_plus_delta_offsets_are_explicit_and_backward_compatible(self):
        records = self.matrix()
        for record in records:
            if "fixture" in record:
                stem = f"pages-{record['pages']}-{record['mode']}"
                record.update(
                    prefix_bytes=123,
                    prefix_locator=f"{stem}.init.bin",
                    delta_locator=f"{stem}.bin",
                )
            else:
                record["stream_byte_range"] = [
                    123 + value for value in record["byte_range"]
                ]
        self.assertTrue(self.analyze(records)["structure_valid"])
        records[1]["stream_byte_range"] = [0, 10]
        self.assertFalse(self.analyze(records)["structure_valid"])
        records[1]["stream_byte_range"] = [123, 133]
        records[0]["prefix_locator"] = "../old.bin"
        self.assertFalse(self.analyze(records)["structure_valid"])
        records[0]["prefix_locator"] = None
        self.assertFalse(self.analyze(records)["structure_valid"])
        records[0]["delta_locator"] = None
        self.assertTrue(self.analyze(records)["structure_valid"])
        records[0]["prefix_bytes"] = "bad"
        self.assertFalse(self.analyze(records)["structure_valid"])

    def test_public_snapshot_json_fields_are_additive(self):
        records = self.matrix()
        for record in records:
            if "frame" in record:
                record.update(
                    sequence=record["frame"] + 2,
                    project_us=100,
                    controls_us=200,
                    reveal_us=300,
                    resource_us=400,
                    paint_us=500,
                    project_visited=12,
                    retained_nodes=11,
                    controls_visited=10,
                    reveal_visited=9,
                    resource_visited=8,
                    paint_visited=7,
                )
        result = self.analyze(records)
        self.assertTrue(result["structure_valid"])
        case = result["cases"]["10/warm"]
        self.assertEqual(case["timings"]["project_us"]["p95_ms"], 0.1)
        self.assertEqual(case["timings"]["paint_us"]["p95_ms"], 0.5)
        self.assertEqual(case["work"]["resource_visited"]["p95"], 8)
        self.assertTrue(
            all(value["within_bound"] for value in result["warm_work_scaling"].values())
        )

    def test_work_scaling_requires_complete_raw_counts(self):
        records = self.matrix()
        missing = self.analyze(records)["warm_work_scaling"]["project_visited"]
        self.assertFalse(missing["complete"])
        self.assertIsNone(missing["within_bound"])
        for record in records:
            if "frame" in record:
                record["project_visited"] = 10 if record["pages"] == 1 else 20
        self.assertTrue(
            self.analyze(records)["warm_work_scaling"]["project_visited"][
                "within_bound"
            ]
        )
        for record in records:
            if "frame" in record and record["pages"] == 10:
                record["project_visited"] = 21
        self.assertFalse(
            self.analyze(records)["warm_work_scaling"]["project_visited"][
                "within_bound"
            ]
        )
        for record in records:
            if (
                record.get("mode") == "warm"
                and record.get("pages") == 1
                and "frame" in record
            ):
                record.pop("project_visited")
                break
        self.assertIsNone(
            self.analyze(records)["warm_work_scaling"]["project_visited"][
                "within_bound"
            ]
        )

    def test_fixture_timings_keep_units_and_missing_sink_explicit(self):
        records = self.matrix()
        for record in records:
            if "frame" in record:
                record["sink_write_ns"] = 250_000
        result = self.analyze(records)
        timings = result["cases"]["1/text"]["timings"]
        self.assertEqual(timings["prepare_present_us"]["p50_ms"], 0.002)
        self.assertEqual(timings["sink_write_ns"]["p95_ms"], 0.25)
        self.assertEqual(timings["sink_flush_ns"]["samples"], 0)
        self.assertIsNone(timings["sink_flush_ns"]["max_ms"])

    def test_old_fixture_without_identity_remains_readable_not_provenanced(self):
        records = self.matrix()
        for record in records:
            record.pop("source_id", None)
            record.pop("build_id", None)
        result = self.analyze(records)
        self.assertTrue(result["structure_valid"])
        self.assertFalse(result["provenance_complete"])
        self.assertEqual(len(result["missing_identity"]), 9)

    def test_missing_duplicate_reordered_and_bad_ranges_rejected(self):
        for change in (
            "drop",
            "duplicate",
            "reorder",
            "range",
            "direction",
            "timing",
            "metadata",
        ):
            with self.subTest(change=change):
                records = self.matrix()
                if change == "drop":
                    records.pop(1)
                elif change == "duplicate":
                    records.insert(1, records[1])
                elif change == "reorder":
                    records[1], records[2] = records[2], records[1]
                elif change == "range":
                    records[2]["byte_range"] = [0, 20]
                elif change == "direction":
                    records[1]["direction"] = "up"
                elif change == "timing":
                    records[1]["update_us"] = -1
                else:
                    records[0]["wheel_count"] = 63
                self.assertFalse(self.analyze(records)["structure_valid"])

    def test_empty_log_and_mixed_source_identity_rejected(self):
        self.assertFalse(self.analyze([])["structure_valid"])
        records = self.matrix()
        records[65]["source_id"] = "other-source"
        self.assertIn(
            "inconsistent source/build identity within matrix",
            self.analyze(records)["errors"],
        )

    def test_bad_identity_types_and_ranges_do_not_crash(self):
        records = self.matrix()
        records[0]["source_id"] = []
        records[1]["byte_range"] = ["oops", None]
        records.append(dict(pages=[], mode={}, frame=0))
        result = self.analyze(records)
        self.assertFalse(result["structure_valid"])
        self.assertFalse(result["provenance_complete"])


if __name__ == "__main__":
    unittest.main()
