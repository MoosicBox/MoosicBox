"""Stable eight-packet raw API diagnostic. Executables are explicit, never inferred."""

import argparse, datetime, hashlib, json, pathlib, struct, subprocess

p = argparse.ArgumentParser()
p.add_argument("--native", required=True)
p.add_argument("--generate", required=True)
p.add_argument("--reference", required=True)
p.add_argument("--archive", required=True, type=pathlib.Path)
p.add_argument("--streams", required=True, type=pathlib.Path)
p.add_argument("--create", action="store_true")
a = p.parse_args()
here = pathlib.Path(__file__).resolve().parent
if (
    hashlib.sha256(a.archive.read_bytes()).hexdigest()
    != "4f2a11b2264e7172169207a29a35e624bda97fdfbd3e2cdb4026bff658b4d500"
):
    raise ValueError("Pinned archive mismatch")
sha = lambda b: hashlib.sha256(b).hexdigest()
print(
    json.dumps(
        {
            "type": "provenance",
            "archive_sha256": sha(a.archive.read_bytes()),
            "utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "sources": {p.name: sha(p.read_bytes()) for p in sorted(here.glob("*.rs"))},
            "drivers": {
                p.name: sha(p.read_bytes()) for p in sorted(here.glob("matrix*"))
            },
            "executables": {
                x: sha(pathlib.Path(x).read_bytes())
                for x in [a.native, a.generate, a.reference]
            },
        }
    )
)
a.streams.mkdir(parents=True, exist_ok=True)


def frames(data, ch):
    result = []
    offset = 0
    while offset < len(data):
        n = struct.unpack_from("<I", data, offset)[0]
        offset += 4
        values = struct.unpack_from("<" + "h" * (n * ch), data, offset)
        offset += 2 * n * ch
        result.append(values)
    if len(result) != 8:
        raise ValueError("wrong packet count")
    return result


for rate in [8000, 12000, 16000]:
    for channels in [1, 2]:
        for ms in [10, 20, 40, 60]:
            path = a.streams / f"{rate}-{channels}-{ms}.packets"
            if a.create:
                run = subprocess.run(
                    [a.generate, str(rate), str(channels), str(ms)],
                    capture_output=True,
                    check=True,
                )
                path.write_bytes(run.stdout)
            data = path.read_bytes()
            reference = subprocess.run(
                [a.reference, str(rate), str(channels)],
                input=data,
                capture_output=True,
                check=True,
            ).stdout
            native = subprocess.run(
                [a.native, str(rate), str(channels)], input=data, capture_output=True
            )
            record = {
                "type": "case",
                "rate": rate,
                "channels": channels,
                "ms": ms,
                "packet_sha256": sha(data),
                "reference_sha256": sha(reference),
                "native_sha256": sha(native.stdout),
                "exit": native.returncode,
            }
            if native.returncode:
                record["error"] = native.stderr.decode(errors="replace")[-1000:]
            else:
                c = frames(reference, channels)
                r = frames(native.stdout, channels)
                record["counts"] = [
                    [len(x) // channels, len(y) // channels] for x, y in zip(c, r)
                ]
                record["mismatches"] = sum(
                    sum(x != y for x, y in zip(cf, rf)) + abs(len(cf) - len(rf))
                    for cf, rf in zip(c, r)
                )
                record["first"] = next(
                    (
                        [i, j, x, y]
                        for i, (cf, rf) in enumerate(zip(c, r))
                        for j, (x, y) in enumerate(zip(cf, rf))
                        if x != y
                    ),
                    None,
                )
            print(json.dumps(record), flush=True)
