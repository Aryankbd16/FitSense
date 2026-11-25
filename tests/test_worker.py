import json
import subprocess
import sys
import unittest
from pathlib import Path


WORKER = Path(__file__).resolve().parents[1] / "python-worker" / "worker.py"


def frame(request_id, method, version=1):
    return json.dumps({"version": version, "id": request_id, "method": method}).encode() + b"\n"


def run_worker(payload=b""):
    result = subprocess.run(
        [sys.executable, "-I", "-B", "-u", str(WORKER)],
        input=payload, capture_output=True, timeout=5, check=True,
    )
    return [json.loads(line) for line in result.stdout.splitlines()], result.stderr


class WorkerTests(unittest.TestCase):
    def test_startup_and_eof_shutdown(self):
        messages, stderr = run_worker()
        self.assertEqual(messages, [{"version": 1, "type": "ready", "service": "fitsense-worker"}])
        self.assertEqual(stderr.splitlines(), [b"worker_started", b"worker_stopped"])

    def test_health_diagnostics_correlation_and_graceful_shutdown(self):
        messages, stderr = run_worker(b"".join((
            frame("health-1", "health"), frame("test_2", "diagnostics"), frame("end", "shutdown"),
            frame("never", "health"),
        )))
        self.assertEqual([message["id"] for message in messages[1:]], ["health-1", "test_2", "end"])
        self.assertEqual([message["type"] for message in messages[1:]], ["response"] * 3)
        self.assertEqual(messages[1]["result"], {"status": "ok"})
        self.assertEqual(messages[2]["result"], {"pythonVersion": ".".join(map(str, sys.version_info[:3]))})
        self.assertEqual(messages[3]["result"], {"status": "stopping"})
        self.assertIn(b"worker_stopped", stderr)

    def test_malformed_requests_are_redacted_and_valid_requests_still_work(self):
        private = b"portrait.jpg / personal-measurement"
        messages, stderr = run_worker(b"".join((
            private + b"\n", b"\xff\n", b"[]\n", b'{"id":"a","id":"b"}\n',
            b'{"id":"../portrait.jpg","version":1,"method":"health"}\n',
            frame("wrong-version", "health", True), frame("bad-method", private.decode()),
            frame("good", "health"),
        )))
        self.assertEqual([message.get("code") for message in messages[1:]], [
            "malformed_json", "malformed_json", "invalid_request", "malformed_json",
            "invalid_request", "unsupported_version", "unknown_method", None,
        ])
        self.assertEqual([message["id"] for message in messages[1:]], [None] * 5 + ["wrong-version", "bad-method", "good"])
        self.assertNotIn(private, stderr)
        self.assertNotIn("portrait.jpg", str(messages))

    def test_oversized_or_truncated_frames_terminate_cleanly(self):
        for invalid in (b"x" * (32 * 1024) + b"\n", b"{}"):
            with self.subTest(size=len(invalid)):
                messages, stderr = run_worker(invalid)
                self.assertEqual(messages[1], {"version": 1, "type": "error", "id": None, "code": "invalid_frame"})
                self.assertIn(b"worker_invalid_frame", stderr)


if __name__ == "__main__":
    unittest.main()
