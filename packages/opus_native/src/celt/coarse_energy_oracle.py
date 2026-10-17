#!/usr/bin/env python3
"""Read-only coarse-energy oracle using the bundled fixed-point libopus.

Run from the repository root. Resolves existing local symbols with nm; does not
rebuild or instrument libopus. Decoder layout follows celt/entcode.h. Outputs
literal Rust expectations, not values computed by the native implementation.
"""
import ctypes as c
import hashlib
import pathlib
import re
import subprocess

root = pathlib.Path(__file__).resolve().parents[4]
path = root / "packages/opus_native/libopus/opus/.libs/libopus.so"
expected_sha256 = "be5a822ed771053ee32cecf4ba5a1c312e776792761f9d292d733a0828e72def"
assert (
    hashlib.sha256(path.read_bytes()).hexdigest() == expected_sha256
), "Reference binary changed: reverify fixed-point configuration and internal ABI"
print("REFERENCE SHA256", expected_sha256)
lib = c.CDLL(str(path))
symbols = {}
for line in subprocess.check_output(["nm", str(path)], text=True).splitlines():
    fields = line.split()
    if len(fields) == 3:
        symbols[fields[2]] = int(fields[0], 16)
base = c.cast(lib.opus_decode, c.c_void_p).value - symbols["opus_decode"]


class EC(c.Structure):
    _fields_ = [
        ("buf", c.c_void_p),
        ("storage", c.c_uint),
        ("end_offs", c.c_uint),
        ("end_window", c.c_uint),
        ("nend_bits", c.c_int),
        ("nbits_total", c.c_int),
        ("offs", c.c_uint),
        ("rng", c.c_uint),
        ("val", c.c_uint),
        ("ext", c.c_uint),
        ("rem", c.c_int),
        ("error", c.c_int),
    ]


def function(name, result, *args):
    return c.CFUNCTYPE(result, *args)(base + symbols[name])


init = function("ec_dec_init", None, c.POINTER(EC), c.c_void_p, c.c_uint)
bit = function("ec_dec_bit_logp", c.c_int, c.POINTER(EC), c.c_uint)
uint = function("ec_dec_uint", c.c_uint, c.POINTER(EC), c.c_uint)
bits = function("ec_dec_bits", c.c_uint, c.POINTER(EC), c.c_uint)
icdf = function("ec_dec_icdf", c.c_int, c.POINTER(EC), c.POINTER(c.c_ubyte), c.c_uint)
mode_create = function(
    "opus_custom_mode_create", c.c_void_p, c.c_int, c.c_int, c.POINTER(c.c_int)
)
coarse = function(
    "unquant_coarse_energy",
    None,
    c.c_void_p,
    c.c_int,
    c.c_int,
    c.POINTER(c.c_int32),
    c.c_int,
    c.POINTER(EC),
    c.c_int,
    c.c_int,
)
err = c.c_int()
mode = mode_create(48000, 960, c.byref(err))
assert mode and err.value == 0
source = (pathlib.Path(__file__).parent / "decoder.rs").read_text()
packets = re.findall(r'"(f0[0-9a-f]{118})"', source)
assert len(packets) == 3


def tf_reference(ec, frame_bytes, lm, transient, start=0, end=21, context=False):
    reference = EC.from_buffer_copy(ec)
    logp = 2 if transient else 4
    budget = frame_bytes * 8
    tell = reference.nbits_total - reference.rng.bit_length()
    reserved = lm > 0 and tell + logp + 1 <= budget
    budget -= reserved
    curr = changed = 0
    changes = []
    for band in range(start, end):
        if tell + logp <= budget:
            curr ^= bit(c.byref(reference), logp)
            tell = reference.nbits_total - reference.rng.bit_length()
            changed |= curr
        changes.append(curr)
        logp = 4 if transient else 5
    table_source = (root / "packages/opus_native/libopus/opus/celt/celt.c").read_text()
    table_text = table_source.split("tf_select_table[4][8] = {", 1)[1].split("};", 1)[0]
    table_text = re.sub(r"/\*.*?\*/", "", table_text, flags=re.S)
    table = list(map(int, re.findall(r"-?\d+", table_text)))[8 * lm : 8 * lm + 8]
    select = 0
    if (
        reserved
        and table[4 * transient + changed] != table[4 * transient + 2 + changed]
    ):
        select = bit(c.byref(reference), 1)
    resolution = [table[4 * transient + 2 * select + v] for v in changes]
    if context:
        return reference
    return (
        changes,
        select,
        resolution,
        (
            reference.rng,
            reference.val,
            reference.nbits_total - reference.rng.bit_length(),
        ),
    )


