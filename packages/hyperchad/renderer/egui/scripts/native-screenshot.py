#!/usr/bin/env python3
"""Capture an explicitly supplied native HyperChad/MoosicBox executable on private X11."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import select
import shutil
import signal
import subprocess
import sys
import time


def arguments():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", required=True, type=Path)
    parser.add_argument("--kind", required=True, choices=("hyperchad", "moosicbox"))
    parser.add_argument(
        "--artifacts",
        required=True,
        type=Path,
        help="New directory; logs, isolated state, PNG and metadata are retained",
    )
    parser.add_argument(
        "--required-id",
        action="append",
        default=[],
        help="Required visible production element ID; repeat for each ID",
    )
    parser.add_argument(
        "--variant",
        choices=("alternate",),
        help="Genuine feedback fixture variation, at unchanged native scale",
    )
    parser.add_argument(
        "--scale",
        type=float,
        default=1.0,
        help="Explicit X11 window scale (0.5..3; default 1); useful for native mismatch checks",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=45,
        help="Capture deadline in seconds (1..300; default: 45)",
    )
    parser.add_argument(
        "--screenshot-env",
        default="HYPERCHAD_EGUI_SCREENSHOT",
        help="Renderer opt-in environment variable receiving an absolute PNG path",
    )
    parser.add_argument(
        "--feedback-example",
        action="store_true",
        help="Enable feedback example-specific baseline comparator (no capture aliases)",
    )
    parser.add_argument(
        "--expect-mismatch",
        action="store_true",
        help="Assert native comparator failure plus expected/actual/diff; requires --baseline",
    )
    parser.add_argument(
        "--baseline",
        type=Path,
        help="Feedback example only: strict native-frame baseline PNG",
    )
    parser.add_argument(
        "args",
        nargs=argparse.REMAINDER,
        help="Arguments after -- are passed verbatim; do not supply real state paths",
    )
    options = parser.parse_args()
    if not 0.5 <= options.scale <= 3:
        parser.error("--scale must be in 0.5..3")
    if not 1 <= options.timeout <= 300:
        parser.error("--timeout must be in 1..300")
    if (
        not options.screenshot_env.isidentifier()
        or not options.screenshot_env.startswith("HYPERCHAD_")
    ):
        parser.error("--screenshot-env must be a HYPERCHAD_ environment variable")
    if options.expect_mismatch and not options.baseline:
        parser.error("--expect-mismatch requires --baseline")
    if options.baseline and not options.feedback_example:
        parser.error("--baseline currently requires --feedback-example")
    if any(not value.strip() or "," in value for value in options.required_id):
        parser.error("--required-id values must be nonempty and contain no commas")
    return options


def stop(process):
    """Terminate the whole owned session, even if its original leader exited."""
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=2)
    except subprocess.TimeoutExpired:
        pass
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    process.wait(timeout=2)


def run(command, env, timeout=5):
    return subprocess.run(
        command, env=env, check=True, capture_output=True, text=True, timeout=timeout
    ).stdout


def image_info(path):
    # Imported lazily so --help works without the visual-test shell.
    from PIL import Image

    with Image.open(path) as image:
        if image.format != "PNG" or min(image.size) < 32:
            raise ValueError("capture must be a PNG at least 32x32")
        image.load()
        if all(low == high for low, high in image.convert("RGB").getextrema()):
            raise ValueError("capture is a uniform image, not native UI proof")
        return {
            "width": image.width,
            "height": image.height,
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        }


def main():
    options = arguments()
    executable = options.executable.absolute()
    executable.resolve(
        strict=True
    )  # Validate without changing argv[0] for multicall binaries.
    if not executable.is_file() or not os.access(executable, os.X_OK):
        raise ValueError("--executable must name an executable file")
    for tool in ("Xvfb", "xauth", "xdpyinfo", "glxinfo", "fc-list"):
        if not shutil.which(tool):
            raise ValueError(f"missing {tool}; enter nix develop .#egui-visual-test")
    fontconfig = Path(os.environ.get("FONTCONFIG_FILE", ""))
    if not fontconfig.is_file() or not str(fontconfig).startswith("/nix/store/"):
        raise ValueError(
            "FONTCONFIG_FILE must be the pinned visual-test shell's Nix font config"
        )
    artifacts = options.artifacts.resolve()
    baseline_hash = None
    if options.baseline:
        options.baseline = options.baseline.resolve(strict=True)
        if not options.baseline.is_file() or options.baseline.is_relative_to(artifacts):
            raise ValueError(
                "baseline must be a regular PNG outside the new artifact directory"
            )
        image_info(options.baseline)
        baseline_hash = hashlib.sha256(options.baseline.read_bytes()).hexdigest()
    artifacts.mkdir(mode=0o700)  # Refuse reused directories/stale screenshot success.
    state = artifacts / "state"
    state.mkdir(mode=0o700)
    env = {
        key: os.environ[key]
        for key in (
            "PATH",
            "LD_LIBRARY_PATH",
            "LIBGL_DRIVERS_PATH",
            "FONTCONFIG_FILE",
            "VK_ICD_FILENAMES",
        )
        if key in os.environ
    }
    env.update(
        HOME=str(state),
        LANG="C.UTF-8",
        LC_ALL="C.UTF-8",
        TZ="UTC",
        LIBGL_ALWAYS_SOFTWARE="1",
        GALLIUM_DRIVER="llvmpipe",
        WINIT_UNIX_BACKEND="x11",
        WINIT_X11_SCALE_FACTOR=str(options.scale),
        GDK_BACKEND="x11",
        QT_QPA_PLATFORM="xcb",
        WGPU_BACKEND="gl",
        RUST_LOG="info",
    )
    for name, directory in (
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_STATE_HOME", "local-state"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_RUNTIME_DIR", "runtime"),
        ("TMPDIR", "tmp"),
    ):
        path = state / directory
        path.mkdir(mode=0o700)
        env[name] = str(path)
    config_root = state / "moosicbox"
    config_root.mkdir(mode=0o700)
    env["MOOSICBOX_CONFIG_DIR"] = str(config_root)
    env["MOOSICBOX_STATE_DIR"] = env["XDG_STATE_HOME"]
    log_root = state / "logs"
    log_root.mkdir(mode=0o700)
    env["MOOSICBOX_LOG_DIR"] = str(log_root)
    # No inherited DISPLAY, WAYLAND_DISPLAY, D-Bus, audio, credentials or app config.
    # Explicit high display + X server lock prevents attachment to an existing server.
    display = f":{secrets.randbelow(50000) + 1000}"
    env["DISPLAY"] = display
    env["XAUTHORITY"] = str(state / "Xauthority")
    run(
        [
            "xauth",
            "-f",
            env["XAUTHORITY"],
            "add",
            display,
            "MIT-MAGIC-COOKIE-1",
            secrets.token_hex(16),
        ],
        env,
    )
    png = artifacts / "screenshot.png"
    env[options.screenshot_env] = str(png)
    env["HYPERCHAD_EGUI_SCREENSHOT_EXIT"] = "1"
    env["HYPERCHAD_EGUI_REQUIRED_IDS"] = ",".join(options.required_id)
    env["HYPERCHAD_EGUI_READY"] = str(artifacts / "ready")
    if options.variant:
        env["HYPERCHAD_FEEDBACK_VARIANT"] = options.variant
    if options.feedback_example:
        # Comparator settings are example-specific; capture always uses the
        # agreed production names, never a legacy OUTPUT/READY alias.
        if options.baseline:
            env["HYPERCHAD_FEEDBACK_BASELINE"] = str(
                options.baseline.resolve(strict=True)
            )
            env["HYPERCHAD_FEEDBACK_ARTIFACTS"] = str(artifacts / "comparison")
    metadata = {
        "status": "starting",
        "kind": options.kind,
        "executable": str(executable),
        "executable_sha256": hashlib.file_digest(
            executable.open("rb"), "sha256"
        ).hexdigest(),
        "display": display,
        "protocol": "x11",
        "software_graphics": True,
        "window_scale": options.scale,
        "fontconfig": str(fontconfig),
        "capture_env": options.screenshot_env,
        "state_root": str(state),
        "argv": options.args,
        "feedback_example_comparator": options.feedback_example,
        "required_ids": options.required_id,
        "variant": options.variant,
        "baseline_sha256_before": baseline_hash,
        "baseline": str(options.baseline.resolve()) if options.baseline else None,
        "note": "Kind is caller-declared; inspect PNG and app log for product acceptance.",
    }
    read_fd = None
    processes = []
    logs = []
    start = time.monotonic()
    deadline = start + options.timeout
    try:
        read_fd, write_fd = os.pipe()
        try:
            xvfb_log = (artifacts / "xvfb.log").open("wb")
            logs.append(xvfb_log)
            xvfb = subprocess.Popen(
                [
                    "Xvfb",
                    display,
                    "-displayfd",
                    str(write_fd),
                    "-screen",
                    "0",
                    "1280x800x24",
                    "-dpi",
                    "96",
                    "-nolisten",
                    "tcp",
                    "-auth",
                    env["XAUTHORITY"],
                    "-noreset",
                ],
                env=env,
                stdout=xvfb_log,
                stderr=subprocess.STDOUT,
                pass_fds=(write_fd,),
                start_new_session=True,
            )
            processes.append(xvfb)
            os.close(write_fd)
            write_fd = None
            if not select.select([read_fd], [], [], min(10, options.timeout))[0]:
                raise TimeoutError("Xvfb did not report readiness")
            reported = os.read(read_fd, 64).decode().strip()
            if reported != display[1:] or xvfb.poll() is not None:
                raise RuntimeError("private Xvfb failed; refusing any existing display")
        finally:
            # Xvfb may publish readiness again during startup/reset. Keep its
            # pipe reader alive until server teardown, avoiding an EPIPE crash.
            if write_fd is not None:
                os.close(write_fd)
        (artifacts / "x11.txt").write_text(run(["xdpyinfo"], env))
        graphics = run(["glxinfo", "-B"], env)
        (artifacts / "graphics.txt").write_text(graphics)
        if "llvmpipe" not in graphics.lower():
            raise RuntimeError(
                "Mesa llvmpipe not confirmed; refusing hardware/unknown renderer"
            )
        (artifacts / "fonts.txt").write_text(run(["fc-list"], env))
        if time.monotonic() >= deadline:
            raise TimeoutError("display setup exhausted capture deadline")
        app_log = (artifacts / "app.log").open("wb")
        logs.append(app_log)
        app_args = options.args[1:] if options.args[:1] == ["--"] else options.args
        app = subprocess.Popen(
            [str(executable), *app_args],
            cwd=state,
            env=env,
            stdout=app_log,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        processes.append(app)
        metadata["app_pid"] = app.pid
        last_image_error = "no screenshot written"
        while time.monotonic() < deadline:
            if png.is_file():
                try:
                    metadata["image"] = image_info(png)
                    remaining = deadline - time.monotonic()
                    if remaining <= 0:
                        raise TimeoutError(
                            "no time left for requested clean screenshot exit"
                        )
                    metadata["app_returncode_at_capture"] = app.wait(timeout=remaining)
                    if (
                        options.baseline
                        and hashlib.sha256(options.baseline.read_bytes()).hexdigest()
                        != baseline_hash
                    ):
                        raise RuntimeError("baseline changed during native comparison")
                    if options.expect_mismatch:
                        comparison = artifacts / "comparison"
                        for name in ("expected.png", "actual.png", "diff.png"):
                            image_info(comparison / name)
                        app_output = (artifacts / "app.log").read_text(errors="replace")
                        if (
                            app.returncode == 0
                            or "Mismatch { pixels:" not in app_output
                        ):
                            raise RuntimeError(
                                "expected native mismatch was not reported by comparator"
                            )
                        from PIL import Image, ImageChops

                        with Image.open(options.baseline) as baseline_image, Image.open(
                            comparison / "expected.png"
                        ) as expected_image:
                            if (
                                baseline_image.size != expected_image.size
                                or ImageChops.difference(
                                    baseline_image.convert("RGB"),
                                    expected_image.convert("RGB"),
                                ).getbbox()
                            ):
                                raise RuntimeError(
                                    "comparator expected image does not match baseline"
                                )
                        metadata["expected_mismatch"] = True
                    elif app.returncode != 0:
                        raise RuntimeError(f"application failed with {app.returncode}")
                    if not (artifacts / "ready").is_file():
                        raise RuntimeError(
                            "capture completed without geometry/assets readiness marker"
                        )
                    if png.with_suffix(".error.txt").exists():
                        raise RuntimeError("renderer reported a capture error artifact")
                    sidecar = png.with_suffix(".json")
                    if sidecar.is_file():
                        metadata["renderer_metadata"] = json.loads(sidecar.read_text())
                    metadata["status"] = "captured"
                    break
                except (OSError, ValueError) as error:
                    last_image_error = str(error)
            if app.poll() is not None:
                raise RuntimeError(
                    f"application exited ({app.returncode}): {last_image_error}"
                )
            if xvfb.poll() is not None:
                raise RuntimeError("private Xvfb exited during capture")
            time.sleep(0.1)
        else:
            raise TimeoutError(
                f"native screenshot deadline exceeded: {last_image_error}"
            )
    except BaseException as error:
        metadata.update(status="failed", error=str(error))
        raise
    finally:
        for process in reversed(processes):
            stop(process)
        if read_fd is not None:
            os.close(read_fd)
        for log in logs:
            log.close()
        metadata["ready_marker_exists"] = (artifacts / "ready").is_file()
        error_path = png.with_suffix(".error.txt")
        if error_path.is_file():
            metadata["renderer_error"] = error_path.read_text(errors="replace")
        if options.baseline:
            metadata["baseline_sha256_after"] = hashlib.sha256(
                options.baseline.read_bytes()
            ).hexdigest()
        metadata["elapsed_seconds"] = round(time.monotonic() - start, 3)
        metadata["cleanup"] = "owned process groups terminated; isolated state retained"
        (artifacts / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
    print(f"Captured {png}; inspect metadata.json, graphics.txt and app.log")


if __name__ == "__main__":

    def interrupted(signum, _frame):
        raise KeyboardInterrupt(f"received signal {signum}")

    signal.signal(signal.SIGTERM, interrupted)
    try:
        main()
    except (Exception, KeyboardInterrupt) as error:
        print(f"native-screenshot: {error}", file=sys.stderr)
        sys.exit(1)
