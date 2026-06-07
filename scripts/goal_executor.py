#!/usr/bin/env python3
"""
TASK-117 — External Goal Task Executor (方案二)

Polls the Runtime API for queued goal tasks, delegates execution to the
desktop's built-in chat pipeline (full 43-tool set, projection back to goal
session) via POST /runtime/tasks/{id}/execute, then polls for completion.

Run alongside the desktop app:

    python scripts/goal_executor.py

Environment:
    CONDUCTOR_ROOT   Override project root (default: I:\\personal-agent)
    EXECUTOR_AGENT   Agent ID reported to the Runtime API (default: goal_executor)
    POLL_INTERVAL    Seconds between poll cycles when idle (default: 3)
    LEASE_TTL        Claim lease duration in seconds (default: 600)
    WORKSPACE_ID     Only claim tasks for this workspace (optional)
    RESULT_TIMEOUT   Max seconds to wait for a task to reach a terminal state
                     after execute is called (default: 540)
"""

import json
import os
import sys
import time
from pathlib import Path

# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------

ROOT = Path(os.environ.get("CONDUCTOR_ROOT", r"I:\personal-agent"))
STATE = ROOT / "state"
RUNTIME_API_STATE = STATE / "runtime-api.json"

AGENT_ID = os.environ.get("EXECUTOR_AGENT", "goal_executor")
POLL_INTERVAL = int(os.environ.get("POLL_INTERVAL", "3"))
LEASE_TTL = int(os.environ.get("LEASE_TTL", "600"))
WORKSPACE_ID = os.environ.get("WORKSPACE_ID")
RESULT_TIMEOUT = int(os.environ.get("RESULT_TIMEOUT", "540"))

TERMINAL_STATUSES = {"review_ready", "accepted", "failed", "blocked", "cancelled"}


# ---------------------------------------------------------------------------
# Runtime API helpers
# ---------------------------------------------------------------------------

def load_api_state():
    """Return (base_url, token) or None if the runtime API is not ready."""
    try:
        data = json.loads(RUNTIME_API_STATE.read_text(encoding="utf-8"))
        if not data.get("running"):
            return None
        base_url = (data.get("baseUrl") or data.get("base_url", "")).rstrip("/")
        token = data.get("token", "")
        if base_url and token:
            return base_url, token
    except (FileNotFoundError, json.JSONDecodeError):
        pass
    return None


def _request(method, base_url, token, path, body=None):
    import urllib.request
    import urllib.error
    url = f"{base_url}{path}"
    payload = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(
        url,
        data=payload,
        method=method,
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {token}",
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=15) as resp:
            return resp.status, json.loads(resp.read())
    except urllib.error.HTTPError as exc:
        try:
            return exc.code, json.loads(exc.read())
        except Exception:
            return exc.code, {"error": str(exc)}
    except Exception as exc:
        return 0, {"error": str(exc)}


def api_post(base_url, token, path, body=None):
    return _request("POST", base_url, token, path, body)


def api_get(base_url, token, path):
    return _request("GET", base_url, token, path)


# ---------------------------------------------------------------------------
# Task lifecycle calls
# ---------------------------------------------------------------------------

def claim_next(base_url, token):
    body = {"agent_id": AGENT_ID, "lease_ttl_seconds": LEASE_TTL}
    if WORKSPACE_ID:
        body["workspace_id"] = WORKSPACE_ID
    status, data = api_post(base_url, token, "/runtime/tasks/claim", body)
    return data if status == 200 else None


def start_task(base_url, token, task_id):
    api_post(base_url, token, f"/runtime/tasks/{task_id}/start", {})


def request_execute(base_url, token, task_id):
    """Ask the desktop to execute the task via the full chat pipeline."""
    status, data = api_post(base_url, token, f"/runtime/tasks/{task_id}/execute", {})
    return status, data


def get_task(base_url, token, task_id):
    status, data = api_get(base_url, token, f"/runtime/tasks/{task_id}")
    return data if status == 200 else None


def fail_task(base_url, token, task_id, error):
    api_post(base_url, token, f"/runtime/tasks/{task_id}/fail", {"error": error})


# ---------------------------------------------------------------------------
# Main execution logic
# ---------------------------------------------------------------------------

def execute_task(base_url, token, task):
    task_id = task["id"]
    title = task.get("title", "")[:80]
    print(f"[executor] claimed task {task_id}: {title!r}", flush=True)

    start_task(base_url, token, task_id)

    # Delegate to the desktop's built-in chat pipeline (full tool set).
    status, data = request_execute(base_url, token, task_id)
    if status not in (200, 202):
        # execute endpoint not available or rejected — fall back to marking failed
        error = data.get("error", f"execute returned HTTP {status}")
        print(f"[executor] execute rejected for {task_id}: {error}", flush=True)
        fail_task(base_url, token, task_id, error)
        return

    print(f"[executor] execute dispatched for {task_id}, polling for result…", flush=True)

    # Poll until terminal status or timeout.
    deadline = time.monotonic() + RESULT_TIMEOUT
    while time.monotonic() < deadline:
        time.sleep(POLL_INTERVAL)
        current = get_task(base_url, token, task_id)
        if current is None:
            print(f"[executor] task {task_id} disappeared; stopping poll", flush=True)
            return
        s = current.get("status", "")
        if s in TERMINAL_STATUSES:
            print(f"[executor] task {task_id} reached terminal status: {s}", flush=True)
            return

    # Timed out waiting.
    print(f"[executor] task {task_id} did not complete within {RESULT_TIMEOUT}s", flush=True)
    fail_task(base_url, token, task_id, f"executor timeout after {RESULT_TIMEOUT}s")


# ---------------------------------------------------------------------------
# Main loop
# ---------------------------------------------------------------------------

def main():
    print(f"[executor] starting — root={ROOT}, agent_id={AGENT_ID}", flush=True)
    while True:
        api = load_api_state()
        if api is None:
            print("[executor] runtime API not ready, waiting…", flush=True)
            time.sleep(POLL_INTERVAL)
            continue

        base_url, token = api
        task = claim_next(base_url, token)
        if task is None:
            time.sleep(POLL_INTERVAL)
            continue

        try:
            execute_task(base_url, token, task)
        except Exception as exc:
            print(f"[executor] unhandled error: {exc}", flush=True)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\n[executor] stopped", flush=True)
        sys.exit(0)