def allocation_reference(ec, lm=2, channels=1, start=0, end=21):
    source = (
        root / "packages/opus_native/libopus/opus/celt/static_modes_fixed.h"
    ).read_text()
    caps_text = source.split("cache_caps50[168] = {", 1)[1].split("}", 1)[0]
    caps_table = list(map(int, re.findall(r"\d+", caps_text)))
    edges = [
        0,
        1,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        10,
        12,
        14,
        16,
        20,
        24,
        28,
        34,
        40,
        48,
        60,
        78,
        100,
    ]
    widths = [channels * (edges[i + 1] - edges[i]) * (1 << lm) for i in range(21)]
    caps = [
        (caps_table[21 * (2 * lm + channels - 1) + i] + 64) * widths[i] // 4
        for i in range(21)
    ]
    ec_tell_frac = function("ec_tell_frac", c.c_uint, c.POINTER(EC))
    total = ec.storage * 64
    logp = 6
    offsets = [0] * 21
    for band in range(start, end):
        quantum = min(widths[band] * 8, max(48, widths[band]))
        loop_logp = logp
        while (
            ec_tell_frac(c.byref(ec)) + loop_logp * 8 < total
            and offsets[band] < caps[band]
        ):
            if not bit(c.byref(ec), loop_logp):
                break
            offsets[band] += quantum
            total -= quantum
            loop_logp = 1
        if offsets[band] > 0:
            logp = max(2, logp - 1)
    trim = (
        icdf(
            c.byref(ec), (c.c_ubyte * 11)(126, 124, 119, 109, 87, 41, 19, 9, 4, 2, 0), 7
        )
        if ec_tell_frac(c.byref(ec)) + 48 <= total
        else 5
    )
    print(
        "ORACLE dynalloc",
        offsets,
        trim,
        (ec.rng, ec.val, ec.nbits_total - ec.rng.bit_length()),
    )
    available = ec.storage * 64 - ec_tell_frac(c.byref(ec)) - 1
    anti = 8 if lm >= 2 and available >= (lm + 2) * 8 else 0
    # caller overrides anti for non-transient frames below
    return offsets, caps, trim, available, ec


class BandCtx(c.Structure):
    _fields_ = [
        ("encode", c.c_int),
        ("resynth", c.c_int),
        ("mode", c.c_void_p),
        ("i", c.c_int),
        ("intensity", c.c_int),
        ("spread", c.c_int),
        ("tf_change", c.c_int),
        ("ec", c.POINTER(EC)),
        ("remaining_bits", c.c_int32),
        ("bandE", c.c_void_p),
        ("seed", c.c_uint32),
        ("arch", c.c_int),
        ("theta_round", c.c_int),
        ("disable_inv", c.c_int),
        ("avoid_split_noise", c.c_int),
    ]


residual_cases = []


