"""Run explicit capacity acceptance cases sequentially on the persistent Linux workspace.

Build first with cargo test --release --lib --no-run, then pass that test executable.
Uses wait4 per case, not cumulative build-process resource statistics. No dependency
installation, sparse payload substitution, global settings or background service.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--cases", nargs="+", choices=["small", "medium", "beyond_former_ceilings"],
                        default=["small", "medium", "beyond_former_ceilings"])
    args = parser.parse_args()
    if sys.platform != "linux":
        parser.error("capacity evidence requires the configured persistent Linux workspace")
    root = Path(__file__).resolve().parent.parent
    binary = args.binary.resolve(strict=True)
    evidence = root / "target" / "snapshot-capacity-evidence-v10"
    evidence.mkdir(exist_ok=True)
    digest = hashlib.file_digest(binary.open("rb"), "sha256").hexdigest()
    for case in args.cases:
        free = shutil.disk_usage(root).free
        required = (160 if case == "beyond_former_ceilings" else 12) * 1024**3
        # ZFS can report removed temporary data until its next transaction group.
        # Wait for reclamation; do not lower the preflight requirement or delete
        # unrelated files to make the next case fit.
        deadline = time.monotonic() + 30
        while free < required and time.monotonic() < deadline:
            time.sleep(2)
            free = shutil.disk_usage(root).free
        if free < required:
            raise SystemExit(f"Blocked: {case} requires {required} free bytes; available={free}")
        test = "hook::tests::restore_runtime::snapshot_capacity_" + case
        started = time.monotonic()
        log = evidence / f"{case}.log"
        with log.open("w") as output:
            child = subprocess.Popen([str(binary), "--exact", test, "--ignored", "--nocapture",
                                      "--test-threads=1"], cwd=root, stdout=output, stderr=subprocess.STDOUT)
            _, status, usage = os.wait4(child.pid, 0)
            child.returncode = os.waitstatus_to_exitcode(status)
        result = dict(case=case, test=test, executable_sha256=digest, exit_code=child.returncode,
                      elapsed_seconds=round(time.monotonic() - started, 3),
                      peak_rss_kib=usage.ru_maxrss, free_bytes_before=free,
                      free_bytes_after=shutil.disk_usage(root).free)
        (evidence / f"{case}.json").write_text(json.dumps(result, indent=2) + "\n")
        print(json.dumps(result), flush=True)
        if child.returncode or "1 passed; 0 failed" not in log.read_text():
            raise SystemExit(f"Failed capacity case; inspect {log}")
        # Pair measured RSS with the test's zero-inline-chunk and bounded
        # control-database/WAL assertions; finite sizes alone are not the proof.
        if usage.ru_maxrss > 128 * 1024:
            raise SystemExit(f"Failed bounded-memory acceptance: {case} exceeded 128 MiB peak RSS")


if __name__ == "__main__":
    main()
