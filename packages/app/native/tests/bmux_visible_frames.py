"""Actual isolated X11 frame sampling, independent of PTY output timestamps."""

import ctypes as C
import json
import math
import time


class XImage(C.Structure):
    _fields_ = [
        (name, kind)
        for name, kind in (
            ("width", C.c_int),
            ("height", C.c_int),
            ("xoffset", C.c_int),
            ("format", C.c_int),
            ("data", C.c_void_p),
            ("byte_order", C.c_int),
            ("bitmap_unit", C.c_int),
            ("bitmap_bit_order", C.c_int),
            ("bitmap_pad", C.c_int),
            ("depth", C.c_int),
            ("bytes_per_line", C.c_int),
            ("bits_per_pixel", C.c_int),
        )
    ]


class FrameSampler:
    def __init__(self, display):
        self.x = C.CDLL("libX11.so.6")
        self.x.XOpenDisplay.argtypes = [C.c_char_p]
        self.x.XOpenDisplay.restype = C.c_void_p
        self.x.XDefaultRootWindow.argtypes = [C.c_void_p]
        self.x.XDefaultRootWindow.restype = C.c_ulong
        self.x.XGetImage.argtypes = [
            C.c_void_p,
            C.c_ulong,
            C.c_int,
            C.c_int,
            C.c_uint,
            C.c_uint,
            C.c_ulong,
            C.c_int,
        ]
        self.x.XGetImage.restype = C.POINTER(XImage)
        self.x.XDestroyImage.argtypes = [C.POINTER(XImage)]
        self.x.XCloseDisplay.argtypes = [C.c_void_p]
        self.display = self.x.XOpenDisplay(display.encode())
        if not self.display:
            raise OSError("Cannot open dedicated display")
        self.root = self.x.XDefaultRootWindow(self.display)

    def close(self):
        self.x.XCloseDisplay(self.display)

    def grab(self, rect):
        begin = time.monotonic()
        image = self.x.XGetImage(self.display, self.root, *rect, 0xFFFFFFFF, 2)
        if not image:
            raise OSError("XGetImage failed")
        try:
            value = image.contents
            if value.bits_per_pixel != 32:
                raise ValueError("Sampler requires actual 32bit XImage storage")
            raw = C.string_at(value.data, value.bytes_per_line * rect[3])
            return begin, time.monotonic(), raw, value.bytes_per_line
        finally:
            self.x.XDestroyImage(image)

    def cached_back(self, send, rect, expected, stride, artifacts):
        sent = time.monotonic()
        send()
        lower, upper, polls = 0.0, None, []
        while time.monotonic() - sent < 0.6:
            begin, end, raw, actual_stride = self.grab(rect)
            match = actual_stride == stride and raw == expected
            polls.append(dict(begin=begin - sent, end=end - sent, matched=match))
            if match:
                upper = end - sent
                break
            lower = max(lower, begin - sent)
            time.sleep(0.005)
        result = dict(
            metric="actual cached artwork+metadata ROI restored",
            rect=rect,
            lower_seconds=lower,
            upper_seconds=upper,
            polls=polls,
        )
        (artifacts / "visible-back.json").write_text(json.dumps(result, indent=2))
        return result

    def translated_samples(
        self, send, rect, delta, artifacts, *, sequential_down=False
    ):
        """Exact same cover AND metadata template after known wheel displacement.

        Caller must select a ROI containing both artwork and labels, safely away
        from clipping. Missing matches are failures, never discarded samples.
        Capture begin/end bounds include uncertainty; overhead is not subtracted.
        Sequential mode re-baselines the same interior ROI before EVERY down input,
        retaining pixel identity across one displacement rather than depending on
        titles surviving clipping. Missing/diversity failures still block acceptance.
        """
        samples, retained = [], []
        for index in range(16):
            time.sleep(0.25)
            baseline_rect = (
                rect
                if sequential_down or index % 2 == 0
                else (rect[0], rect[1] - delta, rect[2], rect[3])
            )
            baseline_begin, baseline_end, baseline, stride = self.grab(baseline_rect)
            direction = 65 if sequential_down or index % 2 == 0 else 64
            target = (
                baseline_rect[0],
                baseline_rect[1] + (-delta if direction == 65 else delta),
                baseline_rect[2],
                baseline_rect[3],
            )
            # Reject uniform/empty templates that could match without a repaint.
            diverse = len(set(baseline[::4])) > 16
            # Reject a template already present at the target before input.
            target_begin, target_end, target_before, target_stride = self.grab(target)
            distinguishable = target_stride != stride or target_before != baseline
            anchor_before = time.monotonic_ns()
            anchor_wall = time.time_ns()
            anchor_after = time.monotonic_ns()
            sent = time.monotonic()
            send(direction)
            write_completed = time.monotonic()
            polls, upper, lower = [], None, 0.0
            raw, actual_stride = b"", stride
            while time.monotonic() - sent < 0.6:
                begin, end, raw, actual_stride = self.grab(target)
                matched = (
                    diverse
                    and distinguishable
                    and actual_stride == stride
                    and raw == baseline
                )
                polls.append(dict(begin=begin - sent, end=end - sent, matched=matched))
                if matched:
                    upper = end - sent
                    break
                lower = max(lower, begin - sent)
                time.sleep(0.005)
            retained.append((index, baseline, target_before, raw, stride))
            samples.append(
                dict(
                    index=index,
                    input_id=f"visible-wheel-{index}",
                    clock_anchor={
                        "monotonic_before_ns": anchor_before,
                        "wall_ns": anchor_wall,
                        "monotonic_after_ns": anchor_after,
                    },
                    direction=direction,
                    input_monotonic=sent,
                    write_completed_monotonic=write_completed,
                    baseline_rect=baseline_rect,
                    target_rect=target,
                    distinguishable=distinguishable,
                    status=(
                        "matched" if upper is not None else "missing_exact_translation"
                    ),
                    baseline_stride=stride,
                    target_before_stride=target_stride,
                    baseline_capture_bounds=[baseline_begin, baseline_end],
                    target_before_capture_bounds=[target_begin, target_end],
                    target_before_file=f"visible-{index}-target-before.bgra",
                    last_poll_stride=actual_stride,
                    baseline_file=f"visible-{index}-before.bgra",
                    last_poll_file=f"visible-{index}-after.bgra",
                    lower_seconds=lower,
                    upper_seconds=upper,
                    polls=polls,
                    diverse=diverse,
                )
            )
        complete = sorted(
            s["upper_seconds"] for s in samples if s["upper_seconds"] is not None
        )
        result = dict(
            metric="actual Xvfb translated viewport pixels",
            motion_pattern="sequential_down" if sequential_down else "alternating",
            rect=rect,
            delta_pixels=delta,
            samples=samples,
            completed=len(complete),
            p95_upper_seconds=(
                complete[math.ceil(0.95 * len(complete)) - 1]
                if len(complete) == 16
                else None
            ),
            max_capture_seconds=max(
                p["end"] - p["begin"] for s in samples for p in s["polls"]
            ),
            limitation="Kitty/Xvfb only; exact-template misses block acceptance, not stall proof",
        )
        (artifacts / "visible-performance.json").write_text(
            json.dumps(result, indent=2)
        )
        # No files/PNG encoding during sample windows.
        for index, before, target_before, after, stride in retained:
            (artifacts / f"visible-{index}-target-before.bgra").write_bytes(
                target_before
            )
            (artifacts / f"visible-{index}-before.bgra").write_bytes(before)
            (artifacts / f"visible-{index}-after.bgra").write_bytes(after)
        return result
