"""FitSense worker v1. Standard library only, with stdout reserved for IPC."""

import json
import re
import sys


VERSION = 1
MAX_FRAME = 32 * 1024
REQUEST_ID = re.compile(r"[a-zA-Z0-9_-]{1,64}")


def log(event):
    print(event, file=sys.stderr, flush=True)


def write(message):
    wire = json.dumps({"version": VERSION, **message}, separators=(",", ":"))
    sys.stdout.buffer.write(wire.encode("ascii") + b"\n")
    sys.stdout.buffer.flush()


def fail(code, request_id=None):
    write({"type": "error", "id": request_id, "code": code})


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate key")
        result[key] = value
    return result


def reject_constant(_value):
    raise ValueError("invalid JSON constant")


def request(frame):
    try:
        decoded = json.loads(
            frame.decode("utf-8"), object_pairs_hook=unique_object,
            parse_constant=reject_constant,
        )
    except (ValueError, RecursionError):
        return None, None, "malformed_json"

    if not isinstance(decoded, dict):
        return None, None, "invalid_request"
    request_id = decoded.get("id")
    if not isinstance(request_id, str) or not REQUEST_ID.fullmatch(request_id):
        return None, None, "invalid_request"
    if set(decoded) != {"version", "id", "method"}:
        return request_id, None, "invalid_request"
    if type(decoded["version"]) is not int or decoded["version"] != VERSION:
        return request_id, None, "unsupported_version"
    if not isinstance(decoded["method"], str):
        return request_id, None, "invalid_request"
    return request_id, decoded["method"], None


def main():
    log("worker_started")
    methods = {
        "health": {"status": "ok"},
        "diagnostics": {"pythonVersion": ".".join(map(str, sys.version_info[:3]))},
        "shutdown": {"status": "stopping"},
    }
    write({"type": "ready", "service": "fitsense-worker"})

    while frame := sys.stdin.buffer.readline(MAX_FRAME + 1):
        if len(frame) > MAX_FRAME or not frame.endswith(b"\n"):
            log("worker_invalid_frame")
            fail("invalid_frame")
            break

        request_id, method, error = request(frame)
        if error:
            log("worker_invalid_request")
            fail(error, request_id)
            continue

        if method not in methods:
            fail("unknown_method", request_id)
            continue

        write({"type": "response", "id": request_id, "result": methods[method]})
        if method == "shutdown":
            break

    log("worker_stopped")


if __name__ == "__main__":
    try:
        main()
    except BrokenPipeError:
        # The supervisor can disappear while a response is being flushed.
        sys.exit(1)
    except Exception:
        log("worker_fatal")
        sys.exit(1)
