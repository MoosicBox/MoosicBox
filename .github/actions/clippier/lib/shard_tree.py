#!/usr/bin/env python3
"""Plan, validate, preview and read bounded GitHub workflow hierarchies.

Only jq evaluates user expressions. This module never executes shell source.
YAML configuration is converted to JSON by the action's existing yq dependency.
"""
import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

CEILING = 256
PAYLOAD = 200000  # Conservative UTF-8 transport budget, not GitHub's exact limit.


def encode(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def fits(values, limit, budget=PAYLOAD):
    return len(values) <= limit and len(encode(values).encode()) <= budget


def limit_value(value):
    if type(value) is not int or not 1 <= value <= CEILING:
        raise ValueError("matrix-max-jobs must be an integer from 1 to 256")
    return value


def partition(values, limit, budget=PAYLOAD):
    parts = []
    for value in values:
        if not fits([value], limit, budget):
            raise ValueError("Single job/descriptor exceeds transport payload budget")
        if not parts or not fits(parts[-1] + [value], limit, budget):
            parts.append([])
        parts[-1].append(value)
    return parts


def jq(expression, value, context):
    result = subprocess.run(
        ["jq", "-c", "[ (" + expression + ") ]"],
        input=encode(value),
        text=True,
        capture_output=True,
        timeout=30,
    )
    if result.returncode:
        # Do not echo arbitrary job/environment data from jq error messages.
        raise ValueError(
            f"{context}: jq evaluation failed (check expression syntax/types)"
        )
    values = json.loads(result.stdout)
    if len(values) != 1:
        raise ValueError(f"{context}: expression must produce exactly one value")
    return values[0]


def configuration(levels, default):
    limit_value(default)
    if not isinstance(levels, list) or not 1 <= len(levels) <= 2:
        raise ValueError(
            "shard-levels must contain one or two levels (static dispatcher depth)"
        )
    for i, level in enumerate(levels, 1):
        if not isinstance(level, dict) or set(level) - {
            "group-by-jq",
            "name-jq",
            "matrix-max-jobs",
        }:
            raise ValueError(f"Level {i}: invalid or unknown configuration fields")
        for field in ("group-by-jq", "name-jq"):
            if field == "name-jq" and field not in level:
                continue
            if not isinstance(level.get(field), str) or not level[field].strip():
                raise ValueError(f"Level {i}: {field} must be a nonempty jq expression")
        limit_value(level.get("matrix-max-jobs", default))


def plan(jobs, levels, default=256):
    configuration(levels, default)
    if not isinstance(jobs, list) or any(not isinstance(j, dict) for j in jobs):
        raise ValueError("Job plan must be an array of objects")
    nodes = {}

    def build(items, depth, path):
        level = levels[depth]
        limit = level.get("matrix-max-jobs", default)
        expression = level["group-by-jq"]
        grouped = jq(
            "to_entries | map(.key as $index | .value | [(" + expression + ")] | "
            'if length != 1 then error("one key required") '
            "else {key:.[0],index:$index} end) | group_by(.key) | "
            "map({key:.[0].key, indices:map(.index)})",
            items,
            f"Level {depth + 1}, group-by-jq",
        )
        descriptors = []
        for group in grouped:
            key = group["key"]
            members = [items[i] for i in group["indices"]]
            group_path = path + [key]
            children = (
                build(members, depth + 1, group_path)
                if depth + 1 < len(levels)
                else None
            )
            values = children if children is not None else members
            parts = partition(values, limit)
            for index, part in enumerate(parts, 1):
                identity = encode([group_path, index, depth])
                node_id = "node-" + hashlib.sha256(identity.encode()).hexdigest()[:24]
                context = dict(
                    group=key,
                    path=group_path,
                    level=depth + 1,
                    job_count=len(members),
                    part_index=index,
                    part_count=len(parts),
                )
                name = jq(
                    level.get(
                        "name-jq", '.group | if type == "string" then . else tojson end'
                    ),
                    context,
                    f"Level {depth + 1}, name-jq",
                )
                if (
                    not isinstance(name, str)
                    or not name
                    or any(ord(c) < 32 or ord(c) == 127 for c in name)
                ):
                    raise ValueError(
                        f"Level {depth + 1}: name must be a nonempty single-line string"
                    )
                if len(name.encode()) > PAYLOAD:
                    raise ValueError(
                        f"Level {depth + 1}: name exceeds descriptor payload budget"
                    )
                if len(parts) > 1:
                    name += f" · {index}/{len(parts)}"
                kind = "group" if children is not None else "leaf"
                descriptor = dict(id=node_id, name=name, kind=kind)
                if node_id in nodes:
                    raise ValueError("Duplicate node identity")
                reasons = []
                if len(values) > limit:
                    reasons.append("matrix job/descriptor-count limit")
                if len(encode(values).encode()) > PAYLOAD:
                    reasons.append("matrix payload budget")
                nodes[node_id] = dict(
                    **descriptor,
                    path=group_path,
                    level=depth + 1,
                    limit=limit,
                    split_reasons=reasons,
                    values=part,
                )
                descriptors.append(descriptor)
        return descriptors

    roots = build(jobs, 0, [])
    if not fits(roots, default):
        raise ValueError(
            "Root descriptor matrix exceeds limits; reduce groups/names or increase matrix-max-jobs. Additional dispatcher depth is required to paginate roots"
        )
    tree = dict(
        version=1, depth=len(levels), matrix_max_jobs=default, roots=roots, nodes=nodes
    )
    validate(tree, jobs)
    return tree


def validate(tree, jobs=None):
    if not isinstance(tree, dict) or set(tree) != {
        "version",
        "depth",
        "matrix_max_jobs",
        "roots",
        "nodes",
    }:
        raise ValueError("Invalid hierarchy schema")
    if (
        type(tree.get("version")) is not int
        or tree.get("version") != 1
        or type(tree.get("depth")) is not int
        or tree.get("depth") not in (1, 2)
    ):
        raise ValueError("Unsupported hierarchy schema/depth")
    nodes = tree["nodes"]
    seen = set()
    flattened = []
    default = limit_value(tree["matrix_max_jobs"])
    if not fits(tree["roots"], default):
        raise ValueError("Oversized root matrix")

    def visit(descriptor, depth, parent_path):
        if not isinstance(descriptor, dict) or set(descriptor) != {
            "id",
            "name",
            "kind",
        }:
            raise ValueError("Invalid descriptor")
        if (
            not isinstance(descriptor["name"], str)
            or not descriptor["name"]
            or any(ord(c) < 32 or ord(c) == 127 for c in descriptor["name"])
        ):
            raise ValueError("Invalid descriptor name")
        node_id = descriptor["id"]
        if not re.fullmatch(r"node-[0-9a-f]{24}", node_id) or node_id in seen:
            raise ValueError("Invalid, duplicate or cyclic node reference")
        seen.add(node_id)
        node = nodes[node_id]
        if set(node) != {
            "id",
            "name",
            "kind",
            "path",
            "level",
            "limit",
            "split_reasons",
            "values",
        } or not isinstance(node["values"], list):
            raise ValueError("Invalid node schema")
        if not isinstance(node["path"], list) or len(node["path"]) != depth:
            raise ValueError("Invalid group path")
        expected = "leaf" if depth == tree["depth"] else "group"
        if (
            descriptor != {k: node[k] for k in ("id", "name", "kind")}
            or node["kind"] != expected
            or node["level"] != depth
        ):
            raise ValueError("Node identity/type/depth mismatch")
        if not node["values"] or not fits(node["values"], limit_value(node["limit"])):
            raise ValueError("Empty or oversized node matrix")
        if node["path"][:-1] != parent_path:
            raise ValueError("Child belongs to a different parent path")
        if expected == "leaf":
            flattened.extend(node["values"])
        else:
            for child in node["values"]:
                visit(child, depth + 1, node["path"])

    for root in tree["roots"]:
        visit(root, 1, [])
    if seen != set(nodes):
        raise ValueError("Unreachable nodes")
    if jobs is not None and Counter(map(encode, jobs)) != Counter(
        map(encode, flattened)
    ):
        raise ValueError("Hierarchy altered, duplicated or omitted execution jobs")


def preview(tree):
    lines = ["## Nested workflow plan", ""]

    def show(descriptor, indent):
        node = tree["nodes"][descriptor["id"]]
        unit = "jobs" if node["kind"] == "leaf" else "child calls"
        reason = (
            "; split: " + ", ".join(node["split_reasons"])
            if node["split_reasons"]
            else ""
        )
        lines.append(
            f"{'  ' * indent}- {node['name']} — {len(node['values'])} {unit}; limit={node['limit']}{reason}"
        )
        if node["kind"] == "group":
            for child in node["values"]:
                show(child, indent + 1)

    for root in tree["roots"]:
        show(root, 0)
    lines += [
        "",
        f"Preparation jobs: {len(tree['nodes'])}. Concurrency is controlled separately.",
        "Parts split on job/descriptor count or the conservative 200000-byte payload budget.",
    ]
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    create = commands.add_parser("plan")
    create.add_argument("jobs", type=Path)
    create.add_argument(
        "levels", type=Path, help="JSON levels (convert YAML with yq -o=json)"
    )
    create.add_argument("destination", type=Path)
    create.add_argument("--max-jobs", type=int, default=256)
    read = commands.add_parser("read")
    read.add_argument("directory", type=Path)
    read.add_argument("node_id")
    read.add_argument("kind", choices=("group", "leaf", "either"))
    args = parser.parse_args()
    if args.command == "plan":
        tree = plan(
            json.loads(args.jobs.read_text()),
            json.loads(args.levels.read_text()),
            args.max_jobs,
        )
        if args.destination.exists():
            raise ValueError(
                "Hierarchy destination already exists; use a fresh plan directory"
            )
        args.destination.parent.mkdir(parents=True, exist_ok=True)
        # Publish a complete validated artifact directory, never partial output.
        with tempfile.TemporaryDirectory(dir=args.destination.parent) as temporary:
            staging = Path(temporary) / "tree"
            staging.mkdir()
            (staging / "tree.json").write_text(encode(tree))
            (staging / "preview.md").write_text(preview(tree))
            os.rename(staging, args.destination)
        print(encode(tree["roots"]))
    else:
        if not re.fullmatch(r"node-[0-9a-f]{24}", args.node_id):
            raise ValueError("Invalid node ID")
        tree = json.loads((args.directory / "tree.json").read_text())
        validate(tree, json.loads((args.directory.parent / "jobs.json").read_text()))
        node = tree["nodes"][args.node_id]
        if args.kind != "either" and node["kind"] != args.kind:
            raise ValueError("Unexpected node type")
        print(encode(node["values"]))


if __name__ == "__main__":
    try:
        main()
    except (
        ValueError,
        KeyError,
        TypeError,
        OSError,
        subprocess.SubprocessError,
    ) as error:
        sys.exit(f"Hierarchy planning/read failed: {error}")
