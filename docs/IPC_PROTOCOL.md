# Worker IPC protocol v1

The Rust parent starts one Python worker. Stdin carries requests and stdout carries responses. Frames are UTF-8 JSON objects with an LF delimiter. CRLF is accepted. The maximum frame size is 32 KiB including the delimiter. Stderr carries fixed diagnostic event tokens that Rust may record after allowlist matching.

## Startup and requests

The worker sends exactly one readiness frame before reading requests. Rust validates the service and version, then runs a health check before reporting a connected worker.

```jsonl
{"version":1,"type":"ready","service":"fitsense-worker"}
{"version":1,"id":"1","method":"health"}
{"version":1,"type":"response","id":"1","result":{"status":"ok"}}
{"version":1,"id":"2","method":"diagnostics"}
{"version":1,"type":"response","id":"2","result":{"pythonVersion":"3.13.17"}}
{"version":1,"id":"3","method":"shutdown"}
{"version":1,"type":"response","id":"3","result":{"status":"stopping"}}
```

| Variant | Fields and rules |
| --- | --- |
| Request | `version` is integer `1`. `id` is a 1–64 character ASCII string using letters, digits, `_` and `-`. `method` is a string. Each request has exactly these fields. |
| Ready | `version`, `type: "ready"` and `service: "fitsense-worker"`. |
| Response | `version`, `type: "response"`, the request `id`, and a method-specific `result`. |
| Error | `version`, `type: "error"`, `id` set to a valid known request ID or `null` and a fixed `code`. |

Error codes are `malformed_json`, `invalid_request`, `unsupported_version`, `unknown_method` and `invalid_frame`. Malformed JSON includes invalid UTF-8 and duplicate keys. The worker continues after validly delimited malformed input. Oversized or truncated input gets an `invalid_frame` error followed by process exit. Errors omit input values, tracebacks and paths. Rust rejects mismatched IDs, versions, message types and results. No results from an abandoned process are reused.

## Timing and cancellation

- Startup allows up to 8 seconds for readiness. The first and subsequent health requests each have a 3-second response budget. Healthy sessions check every 5 seconds.
- The supervisor serializes requests on a bounded queue. A busy queue rejects new work. The diagnostic caller waits up to 4 seconds including queueing. Expired queued diagnostics are discarded. An already sent operation retains its 3-second IPC budget so the pipe stays correlated.
- Shutdown cancels pending work. The idle supervisor sends `shutdown`, closes stdin and waits up to 1 second. If an outstanding request is cancelled or the child does not exit, the supervisor terminates and reaps that child. The Windows Job Object also covers abrupt parent termination.
- The v1 functions are short and single-flight. A future long-running inference method will need a protocol revision or explicit cooperative cancellation handling before it ships.
