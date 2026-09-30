"""Conservatively select CI scope from the complete Git change, never API pages."""

import json
import os
import re
import subprocess
from pathlib import Path


def requires_runtime(paths):
    if not paths:
        return True
    for path in paths:
        if path in {"README.md", "CONTRIBUTING.md"}:
            continue
        if path.startswith("website/"):
            continue
        # Spec can embed DDL, schemas and normative fixtures; fail closed there.
        if path.startswith("docs/") and not path.startswith("docs/spec/") and path.endswith(".md"):
            continue
        return True
    return False


def changed_paths(event_name, event, head, root):
    if event_name == "pull_request":
        base = event["pull_request"]["base"]["sha"]
    elif event_name == "push":
        base = event["before"]
    else:
        raise ValueError("Unknown event requires full verification")
    if not all(re.fullmatch(r"[0-9a-fA-F]{40}", value) and int(value, 16) for value in [base, head]):
        raise ValueError("Missing or initial Git revision")
    # PR HEAD is the checked-out synthetic merge, so compare against its base.
    # --no-renames includes both names, preventing runtime-to-doc moves hiding work.
    result = subprocess.run(
        ["git", "diff", "--no-renames", "--name-only", "-z", base, head, "--"],
        cwd=root, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    return [value.decode("utf-8", errors="strict") for value in result.stdout.split(b"\0") if value]


def main():
    runtime = True
    try:
        event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text(encoding="utf-8"))
        paths = changed_paths(os.environ["GITHUB_EVENT_NAME"], event, os.environ["GITHUB_SHA"], Path.cwd())
        runtime = requires_runtime(paths)
        print(f"Changed paths: {len(paths)}; runtime verification: {runtime}")
    except (KeyError, ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"Cannot prove documentation-only scope ({type(error).__name__}); using full verification.")
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
        output.write(f"runtime={str(runtime).lower()}\n")


if __name__ == "__main__":
    main()
