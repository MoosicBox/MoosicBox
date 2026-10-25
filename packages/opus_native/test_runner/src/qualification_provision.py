"""Explicit checksum-pinned CI fixture provisioning; never called by normal tests.

Offline copy is the default. Network requires --download and an explicit HTTPS
URL for the already-reviewed derived music WAV, not a newly encoded replacement.
No uploads, automatic conversion or cache fallback.
"""

import argparse
from pathlib import Path
import shutil
import tempfile
from urllib.parse import urlparse
from urllib.request import urlopen

from qualification_inputs import verify_inputs
import json
from qualification_inputs import MANIFEST


def download(url, path, expected_bytes):
    if urlparse(url).scheme != "https":
        raise ValueError("fixture downloads require HTTPS")
    with urlopen(url, timeout=30) as response:
        if urlparse(response.geturl()).scheme != "https":
            raise ValueError("fixture download redirected away from HTTPS")
        data = response.read(expected_bytes + 1)
    if len(data) != expected_bytes:
        raise ValueError("fixture download size differs from reviewed bytes")
    with path.open("xb") as output:
        output.write(data)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--source-directory", type=Path)
    source.add_argument("--download", action="store_true")
    parser.add_argument(
        "--music-wav-url", help="explicit HTTPS location of exact reviewed derived WAV"
    )
    parser.add_argument("--destination", type=Path, required=True)
    args = parser.parse_args()
    if args.download and not args.music_wav_url:
        parser.error(
            "--download requires --music-wav-url; no reviewed hosted WAV URL is inferred"
        )
    if args.music_wav_url and not args.download:
        parser.error("--music-wav-url requires explicit --download")
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    with tempfile.TemporaryDirectory(prefix="opus-reviewed-media-") as directory:
        staging = Path(directory)
        if args.download:
            for label, entry in manifest["media"].items():
                url = entry["source_url"] if label == "speech" else args.music_wav_url
                download(url, staging / entry["file"], entry["bytes"])
        else:
            _, media = verify_inputs(args.source_directory)
            for label, path in media.items():
                shutil.copyfile(path, staging / manifest["media"][label]["file"])
        verify_inputs(staging)
        # Exclusive directory creation; never replace a caller's cache or fixture.
        args.destination.mkdir(parents=False, exist_ok=False)
        for entry in manifest["media"].values():
            with (staging / entry["file"]).open("rb") as source_file:
                with (args.destination / entry["file"]).open("xb") as destination_file:
                    shutil.copyfileobj(source_file, destination_file)
        verify_inputs(args.destination)
    print("Verified reviewed speech/music fixtures: " + str(args.destination))


if __name__ == "__main__":
    main()
