#!/usr/bin/env python3
"""Offline Linux benchmark with generated content and ten fresh destinations.

All artifacts are retained in the repository's ignored .dev/benchmarks folder.
No removable media, mounts, cloud services or game downloads are involved.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mib-per-game", type=int, default=2, choices=range(1, 65))
    parser.add_argument("--workers", type=int, nargs="+", default=[1, 2, 4, 10], choices=range(1, 11))
    args = parser.parse_args()
    repo = Path(__file__).resolve().parent.parent
    binary = repo / "target/debug/usb"
    if not binary.is_file():
        parser.error("build first: cargo build --workspace --locked")
    if len(args.workers) > 10:
        parser.error("at most ten benchmark configurations")
    base = repo / ".dev/benchmarks"
    base.mkdir(parents=True, exist_ok=True)
    estimated = (1 + 10 * len(args.workers)) * 2 * args.mib_per_game * 1024**2
    if shutil.disk_usage(base).free < estimated + 1024**3:
        parser.error("insufficient space for retained fixtures plus a 1 GiB reserve")
    root = Path(tempfile.mkdtemp(prefix="batch-", dir=base))
    archive = root / "archive"
    archive.mkdir()
    (archive / "archive.json").write_text(json.dumps({"schema_version": 1,
        "platforms": [{"id": "p", "name": "Generated"}],
        "collections": [{"id": "c", "name": "Benchmark"}]}))
    payload = bytes(range(256)) * 4096
    for index in range(2):
        game = archive / f"fixture-{index}"
        content = game / "content"
        content.mkdir(parents=True)
        (content / "empty").mkdir()
        with (content / "data.bin").open("wb") as output:
            for _ in range(args.mib_per_game):
                output.write(payload)
        (content / "LICENCE.txt").write_text("Original generated benchmark fixture; freely redistributable.\n")
        (game / "game.json").write_text(json.dumps({"schema_version": 1, "id": game.name,
            "name": game.name, "platform": "p", "collection": "c",
            "redistribution_approved": True, "approval_note": "Original generated fixture"}))
    results = []
    for index, workers in enumerate(args.workers):
        run = root / f"run-{index}-workers-{workers}"
        run.mkdir()
        command = [str(binary), "--archive", str(archive), "--json", "copy-batch",
                   "--workers", str(workers), "--filesystem", "ext4"]
        for port in range(10):
            destination = run / f"port-{port + 1}"
            destination.mkdir()
            command += ["--destination", str(destination)]
        started = time.perf_counter()
        with (run / "report.json").open("wb") as stdout, (run / "events.jsonl").open("wb") as stderr:
            process = subprocess.Popen(command, stdout=stdout, stderr=stderr)
            _, status, usage = os.wait4(process.pid, 0)
            process.returncode = os.waitstatus_to_exitcode(status)
        elapsed = time.perf_counter() - started
        if process.returncode:
            raise RuntimeError(f"batch failed; retain and inspect {run}")
        report = json.loads((run / "report.json").read_text())
        if report["status"] != "completed" or len(report["jobs"]) != 10 or any(
            len(job["report"]["completed"]) != 2 for job in report["jobs"]
        ):
            raise RuntimeError(f"unexpected completion report: {run}")
        result = {"workers": workers, "workers_started": report["workers_started"],
            "event_queue_capacity": report["event_queue_capacity"], "wall_seconds": elapsed,
            "confirmed_payload_bytes": report["confirmed_payload_bytes"],
            "payload_mib_per_second": report["confirmed_payload_bytes"] / 1024**2 / elapsed,
            "maximum_rss_kib": usage.ru_maxrss, "user_seconds": usage.ru_utime,
            "system_seconds": usage.ru_stime}
        results.append(result)
        print(json.dumps(result), flush=True)
    (root / "summary.json").write_text(json.dumps({"fixture_mib_per_game": args.mib_per_game,
        "jobs": 10, "games_per_job": 2, "results": results}, indent=2) + "\n")
    print(f"Artifacts retained: {root}")


if __name__ == "__main__":
    main()
