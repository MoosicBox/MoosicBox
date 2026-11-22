"""Index an already staged Opus evidence tree. This does NOT issue acceptance.

Pass paths relative to --root (a fresh private staging directory). Extract each
hosted execution artifact into its own platform subdirectory; preserve relative
paths in all receipts, listings and logs. Do not pass comparison summaries in
place of evidence. Run qualification_matrix_acceptance.py on finish-bundle.json.
"""
import argparse
import hashlib
import json
from pathlib import Path


def assemble(root, platform_directory, **reports):
    root = root.resolve(strict=True)

    def locate(name):
        if Path(name).is_absolute():
            raise ValueError(f"expected a bundle-relative path: {name}")
        path = (root / name).resolve(strict=True)
        return path, path.relative_to(root).as_posix()

    directory, relative = locate(platform_directory)
    if not directory.is_dir() or not any(directory.rglob("ci.json")):
        raise ValueError("raw platform execution directories with ci.json required")
    bundle = {"schema": "moosicbox.opus.finish-bundle.v1", "platform_directory": relative}
    for key, name in reports.items():
        path, relative = locate(name)
        data = path.read_bytes()
        report = json.loads(data)
        if key == "performance":
            # Record the input's claimed source, NOT a new stamp from this checkout.
            bundle["runtime_source_sha256"] = report["source_before_sha256"]
        bundle[key] = {"path": relative, "sha256": hashlib.sha256(data).hexdigest()}
    with (root / "finish-bundle.json").open("x", encoding="utf-8") as output:
        json.dump(bundle, output, indent=2, sort_keys=True)
        output.write("\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    for key in ("platform-directory", "performance", "public-silk", "unit-shards", "pipeline"):
        parser.add_argument("--" + key, required=True)
    assemble(**vars(parser.parse_args()))