def residual_reference(ec, pulses, fine_energy, coded, spread, transient, packet_index):
    import json

    saved = EC.from_buffer_copy(ec)
    x = (c.c_int16 * 480)()
    masks = (c.c_ubyte * 21)()
    seed = c.c_uint32(0)
    tf = (c.c_int * 21)()
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
        tf,
        ec.storage * 64 - (8 if transient else 0),
        0,
        c.byref(ec),
        2,
        coded,
        c.byref(seed),
        0,
        0,
        0,
    )
    case = dict(
        packet_index=packet_index,
        coefficients=list(x),
        masks=list(masks),
        seed=seed.value,
        entropy=[ec.rng, ec.val, ec.nbits_total - ec.rng.bit_length()],
        bands=[],
    )
    # Trace each band through the same actual quant_band used by quant_all_bands.
    band = function(
        "quant_band",
        c.c_uint,
        c.POINTER(BandCtx),
        c.POINTER(c.c_int16),
        c.c_int,
        c.c_int,
        c.c_int,
        c.c_void_p,
        c.c_int,
        c.c_void_p,
        c.c_int32,
        c.c_void_p,
        c.c_int,
    )
    sqrt = function("celt_sqrt", c.c_int32, c.c_int32)
    ec = EC.from_buffer_copy(saved)
    ctx = BandCtx(
        0, 1, mode, 0, 0, spread, 0, c.pointer(ec), 0, None, 0, 0, 0, 0, transient
    )
    norm = (c.c_int16 * 480)()
    edges = [
        0,
        1,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        10,
        12,
        14,
        16,
        20,
        24,
        28,
        34,
        40,
        48,
        60,
        78,
        100,
    ]
    ec_frac = function("ec_tell_frac", c.c_uint, c.POINTER(EC))
    balance = 0
    lowband_offset = 0
    update = True
    bcount = 4 if transient else 1
    cm = [0] * 21
    for i in range(21):
        n = 4 * (edges[i + 1] - edges[i])
        offset = 4 * edges[i]
        tell = ec_frac(c.byref(ec))
        if i:
            balance -= tell
        ctx.remaining_bits = ec.storage * 64 - (8 if transient else 0) - tell - 1
        b = (
            max(
                0,
                min(
                    16383,
                    ctx.remaining_bits + 1,
                    pulses[i] + int(balance / min(3, coded - i)),
                ),
            )
            if i < coded
            else 0
        )
        if offset - n >= 0 and (update or lowband_offset == 0):
            lowband_offset = i
        effective = -1
        fill = (1 << bcount) - 1
        if lowband_offset and (spread != 3 or bcount > 1):
            effective = max(0, 4 * edges[lowband_offset] - n)
            fold_start = lowband_offset - 1
            while 4 * edges[fold_start] > effective:
                fold_start -= 1
            fold_end = lowband_offset
            while fold_end < i and 4 * edges[fold_end] < effective + n:
                fold_end += 1
            fill = 0
            for j in range(fold_start, fold_end):
                fill |= cm[j]
        low = c.byref(norm, 2 * effective) if effective >= 0 else None
        output = (c.c_int16 * n)()
        low_out = c.byref(norm, 2 * offset) if i != 20 else None
        scratch = (c.c_int16 * n)()
        ctx.i = i
        cm[i] = band(
            c.byref(ctx),
            output,
            n,
            b,
            bcount,
            low,
            2,
            low_out,
            2147483647,
            scratch if i != 20 else None,
            fill,
        )
        case["bands"].append(
            dict(
                bits=b,
                coefficients=list(output),
                mask=cm[i],
                entropy=[ec.rng, ec.val, ec.nbits_total - ec.rng.bit_length()],
            )
        )
        balance += pulses[i] + tell
        update = b > n * 8
        ctx.avoid_split_noise = 0
    assert [v for b in case["bands"] for v in b["coefficients"]] == case[
        "coefficients"
    ][:400]
    assert cm == case["masks"]
    assert case["entropy"] == [ec.rng, ec.val, ec.nbits_total - ec.rng.bit_length()]
    # Reference actual celt_synthesis/MDCT on nonzero real-payload frequencies.
    denormalize = function(
        "denormalise_bands",
        None,
        c.c_void_p,
        c.POINTER(c.c_int16),
        c.POINTER(c.c_int32),
        c.POINTER(c.c_int32),
        c.c_int,
        c.c_int,
        c.c_int,
        c.c_int,
        c.c_int,
    )
    freq = (c.c_int32 * 480)()
    denormalize(mode, x, freq, fine_energy, 0, 21, 4, 1, 0)
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
    out = (c.c_int32 * 540)()
    ptrs = (c.POINTER(c.c_int32) * 1)(out)
    synthesis(mode, x, ptrs, fine_energy, 0, 21, 1, 1, transient, 2, 1, 0, 0)
    case["frequency"] = list(freq)
    case["synthesis"] = list(out)
    # C4's flat per-band budgets before the mono rebalance repair. First bands
    # are scalar leaves (N=4), so actual alg_unquant isolates budget divergence.
    flat_ec = EC.from_buffer_copy(saved)
    flat_alg = function(
        "alg_unquant",
        c.c_uint,
        c.POINTER(c.c_int16),
        c.c_int,
        c.c_int,
        c.c_int,
        c.c_int,
        c.POINTER(EC),
        c.c_int32,
    )
    cache_source = (
        root / "packages/opus_native/libopus/opus/celt/static_modes_fixed.h"
    ).read_text()

    def cached_table(name, length):
        return list(
            map(
                int,
                re.findall(
                    r"-?\d+",
                    cache_source.split(f"{name}[{length}] = {{", 1)[1].split("}", 1)[0],
                ),
            )
        )

    indices = cached_table("cache_index50", 105)
    cache_values = cached_table("cache_bits50", 392)
    for band_index in range(12):
        cache = cache_values[indices[63 + band_index] :]
        target = pulses[band_index] - 1
        lo = 0
        hi = cache[0]
        for _ in range(6):
            mid = (lo + hi + 1) // 2
            if cache[mid] >= target:
                hi = mid
            else:
                lo = mid
        q = lo if target - (-1 if lo == 0 else cache[lo]) <= cache[hi] - target else hi
        k = q if q < 8 else (8 + (q & 7)) << ((q >> 3) - 1)
        dimensions = 4 * ([1] * 8 + [2] * 4)[band_index]
        coefficients = (c.c_int16 * dimensions)()
        flat_alg(
            coefficients, dimensions, k, spread, bcount, c.byref(flat_ec), 2147483647
        )
        actual = [
            flat_ec.rng,
            flat_ec.val,
            flat_ec.nbits_total - flat_ec.rng.bit_length(),
        ]
        if actual != case["bands"][band_index]["entropy"]:
            print(
                "ORACLE C4 first flat range divergence",
                packet_index,
                "band",
                band_index,
                "flat_bits",
                pulses[band_index],
                "correct_bits",
                case["bands"][band_index]["bits"],
                "flat",
                actual,
                "correct",
                case["bands"][band_index]["entropy"],
            )
            break
    residual_cases.append(case)
    (root / "packages/opus_native/src/celt/residual_oracle.json").write_text(
        json.dumps(residual_cases)
    )
    print("ORACLE full residual", packet_index, case["entropy"], "seed", case["seed"])


