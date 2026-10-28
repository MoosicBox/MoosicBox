#!/usr/bin/env python3
"""Deterministic hierarchy invariants and real artifact reader regressions."""
import copy
import importlib.util
import json
from pathlib import Path
import random
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("tree", ROOT / "lib/shard_tree.py")
tree = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(tree)


def jobs(count):
    return [
        dict(id=i, os="linux", name="package", features=[str(i)]) for i in range(count)
    ]


def leaves(manifest):
    return [n for n in manifest["nodes"].values() if n["kind"] == "leaf"]


class HierarchyTests(unittest.TestCase):
    def test_leaf_boundaries(self):
        for limit in (1, 2, 256):
            for count in sorted({0, 1, max(0, limit - 1), limit, limit + 1}):
                with self.subTest(limit=limit, count=count):
                    levels = [
                        {"group-by-jq": ".os"},
                        {"group-by-jq": ".name", "matrix-max-jobs": limit},
                    ]
                    manifest = tree.plan(jobs(count), levels)
                    tree.validate(manifest, jobs(count))
                    self.assertTrue(
                        all(len(n["values"]) <= limit for n in leaves(manifest))
                    )
                    self.assertEqual(len(manifest["nodes"]) == 0, count == 0)

    def test_parent_overflow_and_root_rejection(self):
        source = jobs(257)
        for job in source:
            job["name"] = str(job["id"])
        manifest = tree.plan(source, [{"group-by-jq": ".os"}, {"group-by-jq": ".name"}])
        self.assertEqual(len(manifest["roots"]), 2)
        self.assertEqual(
            [len(manifest["nodes"][r["id"]]["values"]) for r in manifest["roots"]],
            [256, 1],
        )
        with self.assertRaisesRegex(ValueError, "Root descriptor"):
            tree.plan(source, [{"group-by-jq": ".name"}])

    def test_combined_output_accounting(self):
        import output_budget as budget

        self.assertEqual(budget.utf16_bytes("aé🎵"), 8)
        exact = {"matrix": "x" * ((budget.PAYLOAD - 12) // 2)}
        self.assertEqual(budget.validate_outputs(exact), budget.PAYLOAD)
        with self.assertRaises(ValueError):
            budget.validate_outputs({"matrix": exact["matrix"] + "x"})
        # Aliases and unrelated metadata count individually, even when equal.
        value = "x" * 250000
        for outputs in (
            {"matrix": value, "direct-matrix": value},
            {"matrix": value, "reasoning": value},
        ):
            with self.assertRaises(ValueError):
                budget.validate_outputs(outputs)
        for outputs in (
            {"a": 'é🎵\\"\n'},
            exact,
            {"matrix": value, "reasoning": value},
        ):
            result = subprocess.run(
                [
                    "bash",
                    "-c",
                    'source "$1"; jq -c "$CLIPPIER_OUTPUT_JQ outputbytes"',
                    "test",
                    str(ROOT / "lib/shard-plan.sh"),
                ],
                input=json.dumps(outputs),
                text=True,
                capture_output=True,
                check=True,
            )
            self.assertEqual(int(result.stdout), budget.output_bytes(outputs))

    def test_analysis_guard_covers_every_declared_output(self):
        workflow = json.loads(
            subprocess.run(
                [
                    "yq",
                    "-o=json",
                    ".jobs.determine-affected-packages",
                    str(ROOT.parents[1] / "workflows/build-and-test.yml"),
                ],
                text=True,
                capture_output=True,
                check=True,
            ).stdout
        )
        script = next(
            s["run"]
            for s in workflow["steps"]
            if s.get("name") == "Validate combined job output budget"
        )
        import re

        checked = dict(re.findall(r'^  "([^"]+)": (.*?)(?:,)?$', script, re.MULTILINE))
        self.assertEqual(set(checked), set(workflow["outputs"]))
        for key, expression in workflow["outputs"].items():
            self.assertEqual(
                checked[key].rstrip(","),
                "${{ toJson(format('{0}', " + expression[3:-3].strip() + ")) }}",
            )

    def test_payload_and_unicode(self):
        value = {"emoji": '🎵\\"'}
        size = tree.utf16_bytes(tree.encode([value])) + tree.utf16_bytes(
            "hierarchy-matrix"
        )
        self.assertEqual(tree.partition([value], 1, size), [[value]])
        with self.assertRaises(ValueError):
            tree.partition([value], 1, size - 1)
        source = jobs(6)
        for job in source:
            job["payload"] = "🎵" * 100000
        manifest = tree.plan(source, [{"group-by-jq": ".os"}, {"group-by-jq": ".name"}])
        self.assertEqual([len(n["values"]) for n in leaves(manifest)], [2, 2, 2])
        source[0]["payload"] = "x" * 500000
        with self.assertRaises(ValueError):
            tree.plan(source, [{"group-by-jq": ".os"}])
        with self.assertRaises(ValueError):
            tree.plan(jobs(1), [{"group-by-jq": ".os", "name-jq": '"x" * 500000'}])

    def test_generic_keys_and_order(self):
        source = jobs(8)
        keys = [None, False, 1, "1", [1, "x"], {"a": 1, "b": 2}, {"b": 2, "a": 1}, 1.0]
        for job, key in zip(source, keys):
            job["key"] = key
        manifest = tree.plan(
            source, [{"group-by-jq": ".key", "name-jq": '"same label"'}]
        )
        self.assertEqual(len(manifest["roots"]), 6)
        tree.validate(manifest, source)
        for leaf in leaves(manifest):
            ids = [j["id"] for j in leaf["values"]]
            self.assertEqual(ids, sorted(ids))
        self.assertEqual(
            manifest,
            tree.plan(source, [{"group-by-jq": ".key", "name-jq": '"same label"'}]),
        )

    def test_randomized_losslessness(self):
        randomizer = random.Random(1841)
        for iteration in range(16):
            source = jobs(randomizer.randrange(1, 35))
            for job in source:
                job.update(
                    os=randomizer.choice(["linux", "windows"]),
                    name=str(randomizer.randrange(3)),
                )
            source.append(copy.deepcopy(source[0]))
            levels = [
                {"group-by-jq": ".os"},
                {
                    "group-by-jq": "[.name,.os]",
                    "matrix-max-jobs": randomizer.randrange(1, 6),
                },
            ]
            depth = 1 + iteration % tree.MAX_GROUP_LEVELS
            if depth == 1:
                levels = [levels[1]]
            else:
                levels = (
                    levels[:1] + [{"group-by-jq": ".name"}] * (depth - 2) + levels[1:]
                )
            manifest = tree.plan(source, levels)
            tree.validate(manifest, source)
            self.assertEqual(manifest, tree.plan(source, levels))
            renamed = tree.plan(
                source, [dict(level, **{"name-jq": '"renamed"'}) for level in levels]
            )
            self.assertEqual(set(manifest["nodes"]), set(renamed["nodes"]))

    def test_single_level_and_root_boundaries(self):
        for limit in (1, 2, 256):
            for count in sorted({0, 1, max(0, limit - 1), limit, limit + 1}):
                source = jobs(count)
                for job in source:
                    job["key"] = job["id"]
                if count > limit:
                    with self.assertRaisesRegex(ValueError, "Root descriptor"):
                        tree.plan(source, [{"group-by-jq": ".key"}], limit)
                else:
                    manifest = tree.plan(source, [{"group-by-jq": ".key"}], limit)
                    self.assertTrue(
                        all(n["kind"] == "leaf" for n in manifest["nodes"].values())
                    )
                    tree.validate(manifest, source)

    def test_descriptor_payload_splitting(self):
        source = jobs(4)
        for job in source:
            job["name"] = str(job["id"])
        levels = [
            {"group-by-jq": ".os"},
            {"group-by-jq": ".name", "name-jq": '"🎵" * 100000'},
        ]
        manifest = tree.plan(source, levels)
        self.assertEqual(len(manifest["roots"]), 2)
        for descriptor in manifest["roots"]:
            node = manifest["nodes"][descriptor["id"]]
            self.assertEqual(len(node["values"]), 2)
            self.assertIn("matrix payload budget", node["split_reasons"])
            self.assertTrue(tree.fits(node["values"], 256))
        with self.assertRaisesRegex(ValueError, "Root descriptor"):
            tree.plan(source, [{"group-by-jq": ".name", "name-jq": '"🎵" * 100000'}])

    def test_every_depth_and_caller_budget(self):
        for caller in range(1, tree.MAX_GROUP_LEVELS + 1):
            available = tree.capacity(caller)
            for depth in range(1, available + 1):
                levels = [{"group-by-jq": ".os"}] * depth
                source = jobs(5)
                manifest = tree.plan(source, levels, caller_depth=caller)
                tree.validate(manifest, source)
                self.assertEqual(manifest["depth"], depth)
                self.assertEqual(len(manifest["nodes"]), depth)
            with self.assertRaisesRegex(ValueError, "only.*available"):
                tree.plan(
                    [], [{"group-by-jq": ".os"}] * (available + 1), caller_depth=caller
                )
        for caller in (0, -1, True, 9, "1"):
            with self.assertRaises(ValueError):
                tree.plan([], [{"group-by-jq": ".os"}], caller_depth=caller)
        for depth in (0, -1, True, 9):
            manifest = tree.plan(jobs(1), [{"group-by-jq": ".os"}])
            manifest["depth"] = depth
            with self.assertRaises(ValueError):
                tree.validate(manifest)

    def test_deep_partitioning_and_corruption(self):
        for depth in range(1, tree.MAX_GROUP_LEVELS + 1):
            source = jobs(7)
            levels = [{"group-by-jq": ".os"} for _ in range(depth)]
            levels[-1]["matrix-max-jobs"] = 2
            manifest = tree.plan(source, levels)
            tree.validate(manifest, source)
            self.assertEqual([len(n["values"]) for n in leaves(manifest)], [2, 2, 2, 1])
            bad = copy.deepcopy(manifest)
            leaf = leaves(bad)[0]
            leaf["level"] += 1
            with self.assertRaises(ValueError):
                tree.validate(bad, source)

    def test_generated_graph_and_actual_dispatcher_readers(self):
        import re
        from shard_capacity import (
            dispatcher_name,
            GITHUB_WORKFLOW_LEVELS,
            GITHUB_UNIQUE_WORKFLOWS,
        )

        generator = ROOT / "generate-dispatchers.py"
        subprocess.run(["python3", str(generator), "--check"], check=True)
        workflows = ROOT.parents[1] / "workflows"

        # Count static call targets, including skipped leaf branches: no cycles,
        # no dynamic targets, maximum caller-to-executor path stays within 10.
        def walk(name, path):
            self.assertNotIn(name, path)
            text = (workflows / name).read_text()
            targets = re.findall(r"uses: \./\.github/workflows/([^\s]+)", text)
            lengths = [walk(target, path + [name]) for target in targets]
            return 1 + max(lengths, default=0)

        for entry in range(1, tree.MAX_GROUP_LEVELS + 1):
            longest = walk(dispatcher_name(entry), [])
            self.assertEqual(entry + longest, GITHUB_WORKFLOW_LEVELS)
        unique = set()

        def collect(name):
            for target in re.findall(
                r"uses: \./\.github/workflows/([^\s]+)", (workflows / name).read_text()
            ):
                if target not in unique:
                    unique.add(target)
                    collect(target)

        collect("build-and-test.yml")
        self.assertLessEqual(len(unique), GITHUB_UNIQUE_WORKFLOWS)
        for entry in range(1, tree.MAX_GROUP_LEVELS + 1):
            text = (workflows / dispatcher_name(entry)).read_text()
            self.assertNotIn("runs-on:", text)
            self.assertNotIn("prepare:", text)
            self.assertIn("fromJson(inputs.node-json)", text)

    def test_inline_routing_and_resolve(self):
        for depth in range(1, tree.MAX_GROUP_LEVELS + 1):
            source = jobs(5)
            source.append(copy.deepcopy(source[0]))
            manifest = tree.plan(source, [{"group-by-jq": ".os"}] * depth)
            routes = tree.routing(manifest, source)
            with tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary) / "hierarchy"
                directory.mkdir()
                (directory / "tree.json").write_text(tree.encode(manifest))
                (directory.parent / "jobs.json").write_text(tree.encode(source))

                def check(items):
                    for item in items:
                        if item["kind"] == "group":
                            check(item["children"])
                        else:
                            for ref in item["jobs"]:
                                self.assertEqual(
                                    tree.resolve_job(directory, item["id"], ref),
                                    source[ref["index"]],
                                )
                                for field, value in (
                                    ("index", 999),
                                    ("os", "wrong"),
                                    ("name", "changed"),
                                ):
                                    bad = dict(ref, **{field: value})
                                    with self.assertRaises(ValueError):
                                        tree.resolve_job(directory, item["id"], bad)

                check(routes)
        huge = tree.plan(
            jobs(1), [{"group-by-jq": ".os", "name-jq": '"x" * 250000'}] * 2
        )
        with self.assertRaises(ValueError):
            tree.routing(huge, jobs(1))

    def test_leaf_lifecycle_parity_and_resolver_script(self):
        workflows = ROOT.parents[1] / "workflows"

        def steps(filename):
            return json.loads(
                subprocess.run(
                    ["yq", "-o=json", ".jobs.build.steps", str(workflows / filename)],
                    capture_output=True,
                    text=True,
                    check=True,
                ).stdout
            )

        # All execution steps after resolution must stay equivalent to the
        # established flat lifecycle, apart from reference/name expressions.
        flat = steps("clippier-build-shard.yml")
        nested = steps("clippier-build-leaf.yml")

        def normalize(value):
            text = json.dumps(value, sort_keys=True)
            for old, new in (
                ("steps.resolve.outputs.package", "toJson(matrix.package)"),
                ("fromJson(toJson(matrix.package)).name", "matrix.package.name"),
                ("matrix.job.os", "matrix.package.os"),
                (
                    "inputs.node-id != '' && inputs.node-id || inputs.shard-id",
                    "inputs.node-id",
                ),
            ):
                text = text.replace(old, new)
            return text

        self.assertEqual(normalize(flat[2:]), normalize(nested[4:]))
        source = jobs(2)
        manifest = tree.plan(source, [{"group-by-jq": ".os"}])
        routes = tree.routing(manifest, source)
        script = next(s["run"] for s in nested if s.get("id") == "resolve")
        with tempfile.TemporaryDirectory() as temporary:
            import os

            base = Path(temporary)
            lib = base / ".github/actions/clippier/lib"
            lib.mkdir(parents=True)
            for filename in ("shard_tree.py", "shard_capacity.py", "output_budget.py"):
                (lib / filename).write_text((ROOT / "lib" / filename).read_text())
            directory = base / "clippier-plan/hierarchy"
            directory.mkdir(parents=True)
            (directory / "tree.json").write_text(tree.encode(manifest))
            (directory.parent / "jobs.json").write_text(tree.encode(source))
            output = base / "outputs"
            env = dict(
                os.environ,
                NODE_ID=routes[0]["id"],
                JOB_REFERENCE=tree.encode(routes[0]["jobs"][0]),
                RUNNER_TEMP=str(base),
                GITHUB_OUTPUT=str(output),
            )
            result = subprocess.run(
                ["bash", "-e", "-o", "pipefail", "-c", script],
                cwd=base,
                env=env,
                capture_output=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(
                json.loads(output.read_text().removeprefix("package=")), source[0]
            )
            output.write_text("")
            env["NODE_ID"] = "../invalid"
            result = subprocess.run(
                ["bash", "-e", "-o", "pipefail", "-c", script],
                cwd=base,
                env=env,
                capture_output=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(output.read_text(), "")

    def test_configuration_and_expression_errors(self):
        for levels in (
            [],
            {},
            [{"group-by-jq": "."}] * (tree.MAX_GROUP_LEVELS + 1),
            [{"group-by-jq": ".", "typo": 1}],
            [{"group-by-jq": ""}],
            [{"group-by-jq": ".", "matrix-max-jobs": True}],
        ):
            with self.assertRaises(ValueError):
                tree.plan([], levels)
        for expression in ("empty", ".os,.name", "(", 'error("private value")'):
            with self.assertRaises(ValueError) as error:
                tree.plan(jobs(1), [{"group-by-jq": expression}])
            self.assertNotIn("private value", str(error.exception))
        for expression in ('""', '"a\\nb"', "1", "empty", '"a","b"'):
            with self.assertRaises(ValueError):
                tree.plan(jobs(1), [{"group-by-jq": ".os", "name-jq": expression}])

    def test_corrupt_tree(self):
        original = tree.plan(
            jobs(3), [{"group-by-jq": ".os"}, {"group-by-jq": ".name"}]
        )
        for mutation in (
            "missing",
            "unreachable",
            "cycle",
            "type",
            "version",
            "path",
            "changed",
            "duplicate",
        ):
            manifest = copy.deepcopy(original)
            root = manifest["nodes"][manifest["roots"][0]["id"]]
            leaf = leaves(manifest)[0]
            if mutation == "missing":
                del manifest["nodes"][leaf["id"]]
            elif mutation == "unreachable":
                manifest["nodes"]["node-" + "a" * 24] = copy.deepcopy(leaf)
            elif mutation == "cycle":
                root["values"] = manifest["roots"]
            elif mutation == "type":
                leaf["kind"] = "group"
            elif mutation == "version":
                manifest["version"] = 99
            elif mutation == "path":
                leaf["path"][0] = "wrong parent"
            elif mutation == "changed":
                leaf["values"][0]["features"] = []
            else:
                root["values"].append(copy.deepcopy(root["values"][0]))
            with self.assertRaises((ValueError, KeyError)):
                tree.validate(manifest, jobs(3))

    def test_real_cli_atomicity_reader_and_preview(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            source, config = base / "jobs.json", base / "levels.json"
            source.write_text(json.dumps(jobs(3)))
            config.write_text('[{"group-by-jq":".os"},{"group-by-jq":".name"}]')
            cli = ["python3", str(ROOT / "lib/shard_tree.py")]
            result = subprocess.run(
                cli + ["plan", str(source), str(config), str(base / "hierarchy")],
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            manifest = json.loads((base / "hierarchy/tree.json").read_text())
            for node in manifest["nodes"].values():
                read = subprocess.run(
                    cli + ["read", str(base / "hierarchy"), node["id"], node["kind"]],
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(read.returncode, 0, read.stderr)
                self.assertEqual(json.loads(read.stdout), node["values"])
            for node_id, kind in (
                ("../jobs", "leaf"),
                ("node-" + "f" * 24, "leaf"),
                (manifest["roots"][0]["id"], "leaf"),
            ):
                read = subprocess.run(
                    cli + ["read", str(base / "hierarchy"), node_id, kind],
                    capture_output=True,
                )
                self.assertNotEqual(read.returncode, 0)
                self.assertEqual(read.stdout, b"")
            self.assertIn(
                "Preparation jobs: 0", (base / "hierarchy/preview.md").read_text()
            )
            config.write_text('[{"group-by-jq":"empty"}]')
            failed = subprocess.run(
                cli + ["plan", str(source), str(config), str(base / "failed")],
                capture_output=True,
            )
            self.assertNotEqual(failed.returncode, 0)
            self.assertFalse((base / "failed").exists())


if __name__ == "__main__":
    unittest.main()
