#!/usr/bin/env python3
"""Offline test: run bin/t3-live against a fake T3 MCP server.

The fake mirrors the real nightly wire format: initialize returns an
mcp-session-id header, tool results arrive as SSE `data:` lines with
structuredContent, and t3_thread_list requires projectId.
"""
import json
import os
import subprocess
import sys
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer

TOKEN = "test-token"
calls = []


def tool(name, args):
    if name == "t3_project_list":
        return {"projects": [{"id": "p1", "title": "naarchy", "workspaceRoot": "/w/naarchy", "deletedAt": None},
                             {"id": "p2", "title": "gone", "workspaceRoot": "/w/gone", "deletedAt": "2026-01-01T00:00:00Z"}],
                "nextCursor": None}
    if name == "t3_thread_list":
        assert args.get("projectId") == "p1", args
        assert "running" in args["statuses"]
        return {"projectId": "p1", "currentThreadId": None, "nextCursor": None, "total": 2, "threads": [
            {"threadId": "t1", "title": "Fix pill", "status": "running", "latestRunId": "r1",
             "updatedAt": "2026-10-10T12:00:00.000Z"},
            {"threadId": "t2", "title": "Ask me", "status": "running", "latestRunId": "r2",
             "updatedAt": "2026-10-10T12:01:00.000Z"},
        ]}
    if name == "t3_thread_read":
        tid = args["threadId"]
        return {"thread": {"threadId": tid, "activeRunId": "r" + tid[1:], "latestRunId": "r" + tid[1:],
                           "pendingRequestCount": 1 if tid == "t2" else 0, "branch": None},
                "recentRuns": [{"runId": "r" + tid[1:], "status": "running",
                                "requestedAt": "2026-10-10T11:59:00.000Z",
                                "startedAt": "2026-10-10T11:59:30.000Z" if tid == "t1" else None}],
                "items": [], "nextPosition": None, "hasMore": False}
    raise AssertionError(name)


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def do_POST(self):
        if self.headers.get("authorization") != "Bearer " + TOKEN:
            self.send_response(401)
            self.end_headers()
            return
        body = json.loads(self.rfile.read(int(self.headers["content-length"])))
        calls.append(body.get("method"))
        if "id" not in body:
            self.send_response(202)
            self.end_headers()
            return
        if body["method"] == "initialize":
            result = {"protocolVersion": "2025-06-18", "capabilities": {}, "serverInfo": {"name": "T3 Code"}}
        else:
            assert self.headers.get("mcp-session-id") == "s1"
            p = body["params"]
            data = tool(p["name"], p["arguments"])
            result = {"content": [{"type": "text", "text": json.dumps(data)}], "structuredContent": data,
                      "isError": False}
        payload = json.dumps({"jsonrpc": "2.0", "id": body["id"], "result": result})
        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.send_header("mcp-session-id", "s1")
        self.end_headers()
        self.wfile.write(("event: message\ndata: %s\n\n" % payload).encode())


def main():
    server = HTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    here = os.path.dirname(os.path.abspath(__file__))
    with tempfile.TemporaryDirectory() as data:
        env = dict(os.environ, T3_URL="http://127.0.0.1:%d" % server.server_port, T3_MCP_TOKEN=TOKEN,
                   NAARCHY_PLUGIN_DATA=data, T3_LIVE_ONCE="1", PYTHONDONTWRITEBYTECODE="1")
        out = subprocess.run([sys.executable, os.path.join(here, "bin", "t3-live")], env=env,
                             capture_output=True, text=True, timeout=30)
        assert out.returncode == 0, out.stderr
        acts = {a["id"]: a for a in json.loads(out.stdout)}
        assert acts["t1"]["title"] == "naarchy", acts
        assert acts["t1"]["detail"] == "running · Fix pill", acts
        assert acts["t1"]["started_at"] == 1791633570, acts["t1"]  # run startedAt wins
        assert acts["t2"]["detail"] == "needs you · Ask me", acts
        assert acts["t2"]["priority"] == 70 and acts["t1"]["priority"] == 40
        assert acts["t2"]["started_at"] == 1791633540  # falls back to requestedAt
        assert calls[0] == "initialize" and calls[1] == "notifications/initialized", calls

        env["T3_MCP_TOKEN"] = "wrong"
        bad = subprocess.run([sys.executable, os.path.join(here, "bin", "t3-live")], env=env,
                             capture_output=True, text=True, timeout=30)
        assert bad.returncode == 1 and "rejected" in bad.stderr, bad.stderr
    server.shutdown()
    print("t3-live: ok")


if __name__ == "__main__":
    main()
