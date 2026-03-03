#!/usr/bin/env python3

import json
import os
import signal
import sqlite3
import subprocess
import sys
import time
from typing import Dict, List, Optional, Set, Tuple


UPDATE_INTERVAL_SECONDS = 3
HEARTBEAT_INTERVAL_SECONDS = 10
NO_SESSION_NOTE_SECONDS = 20
UNDERSTANDING_FILE = "/tmp/ai_understanding.txt"
FINAL_COMMENT_FILE = "/tmp/final_comment_id.txt"
DONE_SIGNAL_FILE = "/tmp/opencode_agent_done.txt"
IDLE_AFTER_FINAL_SECONDS = 10
DB_PATH = os.path.join(
    os.path.expanduser("~"), ".local", "share", "opencode", "opencode.db"
)


running = True
received_signal = False


def log(message: str) -> None:
    print(f"[opencode-db-watcher] {message}", file=sys.stderr, flush=True)


def on_signal(_sig, _frame) -> None:
    global running, received_signal
    received_signal = True
    running = False
    log("Received signal, stopping watcher")


def run_gh(args: List[str], *, stdin: Optional[str] = None) -> Tuple[int, str, str]:
    proc = subprocess.run(
        ["gh", "api", *args],
        input=stdin,
        text=True,
        capture_output=True,
    )
    return proc.returncode, proc.stdout, proc.stderr


def detect_api_endpoint(repo: str, comment_id: str, comment_type: str) -> str:
    if comment_type == "pr_review_comment":
        return f"/repos/{repo}/pulls/comments/{comment_id}"
    if comment_type in ("issue", "pr_issue_comment"):
        return f"/repos/{repo}/issues/comments/{comment_id}"

    code, out, _ = run_gh([f"/repos/{repo}/pulls/comments/{comment_id}", "--jq", ".id"])
    if code == 0 and out.strip():
        log("Auto-detected PR review comment endpoint")
        return f"/repos/{repo}/pulls/comments/{comment_id}"

    log("Auto-detected issue comment endpoint")
    return f"/repos/{repo}/issues/comments/{comment_id}"


def extract_tool_context(tool_name: str, tool_input: Dict) -> str:
    lower = tool_name.lower()
    if lower == "read":
        path = (
            tool_input.get("file_path")
            or tool_input.get("filePath")
            or tool_input.get("file")
            or tool_input.get("path")
        )
        return f" on `{path}`" if path else ""
    if lower == "write":
        path = (
            tool_input.get("file_path")
            or tool_input.get("filePath")
            or tool_input.get("file")
        )
        return f" to `{path}`" if path else ""
    if lower == "edit":
        path = (
            tool_input.get("file_path")
            or tool_input.get("filePath")
            or tool_input.get("file")
        )
        return f" on `{path}`" if path else ""
    if lower == "bash":
        command = tool_input.get("command")
        if not command:
            return ""
        command = str(command)
        if "\n" in command or len(command) > 200:
            return f": `{command.splitlines()[0][:150]}...`"
        return f": `{command}`"
    if lower in ("glob", "grep"):
        pattern = tool_input.get("pattern")
        return f" for `{pattern}`" if pattern else ""
    if lower == "list":
        path = tool_input.get("path")
        return f" in `{path}`" if path else ""
    return ""


def build_progress_block(
    progress_lines: List[str],
    *,
    title: str = "🔄 Live Progress",
    open_details: bool = True,
) -> str:
    details_tag = "<details open>" if open_details else "<details>"
    lines = [details_tag, f"<summary>{title}</summary>", ""]
    lines.extend(progress_lines)
    lines.extend(["", "</details>"])
    return "\n".join(lines)


def strip_existing_progress(body: str) -> str:
    collected: List[str] = []
    for line in body.splitlines():
        if line.strip(" ") == "---":
            break
        collected.append(line)
    while collected and not collected[-1].strip():
        collected.pop()
    return "\n".join(collected)


def update_comment(
    repo: str,
    endpoint: str,
    comment_id: str,
    progress_lines: List[str],
    understanding: Optional[str],
    *,
    title: str = "🔄 Live Progress",
    open_details: bool = True,
) -> bool:
    code, original_body, err = run_gh([endpoint, "--jq", ".body"])
    if code != 0:
        log(f"Failed to fetch comment body: {err.strip()}")
        return False

    base = strip_existing_progress(original_body)
    if understanding and "**My understanding:**" not in base:
        base = (
            f"{base}\n\n**My understanding:** {understanding}"
            if base
            else f"**My understanding:** {understanding}"
        )

    progress_block = build_progress_block(
        progress_lines, title=title, open_details=open_details
    )
    new_body = f"{base}\n\n---\n\n{progress_block}" if base else progress_block

    code, _, err = run_gh(["-X", "PATCH", endpoint, "-F", "body=@-"], stdin=new_body)
    if code != 0:
        log(f"Failed to update comment {comment_id}: {err.strip()}")
        return False

    return True


