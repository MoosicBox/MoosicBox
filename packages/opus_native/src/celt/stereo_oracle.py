#!/usr/bin/env python3
"""Actual fixed-point stereo stages and mono/stereo PCM, unchanged C6 corpus."""
import contextlib
import ctypes as c
import io
import hashlib
import subprocess
import json
import pathlib
import re
import runpy

here = pathlib.Path(__file__).resolve().parent
with contextlib.redirect_stdout(io.StringIO()):
    api = runpy.run_path(str(here / "coarse_energy_oracle.py"))
globals().update({k: v for k, v in api.items() if not k.startswith("__")})
header = json.loads((here / "c6_before.jsonl").read_text().splitlines()[0])
corpus = pathlib.Path(header["root"])
uint = function("ec_dec_uint", c.c_uint, c.POINTER(EC), c.c_uint)
raw = function("ec_dec_bits", c.c_uint, c.POINTER(EC), c.c_uint)
alloc = function(
    "clt_compute_allocation",
    c.c_int,
    c.c_void_p,
    c.c_int,
    c.c_int,
    c.POINTER(c.c_int),
    c.POINTER(c.c_int),
    c.c_int,
    c.POINTER(c.c_int),
    c.POINTER(c.c_int),
    c.c_int,
    c.POINTER(c.c_int),
    c.POINTER(c.c_int),
    c.POINTER(c.c_int),
    c.POINTER(c.c_int),
    c.c_int,
    c.c_int,
    c.POINTER(EC),
    c.c_int,
    c.c_int,
    c.c_int,
)
all_bands = function(
    "quant_all_bands",
    None,
    c.c_int,
    c.c_void_p,
    c.c_int,
    c.c_int,
    c.POINTER(c.c_int16),
    c.c_void_p,
    c.POINTER(c.c_ubyte),
    c.c_void_p,
    c.POINTER(c.c_int),
    c.c_int,
    c.c_int,
    c.c_int,
    c.c_int,
    c.POINTER(c.c_int),
    c.c_int32,
    c.c_int32,
    c.POINTER(EC),
    c.c_int,
    c.c_int,
    c.POINTER(c.c_uint32),
    c.c_int,
    c.c_int,
    c.c_int,
)
anti_fn = function(
    "anti_collapse",
    None,
    c.c_void_p,
    c.POINTER(c.c_int16),
    c.POINTER(c.c_ubyte),
    c.c_int,
    c.c_int,
    c.c_int,
    c.c_int,
    c.c_int,
    c.POINTER(c.c_int32),
    c.POINTER(c.c_int32),
    c.POINTER(c.c_int32),
    c.POINTER(c.c_int),
    c.c_uint32,
    c.c_int,
    c.c_int,
)
create = function(
    "opus_decoder_create", c.c_void_p, c.c_int, c.c_int, c.POINTER(c.c_int)
)
decode = function(
    "opus_decode",
    c.c_int,
    c.c_void_p,
    c.c_void_p,
    c.c_int,
    c.POINTER(c.c_int16),
    c.c_int,
    c.c_int,
)
destroy = function("opus_decoder_destroy", None, c.c_void_p)
# The frozen corpus uses the non-RES24 CMake archive, not the RES24 research .so.
archive = (
    root
    / "target/debug/build/moosicbox_opus_native_libopus-149a3fe3b4ebb7e6/out/lib64/libopus.a"
)
assert (
    hashlib.sha256(archive.read_bytes()).hexdigest()
    == "82615f069ae57976e8ed283dc8c11c8860ca8978385386a938adbc7ce45d581c"
)
shared = here / "c6_reference.so"
subprocess.run(
    [
        "cc",
        "-shared",
        "-o",
        str(shared),
        "-Wl,--whole-archive",
        str(archive),
        "-Wl,--no-whole-archive",
        "-lm",
    ],
    check=True,
)
contract = c.CDLL(str(shared))
contract.opus_decoder_create.restype = c.c_void_p
contract.opus_decoder_create.argtypes = [c.c_int, c.c_int, c.POINTER(c.c_int)]
contract.opus_decode.restype = c.c_int
contract.opus_decode.argtypes = [
    c.c_void_p,
    c.c_void_p,
    c.c_int,
    c.POINTER(c.c_int16),
    c.c_int,
    c.c_int,
]
contract.opus_decoder_destroy.argtypes = [c.c_void_p]
cases = []
for case_id in [
    "sequences/celt_fb_stereo",
    "celt/fb/sine_stereo",
    "celt/wb/sine_stereo",
    "celt/swb/sine_stereo",
]:
    path = corpus / case_id
    folders = (
        json.loads((path / "sequence.json").read_text())["packets"]
        if case_id.startswith("sequences")
        else ["."]
    )
    history = (c.c_int32 * 42)()
    logs = (c.c_int32 * 42)(*[-28 * 2**24] * 42)
    logs2 = (c.c_int32 * 42)(*[-28 * 2**24] * 42)
    seed = c.c_uint32(0)
    contract_mono = contract.opus_decoder_create(48000, 1, c.byref(err))
    contract_stereo = contract.opus_decoder_create(48000, 2, c.byref(err))
    mono = create(48000, 1, c.byref(err))
    stereo = create(48000, 2, c.byref(err))
    for packet_index, folder in enumerate(folders):
        packet = (path / folder / "packet.bin").read_bytes()
        payload = packet[1:]
        lm = (packet[0] >> 3) & 3
        n = 120 << lm
        end = [13, 17, 19, 21][(packet[0] >> 5) - 4]
        buffer = c.create_string_buffer(payload)
        ec = EC()
        init(c.byref(ec), buffer, len(payload))
        assert not bit(c.byref(ec), 15)
        if bit(c.byref(ec), 1):
            octave = uint(c.byref(ec), 6)
            raw(c.byref(ec), 4 + octave)
            raw(c.byref(ec), 3)
            icdf(c.byref(ec), (c.c_ubyte * 3)(2, 1, 0), 2)
        transient = bit(c.byref(ec), 3) if lm > 0 else 0
        intra = bit(c.byref(ec), 3)
        coarse(mode, 0, end, history, intra, c.byref(ec), 2, lm)
        _, _, resolution, _ = tf_reference(ec, len(payload), lm, transient, end=end)
        ec = tf_reference(ec, len(payload), lm, transient, end=end, context=True)
        spread = icdf(c.byref(ec), (c.c_ubyte * 4)(25, 23, 2, 0), 5)
        with contextlib.redirect_stdout(io.StringIO()):
            offsets, caps, trim, available, ec = allocation_reference(
                ec, lm=lm, channels=2, end=end
            )
        anti = 8 if transient and lm >= 2 and available >= (lm + 2) * 8 else 0
        pulses = (c.c_int * 21)()
        fine = (c.c_int * 21)()
        priority = (c.c_int * 21)()
        intensity = c.c_int()
        dual = c.c_int()
        balance = c.c_int()
        coded = alloc(
            mode,
            0,
            end,
            (c.c_int * 21)(*offsets),
            (c.c_int * 21)(*caps),
            trim,
            c.byref(intensity),
            c.byref(dual),
            available - anti,
            c.byref(balance),
            pulses,
            fine,
            priority,
            2,
            lm,
            c.byref(ec),
            0,
            0,
            0,
        )
        fine_decode(mode, 0, end, history, fine, c.byref(ec), 2)
        x = (c.c_int16 * (2 * n))()
        masks = (c.c_ubyte * 42)()
        start_seed = seed.value
        all_bands(
            0,
            mode,
            0,
            end,
            x,
            c.byref(x, n * 2),
            masks,
            None,
            pulses,
            transient,
            spread,
            dual.value,
            intensity.value,
            (c.c_int * 21)(*(resolution + [0] * (21 - end))),
            len(payload) * 64 - anti,
            balance.value,
            c.byref(ec),
            lm,
            coded,
            c.byref(seed),
            0,
            0,
            0,
        )
        coefficients = list(x)
        entropy = [ec.rng, ec.val, ec.nbits_total - ec.rng.bit_length()]
        anti_on = raw(c.byref(ec), 1) if anti else 0
        final_decode(
            mode,
            0,
            end,
            history,
            fine,
            priority,
            len(payload) * 8 - (ec.nbits_total - ec.rng.bit_length()),
            c.byref(ec),
            2,
        )
        if anti_on:
            anti_fn(
                mode,
                x,
                masks,
                lm,
                2,
                n,
                0,
                end,
                history,
                logs,
                logs2,
                pulses,
                seed.value,
                0,
                0,
            )
        energy = list(history)
        # Downmix reference is from the complete decoder, not PCM averaging.
        packet_buffer = c.create_string_buffer(packet)
        pcm1 = (c.c_int16 * n)()
        pcm2 = (c.c_int16 * (n * 2))()
        assert decode(mono, packet_buffer, len(packet), pcm1, n, 0) == n
        assert decode(stereo, packet_buffer, len(packet), pcm2, n, 0) == n
        contract_pcm1 = (c.c_int16 * n)()
        contract_pcm2 = (c.c_int16 * (n * 2))()
        assert (
            contract.opus_decode(
                contract_mono, packet_buffer, len(packet), contract_pcm1, n, 0
            )
            == n
        )
        assert (
            contract.opus_decode(
                contract_stereo, packet_buffer, len(packet), contract_pcm2, n, 0
            )
            == n
        )
        expected = (
            (path / folder / "expected.pcm").read_bytes()
            if (path / folder / "expected.pcm").exists()
            else None
        )
        if expected is not None and len(expected) == n * 4:
            assert bytes(contract_pcm2) == expected
        cases.append(
            dict(
                id=case_id,
                index=packet_index,
                packet=list(packet),
                n=n,
                end=end,
                lm=lm,
                transient=transient,
                intensity=intensity.value,
                dual=dual.value,
                entropy=entropy,
                residual=coefficients,
                masks=list(masks),
                energy=energy,
                pcm_mono=list(pcm1),
                pcm_stereo=list(pcm2),
                contract_mono=list(contract_pcm1),
                contract_stereo=list(contract_pcm2),
            )
        )
        if not transient:
            for j in range(42):
                logs2[j] = logs[j]
                logs[j] = history[j]
        else:
            for j in range(42):
                logs[j] = min(logs[j], history[j])
        seed.value = ec.rng
        print(
            "STEREO",
            case_id,
            packet_index,
            "dual",
            dual.value,
            "intensity",
            intensity.value,
            "entropy",
            entropy,
        )
    destroy(mono)
    destroy(stereo)
    contract.opus_decoder_destroy(contract_mono)
    contract.opus_decoder_destroy(contract_stereo)
