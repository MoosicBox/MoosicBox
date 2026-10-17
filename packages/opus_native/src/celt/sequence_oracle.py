#!/usr/bin/env python3
"""Persistent raw CELT corpus stage oracle, using verified bundled local symbols."""
import contextlib
import ctypes as c
import io
import json
import pathlib
import runpy

here = pathlib.Path(__file__).resolve().parent
with contextlib.redirect_stdout(io.StringIO()):
    api = runpy.run_path(str(here / "coarse_energy_oracle.py"))
globals().update({k: v for k, v in api.items() if not k.startswith("__")})
corpus = (
    root
    / "target/debug/build/moosicbox_opus_native_test_vectors-dba6b52440238931/out/generated/sequences/celt_fb_mono"
)
assert (
    corpus.is_dir()
), "Use the exact corpus root recorded in c4_after.jsonl run header"
# All API addresses and ABI bindings are hash checked by coarse_energy_oracle.py.
uint = function("ec_dec_uint", c.c_uint, c.POINTER(EC), c.c_uint)
raw = function("ec_dec_bits", c.c_uint, c.POINTER(EC), c.c_uint)
frac_tell = function("ec_tell_frac", c.c_uint, c.POINTER(EC))
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
synthesis = function(
    "celt_synthesis",
    None,
    c.c_void_p,
    c.POINTER(c.c_int16),
    c.POINTER(c.POINTER(c.c_int32)),
    c.POINTER(c.c_int32),
    c.c_int,
    c.c_int,
    c.c_int,
    c.c_int,
    c.c_int,
    c.c_int,
    c.c_int,
    c.c_int,
    c.c_int,
)
history = (c.c_int32 * 42)()
logs = (c.c_int32 * 42)(*[-28 * 2**24] * 42)
logs2 = (c.c_int32 * 42)(*[-28 * 2**24] * 42)
seed = c.c_uint32(0)
tail = [0] * 60
cases = []
for i, folder in enumerate(
    json.loads((corpus / "sequence.json").read_text())["packets"]
):
    packet = (corpus / folder / "packet.bin").read_bytes()
    assert packet[0] == 0xF8
    payload = packet[1:]
    buffer = c.create_string_buffer(payload)
    ec = EC()
    init(c.byref(ec), buffer, len(payload))
    assert not bit(c.byref(ec), 15)
    post = bit(c.byref(ec), 1)
    pitch = gain = tap = 0
    if post:
        octave = uint(c.byref(ec), 6)
        pitch = (16 << octave) + raw(c.byref(ec), 4 + octave) - 1
        gain = 3072 * (raw(c.byref(ec), 3) + 1)
        tap = icdf(c.byref(ec), (c.c_ubyte * 3)(2, 1, 0), 2)
    transient = bit(c.byref(ec), 3)
    intra = bit(c.byref(ec), 3)
    coarse(mode, 0, 21, history, intra, c.byref(ec), 1, 3)
    changes, select, resolution, _ = tf_reference(ec, len(payload), 3, transient)
    ec = tf_reference(ec, len(payload), 3, transient, context=True)
    spread = icdf(c.byref(ec), (c.c_ubyte * 4)(25, 23, 2, 0), 5)
    with contextlib.redirect_stdout(io.StringIO()):
        offsets, caps, trim, available, ec = allocation_reference(ec, lm=3)
    anti = 8 if transient and available >= 40 else 0
    pulses = (c.c_int * 21)()
    fine = (c.c_int * 21)()
    priority = (c.c_int * 21)()
    intensity = c.c_int()
    dual = c.c_int()
    balance = c.c_int()
    coded = alloc(
        mode,
        0,
        21,
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
        1,
        3,
        c.byref(ec),
        0,
        0,
        0,
    )
    fine_decode(mode, 0, 21, history, fine, c.byref(ec), 1)
    x = (c.c_int16 * 960)()
    masks = (c.c_ubyte * 21)()
    start_seed = seed.value
    all_bands(
        0,
        mode,
        0,
        21,
        x,
        None,
        masks,
        None,
        pulses,
        transient,
        spread,
        0,
        0,
        (c.c_int * 21)(*resolution),
        len(payload) * 64 - anti,
        balance.value,
        c.byref(ec),
        3,
        coded,
        c.byref(seed),
        0,
        0,
        0,
    )
    residual = list(x)
    entropy = [ec.rng, ec.val, ec.nbits_total - ec.rng.bit_length()]
    anti_on = raw(c.byref(ec), 1) if anti else 0
    final_decode(
        mode,
        0,
        21,
        history,
        fine,
        priority,
        len(payload) * 8 - (ec.nbits_total - ec.rng.bit_length()),
        c.byref(ec),
        1,
    )
    for j in range(21):
        history[j + 21] = history[j]
    if anti_on:
        anti_fn(
            mode,
            x,
            masks,
            3,
            1,
            960,
            0,
            21,
            history,
            logs,
            logs2,
            pulses,
            seed.value,
            0,
            0,
        )
    out = (c.c_int32 * 1020)(*tail)
    ptrs = (c.POINTER(c.c_int32) * 1)(out)
    synthesis(mode, x, ptrs, history, 0, 21, 1, 1, transient, 3, 1, 0, 0)
    tail = list(out)[960:]
    cases.append(
        dict(
            resolution=resolution,
            packet=list(packet),
            transient=transient,
            postfilter=[pitch, gain, tap],
            seed=start_seed,
            residual=residual,
            entropy=entropy,
            energy=list(history)[:21],
            masks=list(masks),
            anti_on=anti_on,
            anti_coefficients=list(x),
            synthesis=list(out)[:960],
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
        "SEQUENCE",
        i,
        "post",
        pitch,
        gain,
        tap,
        "transient",
        transient,
        "anti",
        anti_on,
        "entropy",
        entropy,
    )
(here / "sequence_oracle.json").write_text(json.dumps(cases))
# Rust literals for exact tests; no native result is involved in generation.
out = "// Generated by sequence_oracle.py against actual bundled fixed-point libopus.\n"
out += (
    "const SEQUENCE_PACKETS: [&[u8];4] = ["
    + ",".join("&" + str(x["packet"]) for x in cases)
    + "];\n"
)
for key, ty, width in [
    ("residual", "i16", 800),
    ("anti_coefficients", "i16", 800),
    ("energy", "i32", 21),
    ("synthesis", "i32", 960),
]:
    out += (
        f"const SEQUENCE_{key.upper()}: [[{ty};{width}];4] = ["
        + ",".join(str(x[key][:width]) for x in cases)
        + "];\n"
    )
out += (
    "const SEQUENCE_ENTROPY: [(u32,u32,u32);4] = ["
    + ",".join(str(tuple(x["entropy"])) for x in cases)
    + "];\n"
)
out = re.sub(r"\b\d{5,}\b", lambda m: format(int(m[0]), "_d"), out)
(here / "sequence_expectations.rs").write_text(out)