def discover_session_id(start_ms: int, workspace: Optional[str]) -> Optional[str]:
    if not os.path.isfile(DB_PATH):
        return None

    try:
        conn = sqlite3.connect(f"file:{DB_PATH}?mode=ro", uri=True)
        try:
            cur = conn.cursor()
            cur.execute(
                """
                SELECT id, time_created, directory
                FROM session
                WHERE time_created >= ?
                ORDER BY time_created DESC
                LIMIT 20
                """,
                (start_ms,),
            )
            rows = cur.fetchall()
        finally:
            conn.close()
    except sqlite3.Error as exc:
        log(f"Session discovery query failed: {exc}")
        return None

    if not rows:
        return None

    if workspace:
        for session_id, _created, directory in rows:
            if directory == workspace:
                return str(session_id)

    return str(rows[0][0])


def query_new_parts(
    session_id: str, last_time_created: int
) -> List[Tuple[str, int, Dict]]:
    if not os.path.isfile(DB_PATH):
        return []

    try:
        conn = sqlite3.connect(f"file:{DB_PATH}?mode=ro", uri=True)
        try:
            cur = conn.cursor()
            cur.execute(
                """
                SELECT id, time_created, data
                FROM part
                WHERE session_id = ? AND time_created > ?
                ORDER BY time_created ASC
                """,
                (session_id, last_time_created),
            )
            rows = cur.fetchall()
        finally:
            conn.close()
    except sqlite3.Error as exc:
        log(f"Part polling query failed: {exc}")
        return []

    parsed: List[Tuple[str, int, Dict]] = []
    for part_id, time_created, raw_data in rows:
        try:
            data = json.loads(raw_data)
        except Exception:
            continue
        parsed.append((part_id, int(time_created), data))
    return parsed