(here / "stereo_oracle.json").write_text(json.dumps(cases))
out = "// Literal actual upstream stereo reference stages and raw API PCM.\n"
out += "struct StereoCase {packet:&'static [u8], n:usize, end:usize, entropy:(u32,u32,u32), residual:&'static [i16], mono:&'static [i16], stereo:&'static [i16], reset:bool,contract_mono:&'static [i16],contract_stereo:&'static [i16]}\n"
out += "const STEREO_CASES: &[StereoCase] = &[\n"
for case in cases:
    out += (
        "StereoCase {packet:&"
        + str(case["packet"])
        + ",n:"
        + str(case["n"])
        + ",end:"
        + str(case["end"])
        + ",entropy:"
        + str(tuple(case["entropy"]))
        + ",residual:&"
        + str(case["residual"])
        + ",mono:&"
        + str(case["pcm_mono"])
        + ",stereo:&"
        + str(case["pcm_stereo"])
        + ",reset:"
        + str(case["index"] == 0).lower()
        + ",contract_mono:&"
        + str(case["contract_mono"])
        + ",contract_stereo:&"
        + str(case["contract_stereo"])
        + "},\n"
    )
out += "];\n"
out = re.sub(r"\b\d{5,}\b", lambda m: format(int(m[0]), "_d"), out)
(here / "stereo_expectations.rs").write_text(out)

shared.unlink()