history = (c.c_int32 * 42)()
for index, packet in enumerate(packets):
    data = c.create_string_buffer(bytes.fromhex(packet)[1:])
    ec = EC()
    init(c.byref(ec), data, len(data) - 1)
    assert bit(c.byref(ec), 15) == 0
    assert bit(c.byref(ec), 1) == 1
    octave = uint(c.byref(ec), 6)
    bits(c.byref(ec), 4 + octave)
    bits(c.byref(ec), 3)
    icdf(c.byref(ec), (c.c_ubyte * 3)(2, 1, 0), 2)
    bit(c.byref(ec), 3)
    intra = bit(c.byref(ec), 3)
    coarse(mode, 0, 21, history, intra, c.byref(ec), 1, 2)
    print(
        "ORACLE continuous",
        index,
        list(history)[:21],
        (ec.rng, ec.val, ec.nbits_total - ec.rng.bit_length()),
    )
    if index == 0:
        # Compare the first TF symbol only; do not change history for next frame.
        reference = EC.from_buffer_copy(ec)
        symbol = bit(c.byref(reference), 2)
        native_equivalent = EC.from_buffer_copy(ec)
        wrong_symbol = icdf(c.byref(native_equivalent), (c.c_ubyte * 3)(4, 1, 0), 8)
        print(
            "ORACLE first TF",
            symbol,
            (reference.rng, reference.val),
            "current native ICDF",
            wrong_symbol,
            (native_equivalent.rng, native_equivalent.val),
        )
    result = tf_reference(ec, ec.storage, 2, int(index == 0))
    print("ORACLE TF", index, *result)
    after_tf = tf_reference(ec, ec.storage, 2, int(index == 0), context=True)
    wrong = EC.from_buffer_copy(after_tf)
    correct_symbol = icdf(c.byref(after_tf), (c.c_ubyte * 4)(25, 23, 2, 0), 5)
    wrong_symbol = icdf(c.byref(wrong), (c.c_ubyte * 5)(32, 25, 23, 2, 0), 5)
    print(
        "ORACLE spread",
        index,
        correct_symbol,
        (after_tf.rng, after_tf.val),
        "native",
        wrong_symbol,
        (wrong.rng, wrong.val),
    )
    offsets, caps, trim, available, alloc_ec = allocation_reference(
        EC.from_buffer_copy(after_tf)
    )
    anti = 8 if index == 0 else 0
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
    intensity = c.c_int()
    dual = c.c_int()
    balance = c.c_int()
    pulses = (c.c_int * 21)()
    fine = (c.c_int * 21)()
    priority = (c.c_int * 21)()
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
        2,
        c.byref(alloc_ec),
        0,
        0,
        0,
    )
    print(
        "ORACLE allocation",
        index,
        "coded",
        coded,
        "balance",
        balance.value,
        "shape",
        list(pulses),
        "fine",
        list(fine),
        "priority",
        list(priority),
        "entropy",
        (alloc_ec.rng, alloc_ec.val, alloc_ec.nbits_total - alloc_ec.rng.bit_length()),
    )
    fine_fn = function(
        "unquant_fine_energy",
        None,
        c.c_void_p,
        c.c_int,
        c.c_int,
        c.POINTER(c.c_int32),
        c.POINTER(c.c_int),
        c.POINTER(EC),
        c.c_int,
    )
    fine_history = (c.c_int32 * 42)(*history)
    fine_fn(mode, 0, 21, fine_history, fine, c.byref(alloc_ec), 1)
    residual_reference(
        EC.from_buffer_copy(alloc_ec),
        pulses,
        fine_history,
        coded,
        correct_symbol,
        index == 0,
        index,
    )
    src = (
        root / "packages/opus_native/libopus/opus/celt/static_modes_fixed.h"
    ).read_text()

    def table(name, length):
        return list(
            map(
                int,
                re.findall(
                    r"-?\d+", src.split(f"{name}[{length}] = {{", 1)[1].split("}", 1)[0]
                ),
            )
        )

    cache_index = table("cache_index50", 105)
    cache_bits = table("cache_bits50", 392)
    cache = cache_bits[cache_index[63] :]
    target = pulses[0] - 1
    lo = 0
    hi = cache[0]
    for _ in range(6):
        mid = (lo + hi + 1) // 2
        if cache[mid] >= target:
            hi = mid
        else:
            lo = mid
    q = lo if target - (-1 if lo == 0 else cache[lo]) <= cache[hi] - target else hi
    k = q if q < 8 else (8 + (q & 7)) << ((q >> 3) - 1)
    alg_fn = function(
        "alg_unquant",
        c.c_uint,
        c.POINTER(c.c_int16),
        c.c_int,
        c.c_int,
        c.c_int,
        c.c_int,
        c.POINTER(EC),
        c.c_int32,
    )
    first = (c.c_int16 * 4)()
    alg_fn(
        first,
        4,
        k,
        correct_symbol,
        4 if index == 0 else 1,
        c.byref(alloc_ec),
        2147483647,
    )
    print(
        "ORACLE first residual",
        index,
        "K",
        k,
        list(first),
        (alloc_ec.rng, alloc_ec.val, alloc_ec.nbits_total - alloc_ec.rng.bit_length()),
    )