def main() -> int:
    if len(sys.argv) < 5:
        log("Usage: opencode-db-watcher.py <repo> <token> <comment_id> <comment_type>")
        return 2

    repo = sys.argv[1]
    token = sys.argv[2]
    comment_id = sys.argv[3]
    comment_type = sys.argv[4]

    os.environ["GH_TOKEN"] = token

    signal.signal(signal.SIGTERM, on_signal)
    signal.signal(signal.SIGINT, on_signal)

    endpoint = detect_api_endpoint(repo, comment_id, comment_type)
    log(f"Using API endpoint: {endpoint}")

    now_utc = time.strftime("%Y-%m-%d %H:%M:%S UTC", time.gmtime())
    progress_lines = [
        f"→ Watcher connected at {now_utc}",
        "→ Waiting for model activity...",
    ]
    seen_parts: Set[str] = set()
    seen_status: Dict[str, str] = {}
    last_time_created = 0
    last_update = 0.0
    tool_events = 0
    all_events = 0
    updates = 0
    start_ms = int(time.time() * 1000)
    workspace = os.environ.get("GITHUB_WORKSPACE")
    last_heartbeat = 0.0
    no_session_note_added = False
    last_part_activity = time.time()
    done_signal_written = False

    if os.path.exists(DONE_SIGNAL_FILE):
        try:
            os.remove(DONE_SIGNAL_FILE)
        except OSError:
            pass

    session_id: Optional[str] = None
    understanding_inserted = False

    if update_comment(repo, endpoint, comment_id, progress_lines, None):
        updates += 1
        log("Posted initial watcher status to acknowledgment comment")
    else:
        log("Failed to post initial watcher status to acknowledgment comment")

    while running:
        now = time.time()

        understanding_text: Optional[str] = None
        if os.path.isfile(UNDERSTANDING_FILE):
            try:
                text = open(UNDERSTANDING_FILE, "r", encoding="utf-8").read().strip()
                if text:
                    understanding_text = text
            except OSError:
                understanding_text = None

        if not understanding_inserted and understanding_text:
            understanding_inserted = True
            log("Detected understanding file")
            if now - last_update >= 1:
                if update_comment(
                    repo, endpoint, comment_id, progress_lines, understanding_text
                ):
                    updates += 1
                last_update = now

        if not session_id:
            session_id = discover_session_id(start_ms, workspace)
            if session_id:
                log(f"Detected OpenCode session: {session_id}")

        if (
            not session_id
            and not no_session_note_added
            and (now * 1000 - start_ms) >= (NO_SESSION_NOTE_SECONDS * 1000)
        ):
            progress_lines.append(
                "→ Waiting for OpenCode session to appear in local DB..."
            )
            no_session_note_added = True
            if now - last_update >= 1:
                if update_comment(
                    repo, endpoint, comment_id, progress_lines, understanding_text
                ):
                    updates += 1
                last_update = now
            log("No OpenCode session discovered yet; posted diagnostic note")

        if session_id:
            new_parts = query_new_parts(session_id, last_time_created)
            log(f"Polled {len(new_parts)} new part rows for session {session_id}")
            changed = False
            if new_parts:
                last_part_activity = now

            for part_id, time_created, data in new_parts:
                if time_created > last_time_created:
                    last_time_created = time_created

                if data.get("type") != "tool":
                    part_type = str(data.get("type") or "")
                    if part_type == "step-start":
                        if progress_lines and progress_lines[0].startswith(
                            "→ Waiting for model activity"
                        ):
                            progress_lines.pop(0)
                        progress_lines.append("→ Started reasoning step")
                        changed = True
                        all_events += 1
                    continue

                state = data.get("state") or {}
                status = state.get("status")
                tool = str(data.get("tool") or "Tool")
                raw_input = state.get("input")
                tool_input: Dict = raw_input if isinstance(raw_input, dict) else {}

                prior_status = seen_status.get(part_id)
                if prior_status == status:
                    continue

                seen_status[part_id] = str(status)
                if part_id not in seen_parts:
                    seen_parts.add(part_id)

                context = extract_tool_context(tool, tool_input)
                if status == "completed":
                    line = f"→ Used `{tool}`{context}"
                elif status == "running":
                    line = f"→ Running `{tool}`{context}"
                elif status == "error":
                    line = f"→ `{tool}` failed{context}"
                else:
                    continue

                if progress_lines and progress_lines[0].startswith(
                    "→ Waiting for model activity"
                ):
                    progress_lines.pop(0)
                progress_lines.append(line)
                tool_events += 1
                all_events += 1
                changed = True

            if changed and now - last_update >= UPDATE_INTERVAL_SECONDS:
                if update_comment(
                    repo, endpoint, comment_id, progress_lines, understanding_text
                ):
                    updates += 1
                last_update = now

        if (
            session_id
            and not done_signal_written
            and os.path.isfile(FINAL_COMMENT_FILE)
            and (now - last_part_activity) >= IDLE_AFTER_FINAL_SECONDS
        ):
            done_signal_written = True
            progress_lines.append(
                "→ Final response posted; waiting for workflow cleanup"
            )
            if update_comment(
                repo, endpoint, comment_id, progress_lines, understanding_text
            ):
                updates += 1
            last_update = now
            try:
                with open(DONE_SIGNAL_FILE, "w", encoding="utf-8") as f:
                    f.write("done\n")
            except OSError as exc:
                log(f"Failed to write done signal file: {exc}")
            log(
                "Detected final response and idle tool activity; wrote completion signal"
            )
            break

        if now - last_heartbeat >= HEARTBEAT_INTERVAL_SECONDS:
            last_heartbeat = now
            log(
                "heartbeat "
                + f"session_found={'yes' if session_id else 'no'} "
                + f"db_exists={'yes' if os.path.isfile(DB_PATH) else 'no'} "
                + f"workspace={'set' if workspace else 'unset'} "
                + f"events_seen={all_events} tool_events={tool_events} updates={updates}"
            )

        time.sleep(1)

    final_understanding: Optional[str] = None
    if os.path.isfile(UNDERSTANDING_FILE):
        try:
            final_understanding = (
                open(UNDERSTANDING_FILE, "r", encoding="utf-8").read().strip() or None
            )
        except OSError:
            final_understanding = None

    if done_signal_written:
        progress_lines.append("→ Completed")
    elif received_signal:
        progress_lines.append("→ Interrupted")

    if update_comment(
        repo,
        endpoint,
        comment_id,
        progress_lines,
        final_understanding,
        title="💭 How I worked on this",
        open_details=False,
    ):
        updates += 1
    log(
        f"Stopped watcher; parsed {tool_events} tool events, {all_events} total events, {updates} comment updates"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
