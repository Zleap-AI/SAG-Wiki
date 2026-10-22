#!/usr/bin/env python3
"""Exercise the real SAG service and CLI against a local Jev protocol fixture.

Build first: cargo build --manifest-path cli/Cargo.toml --locked
Run: python3 tests/local-jev-e2e.py --binary cli/target/debug/sag

Only the remote model is simulated. Configuration, persistence, indexing,
connection tests, SAG2 scoring and CLI processes use the production code.
This verifies integration, not real Jev inference quality or performance.
"""

import argparse
import contextlib
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


MODEL = "local-jev-e2e"
FIXTURE_KEY = "local-e2e-fixture-key"
QUESTION = "What does Vega release require before deployment?"
DOCUMENTS = {
    "background.md": "# Vega background\nVega release office plants are watered on Thursdays.\n",
    "checklist.md": "# Vega release policy\nVega release requires the signed checklist VX-417 before deployment.\n",
    "approval.md": "# Vega release approval\nVega release approval is recorded by the release owner.\n",
}


class JevFixture:
    def __init__(self):
        self.auth_mode = "none"
        self.behavior = "valid"
        self.calls = []
        self.violations = []
        fixture = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_args):
                pass

            def do_POST(self):
                status, response = 200, None
                try:
                    payload = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                    if self.path not in ("/v1/systemone", "/decisions"):
                        status, response = 404, {"error": "unexpected Jev endpoint"}
                    elif fixture.behavior == "unauthorized":
                        status, response = 401, {"error": "fixture rejected credentials"}
                    else:
                        header = self.headers.get("Authorization")
                        if fixture.auth_mode == "none":
                            assert header is None, "none authentication sent an Authorization header"
                        else:
                            assert header == f"Bearer {FIXTURE_KEY}", "bearer authentication omitted its configured key"
                        assert payload["model"] == MODEL, "configured model did not reach Jev"
                        assert isinstance(payload["state"], str), "Jev state must be a string"
                        questions = payload["questions"]
                        assert isinstance(questions, dict) and questions, "Jev questions must not be empty"
                        answers = {}
                        for identifier, question in questions.items():
                            assert question["type"] == "noul", "Jev question must request Noul scoring"
                            assert set(question["criteria"]) == {"true", "false"}, "Jev criteria are incomplete"
                            assert isinstance(question["instructions"], dict), "Jev instructions must be structured"
                            if identifier == "test":
                                assert payload["state"] == "Connection test", "connection probe state is incorrect"
                                score = 0.99
                            else:
                                assert identifier.startswith("candidate_"), "unexpected candidate identifier"
                                state = json.loads(payload["state"])
                                assert state["original_question"] == QUESTION, "search query did not reach Jev"
                                assert isinstance(state["previously_retrieved_evidence"], list)
                                passage = question["instructions"]["candidate_passage"]
                                assert isinstance(passage, str) and passage, "candidate passage was not sent"
                                score = 0.98 if "VX-417" in passage else 0.65 if "release owner" in passage else 0.02
                            answers[identifier] = {"type": "noul", "noul": score}
                        response = {"answers": answers}
                        if fixture.behavior == "invalid_score":
                            for answer in answers.values():
                                answer["noul"] = "invalid"
                        elif fixture.behavior == "missing_score":
                            answers.pop(next(iter(answers)))
                        elif fixture.behavior == "timeout":
                            time.sleep(2)
                except (AssertionError, KeyError, TypeError, ValueError) as error:
                    fixture.violations.append(str(error))
                    status, response = 422, {"error": str(error)}
                fixture.calls.append({
                    "path": self.path,
                    "authorization_present": self.headers.get("Authorization") is not None,
                    "status": status,
                    "behavior": fixture.behavior,
                })
                body = json.dumps(response).encode()
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.send_header("Connection", "close")
                self.end_headers()
                with contextlib.suppress(BrokenPipeError, ConnectionResetError):
                    self.wfile.write(body)

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.server.daemon_threads = True
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)

    @property
    def origin(self):
        return f"http://127.0.0.1:{self.server.server_port}"

    def __enter__(self):
        self.thread.start()
        return self

    def __exit__(self, *_args):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=5)


class CoreError(RuntimeError):
    pass