# Truncated payloads intentionally exercise the reference budget fallbacks;
# these are stage tests, not assertions that the packets are valid Opus.
for length, start, end, channels, lm, intra in [
    (2, 8, 17, 1, 0, 0),
    (3, 17, 21, 2, 3, 1),
    (8, 0, 21, 2, 1, 0),
]:
    payload = bytes.fromhex(packets[0])[1 : 1 + length]
    data = c.create_string_buffer(payload)
    ec = EC()
    init(c.byref(ec), data, length)
    history = (c.c_int32 * 42)(*[(-10 * (1 << 24) + i * 12345) for i in range(42)])
    coarse(mode, start, end, history, intra, c.byref(ec), channels, lm)
    print(
        "ORACLE budget",
        length,
        start,
        end,
        channels,
        lm,
        intra,
        list(history)[: 21 * channels],
        (ec.rng, ec.val, ec.nbits_total - ec.rng.bit_length()),
    )

for length, budget, lm, transient, start, end in [
    (2, 0, 0, 0, 0, 21),
    (2, 1, 2, 1, 0, 21),
    (2, 2, 2, 1, 8, 17),
    (3, 3, 1, 0, 0, 21),
    (8, 8, 3, 0, 0, 21),
]:
    data = c.create_string_buffer(bytes.fromhex(packets[0])[1 : 1 + length])
    ec = EC()
    init(c.byref(ec), data, length)
    print(
        "ORACLE TF budget",
        length,
        budget,
        lm,
        transient,
        start,
        end,
        *tf_reference(ec, budget, lm, transient, start, end),
    )

