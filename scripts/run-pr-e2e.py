"""Run selected E2E fixtures in isolated subprocesses, two at a time."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import os
from pathlib import Path
import subprocess
import sys
import tempfile

FIXTURES = {
    "jev": "local-jev-e2e.py",
    "http": "http-api-e2e.py",
    "web": "web-service-e2e.py",
}


def run_fixture(suite, binary):
    fixture = Path(__file__).resolve().parent.parent / "tests" / FIXTURES[suite]
    # The fixtures each create their own SAG data directory and choose an
    # ephemeral port. Also isolate temporary files created by child processes.
    with tempfile.TemporaryDirectory(prefix=f"sag-pr-{suite}-") as temporary:
        env = {**os.environ, "TMPDIR": temporary, "TEMP": temporary, "TMP": temporary}
        command = [sys.executable, str(fixture), "--binary", str(binary)]
        if suite == "jev":
            command.append("--smoke")
        result = subprocess.run(
            command,
            env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            text=True, encoding="utf-8", errors="replace",
        )
    return suite, result.returncode, result.stdout


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("suites", nargs="+", choices=FIXTURES)
    args = parser.parse_args()
    failed = False
    with ThreadPoolExecutor(max_workers=2) as pool:
        jobs = [pool.submit(run_fixture, suite, args.binary.resolve()) for suite in args.suites]
        for job in jobs:
            suite, status, output = job.result()
            print(f"--- {suite} (exit {status}) ---", flush=True)
            print(output, end="" if output.endswith("\n") else "\n", flush=True)
            failed |= status != 0
    return int(failed)


if __name__ == "__main__":
    sys.exit(main())
