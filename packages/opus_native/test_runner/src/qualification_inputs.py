"""Fail-closed qualification inputs and acceptance; no downloads or fixture writes."""

import hashlib
import json
import math
from pathlib import Path
import wave

MANIFEST = (
    Path(__file__).resolve().parents[1] / "fixtures" / "qualification_inputs.json"
)


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def verify_inputs(directory):
    """Verify the reviewed WAV bytes before any codec process is launched."""
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    if manifest["schema"] != "moosicbox.opus.qualification-inputs.v1":
        raise ValueError("unsupported qualification input schema")
    result = {}
    for label, entry in manifest["media"].items():
        path = Path(directory) / entry["file"]
        if path.stat().st_size != entry["bytes"] or sha256(path) != entry["sha256"]:
            raise ValueError(f"unreviewed media bytes: {label}")
        with wave.open(str(path), "rb") as wav:
            if (
                wav.getframerate(),
                wav.getnchannels(),
                wav.getsampwidth(),
                wav.getcomptype(),
            ) != (entry["rate"], entry["channels"], 2, "NONE"):
                raise ValueError(f"unexpected WAV format: {label}")
        result[label] = path.resolve(strict=True)
    return manifest, result


def verify_reference(manifest, expected):
    """Validate portable reference policy, not a machine-specific binary hash."""
    if (
        manifest.get("schema") != expected["reference_schema"]
        or manifest.get("revision") != expected["reference_revision"]
        or manifest.get("tracked_clean") is not True
        or manifest.get("fixed_point") is not True
        or any(
            manifest.get(key) is not False
            for key in ("float_api", "intrinsics", "qext", "res24")
        )
    ):
        raise ValueError(
            "reference does not satisfy reviewed portable fixed-point policy"
        )


def verify_build_receipt(path, source_hash, native_hash, reference_hash):
    """Bind an explicitly retained coherent build observation to tested images.

    This is local evidence supplied by the build owner, not signed attestation.
    Before/after source identities must be captured around the actual build.
    """
    receipt = json.loads(Path(path).read_text(encoding="utf-8"))
    expected = {
        "schema": "moosicbox.opus.qualification-build.v1",
        "source_before_sha256": source_hash,
        "source_after_sha256": source_hash,
        "native_sha256": native_hash,
        "reference_sha256": reference_hash,
    }
    if any(receipt.get(key) != value for key, value in expected.items()):
        raise ValueError(
            "build receipt does not match current source and executable identities"
        )
    if not receipt.get("command") or not receipt.get("profile"):
        raise ValueError("build receipt requires exact command and profile")
    return receipt


def quality_status(snr_db, maximum_parity_error):
    if not math.isfinite(snr_db) or not math.isfinite(maximum_parity_error):
        return "nonfinite_quality_metric"
    if maximum_parity_error > 2:
        return "native_reference_error_above_2lsb"
    if snr_db <= 3:
        return "waveform_snr_not_above_3db"
    return "pass"


def accepted(failures, media_missing, artifact_stable, source_changed, build_verified):
    return (
        failures == 0
        and not media_missing
        and artifact_stable
        and not source_changed
        and build_verified
    )