for first in range(256):
    data = c.create_string_buffer(bytes([first, 0x55, 0xAA, 0x77, 0x11]))
    ec = EC()
    init(c.byref(ec), data, 5)
    result = tf_reference(ec, 5, 2, 1)
    if result[1] == 1:
        print("ORACLE TF select1", first, *result)
        break

fine_decode = function(
    "unquant_fine_energy",
    None,
    c.c_void_p,
    c.c_int,
    c.c_int,
    c.POINTER(c.c_int32),
    c.POINTER(c.c_int),
    c.POINTER(EC),
    c.c_int,
)
final_decode = function(
    "unquant_energy_finalise",
    None,
    c.c_void_p,
    c.c_int,
    c.c_int,
    c.POINTER(c.c_int32),
    c.POINTER(c.c_int),
    c.POINTER(c.c_int),
    c.c_int,
    c.POINTER(EC),
    c.c_int,
)
denorm = function(
    "denormalise_bands",
    None,
    c.c_void_p,
    c.POINTER(c.c_int16),
    c.POINTER(c.c_int32),
    c.POINTER(c.c_int32),
    c.c_int,
    c.c_int,
    c.c_int,
    c.c_int,
    c.c_int,
)
fine = (c.c_int * 21)(*[i % 9 for i in range(21)])
priority = (c.c_int * 21)(*[i % 2 for i in range(21)])
history = (c.c_int32 * 42)(*[i * 12345 - (1 << 24) for i in range(42)])
for frame, packet in enumerate(packets):
    data = c.create_string_buffer(bytes.fromhex(packet)[1:])
    ec = EC()
    init(c.byref(ec), data, len(data) - 1)
    fine_decode(mode, 3, 19, history, fine, c.byref(ec), 2)
    print(
        "ORACLE fine",
        frame,
        list(history),
        (ec.rng, ec.val, ec.nbits_total - ec.rng.bit_length()),
    )
    final_decode(mode, 3, 19, history, fine, priority, 7, c.byref(ec), 2)
    print(
        "ORACLE final",
        frame,
        list(history),
        (ec.rng, ec.val, ec.nbits_total - ec.rng.bit_length()),
    )
