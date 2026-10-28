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

# Also supports importlib-based tests and local preview invocation.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from shard_capacity import (
    MAX_GROUP_LEVELS,
    GITHUB_WORKFLOW_LEVELS,
    capacity,
    dispatcher_name,
)

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


def configuration(levels, default, caller_depth=1):
    limit_value(default)
    available = capacity(caller_depth)
    if not isinstance(levels, list) or not levels:
        raise ValueError("shard-levels must be a nonempty array")
    if len(levels) > available:
        raise ValueError(
            f"Requested {len(levels)} grouping levels; only {available} available: "
            f"GitHub limit {GITHUB_WORKFLOW_LEVELS} minus caller depth {caller_depth} minus executor 1"
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


def plan(jobs, levels, default=256, caller_depth=1):
    configuration(levels, default, caller_depth)
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
        or not 1 <= tree.get("depth") <= MAX_GROUP_LEVELS
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


def routing(tree, jobs):
    """Inline bounded routing only; full execution metadata stays in the artifact."""
    validate(tree, jobs)
    indices = {}
    for index, job in enumerate(jobs):
        indices.setdefault(encode(job), []).append(index)

    def visit(descriptor):
        node = tree["nodes"][descriptor["id"]]
        result = dict(descriptor)
        if node["kind"] == "group":
            result["children"] = [visit(child) for child in node["values"]]
        else:
            result["jobs"] = []
            for job in node["values"]:
                index = indices[encode(job)].pop(0)
                label = job.get("name", "Build")
                if job.get("hasMultipleOs"):
                    label += " " + job.get("osEmoji", "")
                if job.get("osCount", 0) > 1:
                    label += " " + str(job.get("osIndex", ""))
                label += job.get("stackSuffix", "")
                result["jobs"].append(
                    dict(
                        index=index,
                        os=job["os"],
                        name=label,
                        gitSubmodules=job.get("gitSubmodules", False),
                    )
                )
        values = result.get("children", result.get("jobs"))
        if not fits(values, node["limit"]):
            raise ValueError(
                "Inline routing exceeds matrix transport budget; reduce names/groups or use flat sharding"
            )
        # Account for JSON escaping when forwarded as a workflow input string.
        if (
            len(encode(json.dumps(result, ensure_ascii=False, indent=2)).encode())
            > PAYLOAD
        ):
            raise ValueError(
                "Inline routing input exceeds escaped payload budget; use flat sharding or reduce hierarchy"
            )
        return result

    roots = [visit(root) for root in tree["roots"]]
    if not fits(roots, tree["matrix_max_jobs"]):
        raise ValueError(
            "Inline root routing exceeds transport budget; use flat sharding"
        )
    return roots


def resolve_job(directory, node_id, reference):
    """Validate transport references on the actual build runner, before setup."""
    if not re.fullmatch(r"node-[0-9a-f]{24}", node_id):
        raise ValueError("Invalid node ID")
    tree = json.loads((directory / "tree.json").read_text())
    jobs = json.loads((directory.parent / "jobs.json").read_text())
    roots = routing(tree, jobs)

    def find(items):
        for item in items:
            if item["id"] == node_id:
                return item
            found = find(item.get("children", []))
            if found is not None:
                return found
        return None

    leaf = find(roots)
    if not isinstance(reference, dict) or type(reference.get("index")) is not int:
        raise ValueError("Invalid job reference/index")
    if leaf is None or leaf["kind"] != "leaf" or reference not in leaf["jobs"]:
        raise ValueError("Unexpected, altered or misplaced job reference")
    index = reference["index"]
    if type(index) is not int:
        raise ValueError("Invalid job index")
    return jobs[index]


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
        "Preparation jobs: 0 (inline routing). Concurrency is controlled separately.",
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
    create.add_argument("--caller-depth", type=int, default=1)
    entry = commands.add_parser("entry")
    entry.add_argument("--caller-depth", type=int, default=1)
    resolve = commands.add_parser("resolve")
    resolve.add_argument("directory", type=Path)
    resolve.add_argument("node_id")
    resolve.add_argument("reference", help="JSON job reference")
    read = commands.add_parser("read")
    read.add_argument("directory", type=Path)
    read.add_argument("node_id")
    read.add_argument("kind", choices=("group", "leaf", "either"))
    read.add_argument("--remaining-levels", type=int, default=MAX_GROUP_LEVELS)
    args = parser.parse_args()
    if args.command == "entry":
        capacity(args.caller_depth)
        print(dispatcher_name(args.caller_depth))
        return
    if args.command == "resolve":
        print(
            encode(
                resolve_job(args.directory, args.node_id, json.loads(args.reference))
            )
        )
        return
    if args.command == "plan":
        tree = plan(
            json.loads(args.jobs.read_text()),
            json.loads(args.levels.read_text()),
            args.max_jobs,
            args.caller_depth,
        )
        roots = routing(tree, json.loads(args.jobs.read_text()))
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
            (staging / "routing.json").write_text(encode(roots))
            (staging / "preview.md").write_text(preview(tree))
            os.rename(staging, args.destination)
        print(encode(roots))
    else:
        if not re.fullmatch(r"node-[0-9a-f]{24}", args.node_id):
            raise ValueError("Invalid node ID")
        tree = json.loads((args.directory / "tree.json").read_text())
        validate(tree, json.loads((args.directory.parent / "jobs.json").read_text()))
        node = tree["nodes"][args.node_id]
        if (
            type(args.remaining_levels) is not int
            or not 1 <= args.remaining_levels <= MAX_GROUP_LEVELS
        ):
            raise ValueError("Invalid dispatcher capacity")
        required = tree["depth"] - node["level"] + 1
        if required > args.remaining_levels:
            raise ValueError(
                f"Node requires {required} grouping levels; this static entry supports {args.remaining_levels}"
            )
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