class CoreClient:
    """Use the existing authenticated core protocol, including chunked frames."""

    def __init__(self, data):
        endpoint = json.loads((data / "sag-service.json").read_text(encoding="utf-8"))
        self.socket = socket.create_connection(("127.0.0.1", endpoint["port"]), timeout=15)
        self.reader = self.socket.makefile("rb")
        self.counter = 0
        self.send({"frame": "hello", "protocol_version": endpoint["protocol_version"], "token": endpoint["token"]})
        assert self.receive()["frame"] == "welcome", "real core service did not accept its own endpoint token"

    def send(self, frame):
        payload = json.dumps(frame, ensure_ascii=False)
        for offset in range(0, len(payload), 8192):
            part = {"data": payload[offset:offset + 8192], "last": offset + 8192 >= len(payload)}
            self.socket.sendall((json.dumps(part) + "\n").encode())

    def receive(self):
        pieces = []
        while True:
            line = self.reader.readline()
            assert line, "real core service closed before returning a response"
            part = json.loads(line)
            pieces.append(part["data"])
            if part["last"]:
                return json.loads("".join(pieces))

    def call(self, operation, args=None):
        self.counter += 1
        identifier = str(self.counter)
        request = {"operation": operation}
        if args is not None:
            request["args"] = args
        self.send({"frame": "request", "id": identifier, "operation": request})
        while True:
            frame = self.receive()
            if frame["frame"] == "response" and frame["id"] == identifier:
                if "Err" in frame["result"]:
                    raise CoreError(frame["result"]["Err"])
                output = frame["result"]["Ok"]
                return output.get("data")

    def close(self):
        self.reader.close()
        self.socket.close()

    def __enter__(self):
        return self

    def __exit__(self, *_args):
        self.close()


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def run(binary):
    completed = []
    with tempfile.TemporaryDirectory(prefix="sag-local-jev-e2e-") as temporary, JevFixture() as fixture:
        temporary = Path(temporary).resolve()
        data, documents = temporary / "data", temporary / "documents"
        documents.mkdir()
        for filename, text in DOCUMENTS.items():
            (documents / filename).write_text(text, encoding="utf-8")
        environment = {**os.environ, "SAG_DATA_DIR": str(data), "SAG_DESKTOP_DATA_DIR": str(data)}

        def cli(*args, input=None, success=True):
            process = subprocess.run([str(binary), *args], cwd=documents, env=environment,
                                     input=input, encoding="utf-8", capture_output=True, timeout=35)
            require((process.returncode == 0) == success,
                    f"sag {' '.join(args)} exited {process.returncode}: {process.stderr}")
            return process

        def settings():
            with CoreClient(data) as core:
                return core.call("get_settings")

        def test_connection(config=None):
            with CoreClient(data) as core:
                return core.call("test_model", {"kind": "jev", "config": config or settings()["jev"], "key": ""})

        def expect_core_error(action, phrase):
            try:
                action()
            except CoreError as error:
                require(phrase in str(error), f"expected {phrase!r} in core error, got {error}")
            else:
                raise AssertionError(f"core unexpectedly accepted {phrase!r} scenario")

        def search_cli(success=True, timeout="10"):
            before = len(fixture.calls)
            process = cli(QUESTION, "--json", "--timeout", timeout, success=success)
            if success:
                output = json.loads(process.stdout)
                require(output["results"], "CLI semantic search returned no real indexed document")
                require("VX-417" in output["results"][0]["content"], "CLI ranking did not select Jev's highest-scored document")
                require(len(fixture.calls) > before, "CLI search never reached the HTTP Jev fixture")
            return process

        def deep_search():
            answer_model = settings()["answer"]["model"]
            with CoreClient(data) as core:
                output = core.call("run_query", {"query": {
                    "id": f"local-jev-e2e-{time.time_ns()}", "kind": "deep_search", "question": QUESTION,
                    "scope": [str(documents)], "references": [], "model": answer_model, "conversation_id": None,
                    "replace_from": None, "page": None,
                }})
            candidates = output["results"]
            require(candidates, "core deep search returned no indexed candidates")
            require(Path(candidates[0]["path"]).name == "checklist.md", "core deep search ignored configured Jev scores")
            require(candidates[0]["score"] == 0.98, "real core did not preserve the returned Noul score")
            require(output["trace"], "core search did not execute the real retrieval pipeline")

        try:
            configured = json.loads(cli("config", "--url", fixture.origin + "/v1", "--model", MODEL,
                                        "--auth", "none").stdout)
            require(configured["auth_mode"] == "none" and not configured["has_key"],
                    "fresh no-auth configuration unexpectedly requires a stored key")
            require(test_connection() == "连接成功", "fresh no-auth connection test tried to load a nonexistent key")
            current = settings()
            current["retrieval"]["max_hops"] = 0
            current["search_summary_enabled"] = False
            with CoreClient(data) as core:
                core.call("save_settings", {"settings": current, "answer_key": "", "jev_key": "", "mcp_credentials": None})
            indexed = json.loads(cli("add", "--json").stdout)
            require(indexed["index_status"] == "ready", "real index did not complete")
            coverage = json.loads(cli("check", "--json").stdout)
            require(coverage["indexed_files"] == len(DOCUMENTS) and coverage["index_ready"], "fixture documents were not really indexed")
            deep_search()
            search_cli()
            completed.append("fresh no-key config, TestModel, SaveSettings, real indexing, deep and CLI search")

            # Store a real fixture key through stdin; the none-mode checks below
            # must still prove that this saved key cannot leak to a local Jev.
            fixture.auth_mode = "bearer"
            configured = json.loads(cli("config", "--url", fixture.origin + "/v1", "--model", MODEL,
                                        "--auth", "bearer", "--key-stdin", input=FIXTURE_KEY + "\n").stdout)
            require(configured["auth_mode"] == "bearer" and configured["has_key"], "CLI did not persist bearer configuration")
            require(test_connection() == "连接成功", "real core's connection test did not accept valid Jev Noul")
            completed.append("bearer key via stdin and real TestModel HTTP call")

            fixture.auth_mode = "none"
            configured = json.loads(cli("config", "--auth", "none").stdout)
            require(configured["auth_mode"] == "none", "CLI did not switch Jev authentication mode")
            current = settings()
            with CoreClient(data) as core:
                core.call("save_settings", {"settings": current, "answer_key": "", "jev_key": "", "mcp_credentials": None})
            require(test_connection() == "连接成功", "none mode tried to require or reuse a saved key")
            require(json.loads(cli("config", "--if-needed").stdout)["auth_mode"] == "none", "--if-needed does not recognize configured no-auth Jev")
            completed.append("none mode via CLI and SaveSettings without leaking the saved key")

            for suffix in ("/v1", "/v1/", "/v1/systemone", "/v1/systemone/", "/decisions", "/decisions/"):
                cli("config", "--url", fixture.origin + suffix)
                require(test_connection() == "连接成功", f"connection test failed for {suffix}")
                deep_search()
                search_cli()
            completed.append("six base/full/trailing-slash endpoints: TestModel, deep search, CLI search")

            cli("service", "restart")
            persisted = settings()["jev"]
            require(persisted["url"] == fixture.origin + "/decisions/" and persisted["model"] == MODEL
                    and persisted["auth_mode"] == "none", "service restart lost the local Jev configuration")
            require(test_connection() == "连接成功", "persisted configuration could not reconnect")
            deep_search()
            search_cli()
            completed.append("real service restart preserves config, index, connection and retrieval")

            fixture.behavior = "unauthorized"
            expect_core_error(test_connection, "401")
            require("401" in search_cli(success=False).stderr, "CLI hid the HTTP 401 model failure")
            fixture.behavior = "valid"
            wrong = {**settings()["jev"], "url": fixture.origin + "/missing"}
            expect_core_error(lambda: test_connection(wrong), "404")
            cli("config", "--url", fixture.origin + "/missing")
            require("404" in search_cli(success=False).stderr, "CLI hid the HTTP 404 endpoint failure")
            cli("config", "--url", fixture.origin + "/v1/systemone")
            completed.append("HTTP 401 and 404 fail both connection testing and real CLI retrieval")

            fixture.behavior = "invalid_score"
            expect_core_error(test_connection, "评分")
            require("分数" in search_cli(success=False).stderr, "CLI accepted invalid Noul data")
            fixture.behavior = "missing_score"
            expect_core_error(test_connection, "评分")
            require("候选数量" in search_cli(success=False).stderr, "CLI accepted a missing candidate score")
            fixture.behavior = "timeout"
            require("超时" in search_cli(success=False, timeout="1").stderr, "CLI did not report the query deadline")
            # Allow the intentionally slow handler to finish before resetting the fixture.
            time.sleep(2.1)
            fixture.behavior = "valid"
            require(test_connection() == "连接成功", "failed requests broke subsequent model connections")
            search_cli()
            completed.append("invalid/missing Noul and query timeout fail; subsequent requests recover")

            require(not fixture.violations, "Jev protocol contract violations: " + "; ".join(fixture.violations))
            print(json.dumps({"status": "passed", "model": "local protocol fixture (not real inference)",
                              "checks": completed, "http_requests": len(fixture.calls),
                              "protocol_violations": fixture.violations}, ensure_ascii=False, indent=2))
        finally:
            if (data / "sag-service.json").exists():
                cli("service", "stop")


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    binary_name = "sag.exe" if os.name == "nt" else "sag"
    parser.add_argument("--binary", type=Path, default=Path(__file__).resolve().parents[1] / "cli/target/debug" / binary_name)
    parser.add_argument("--serve", action="store_true", help="run only the strict no-auth Jev fixture for desktop UI acceptance")
    arguments = parser.parse_args()
    if arguments.serve:
        with JevFixture() as fixture:
            print(json.dumps({"base_url": fixture.origin + "/v1", "full_url": fixture.origin + "/v1/systemone",
                              "model": MODEL, "auth_mode": "none"}), flush=True)
            try:
                threading.Event().wait()
            except KeyboardInterrupt:
                pass
        return
    binary = arguments.binary.resolve()
    if not binary.is_file():
        parser.error(f"CLI binary does not exist: {binary}; build cli/Cargo.toml first")
    run(binary)


if __name__ == "__main__":
    main()