# Low-order Q24 fractions exercise prescribed Q10 rounding, not a Q8 projection.
energy = (c.c_int32 * 21)(*[(-7 + i % 8) * (1 << 24) + i * 12345 for i in range(21)])
x = (c.c_int16 * 480)(*[((i * 7919) % 32768) - 16384 for i in range(480)])
freq = (c.c_int32 * 480)()
denorm(mode, x, freq, energy, 3, 19, 4, 1, 0)
indices = [
    0,
    11,
    12,
    13,
    16,
    31,
    32,
    45,
    63,
    64,
    79,
    95,
    111,
    135,
    159,
    191,
    192,
    239,
    240,
    479,
]
print("ORACLE denorm", indices, [freq[i] for i in indices])

hash_value = 0xCBF29CE484222325
for value in freq:
    for byte in int(value).to_bytes(4, "little", signed=True):
        hash_value = ((hash_value ^ byte) * 0x100000001B3) & ((1 << 64) - 1)
print("ORACLE denorm FNV64", hex(hash_value))

rotation = function(
    "exp_rotation",
    None,
    c.POINTER(c.c_int16),
    c.c_int,
    c.c_int,
    c.c_int,
    c.c_int,
    c.c_int,
)
haar = function("haar1", None, c.POINTER(c.c_int16), c.c_int, c.c_int)
for n, blocks, pulses, spread in [
    (16, 1, 2, 1),
    (32, 4, 3, 2),
    (8, 1, 1, 3),
    (16, 2, 0, 2),
]:
    x = (c.c_int16 * n)(*[(i * 7919 % 16384) - 8192 for i in range(n)])
    rotation(x, n, -1, blocks, pulses, spread)
    print("ORACLE rotation", n, blocks, pulses, spread, list(x))

for n, original_b, tf in [(16, 4, 1), (32, 4, 2), (16, 1, -2), (32, 4, -1), (8, 1, 0)]:
    x = (c.c_int16 * n)(*[(i * 7919 % 16384) - 8192 for i in range(n)])
    recombine = max(0, tf)
    b = original_b >> recombine
    width = n // b
    divide = 0
    while width % 2 == 0 and tf < 0:
        b *= 2
        width //= 2
        divide += 1
        tf += 1
    if b > 1:
        stride = b << recombine
        count = width >> recombine
        ordery = [
            1,
            0,
            3,
            0,
            2,
            1,
            7,
            0,
            4,
            3,
            6,
            1,
            5,
            2,
            15,
            0,
            8,
            7,
            12,
            3,
            11,
            4,
            14,
            1,
            9,
            6,
            13,
            2,
            10,
            5,
        ]
        src = list(x)
        for lane in range(stride):
            source = ordery[stride - 2 + lane] if original_b == 1 else lane
            for j in range(count):
                x[j * stride + lane] = src[source * count + j]
    for k in range(divide):
        b //= 2
        width *= 2
        haar(x, width, b)
    for k in range(recombine):
        haar(x, n >> k, 1 << k)
    print(
        "ORACLE inverse TF",
        n,
        original_b,
        max(0, recombine) if recombine else -divide,
        list(x),
    )

alg = function(
    "alg_unquant",
    c.c_uint,
    c.POINTER(c.c_int16),
    c.c_int,
    c.c_int,
    c.c_int,
    c.c_int,
    c.POINTER(EC),
    c.c_int32,
)
for n, pulses, spread, blocks in [(8, 1, 2, 1), (16, 2, 1, 4), (4, 3, 3, 1)]:
    data = c.create_string_buffer(bytes.fromhex(packets[0])[1:])
    ec = EC()
    init(c.byref(ec), data, len(data) - 1)
    x = (c.c_int16 * n)()
    mask = alg(x, n, pulses, spread, blocks, c.byref(ec), 2147483647)
    print(
        "ORACLE alg",
        n,
        pulses,
        spread,
        blocks,
        list(x),
        mask,
        (ec.rng, ec.val, ec.nbits_total - ec.rng.bit_length()),
    )
