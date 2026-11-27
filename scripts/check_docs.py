#!/usr/bin/env python3
"""Keep the documentation honest.

Every `make <target>` named in the documents must be a target of the Makefile,
every file path written in backticks must exist, every Renode scenario must be
described in docs/verification/renode-scenarios.md, and the traceability matrix
must cover REQ-001 to REQ-005.
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

INLINE = re.compile(r"`([^`\n]+)`")
PATH = re.compile(r"^(firmware|hdl|sim|scripts|tools|docs|test)/[A-Za-z0-9_./+-]+$")
MAKE_IN_SPAN = re.compile(r"\bmake\s+([a-z][a-z0-9-]*)")
MAKE_IN_FENCE = re.compile(r"^\s*(?:\$\s*)?make\s+([a-z][a-z0-9-]*)")


def make_targets():
    text = (ROOT / "Makefile").read_text()
    # "name:" with no space before the colon and not ":=" (variable assignment).
    return set(re.findall(r"^([A-Za-z0-9][A-Za-z0-9_.-]*):(?!=)", text, re.M))


def scenario_names():
    names = set()
    for rel in ("scripts/renode_prepare.py", "scripts/renode_chain.py", "scripts/renode_update.py"):
        text = (ROOT / rel).read_text()
        block = re.search(r"SCENARIOS\s*=\s*\{(.*?)\n\}", text, re.S)
        if block:
            names |= set(re.findall(r'^\s+"([a-z0-9-]+)":', block.group(1), re.M))
    return names


def doc_files():
    files = {ROOT / "README.md"}
    files |= set((ROOT / "docs").rglob("*.md"))
    return sorted(f for f in files if f.exists())


def main():
    problems = []
    targets = make_targets()

    for doc in doc_files():
        rel = doc.relative_to(ROOT)
        in_fence = False
        for number, line in enumerate(doc.read_text().splitlines(), 1):
            if line.strip().startswith("```"):
                in_fence = not in_fence
                continue
            if in_fence:
                m = MAKE_IN_FENCE.match(line)
                if m and m.group(1) not in targets:
                    problems.append(f"{rel}:{number}: no make target '{m.group(1)}'")
                continue
            for span in INLINE.findall(line):
                span = span.strip()
                m = MAKE_IN_SPAN.search(span)
                if m and m.group(1) not in targets:
                    problems.append(f"{rel}:{number}: no make target '{m.group(1)}'")
                if PATH.match(span) and not (ROOT / span).exists():
                    problems.append(f"{rel}:{number}: path does not exist: {span}")

    scenarios_doc = ROOT / "docs" / "verification" / "renode-scenarios.md"
    scenarios_text = scenarios_doc.read_text() if scenarios_doc.exists() else ""
    for name in sorted(scenario_names()):
        if name not in scenarios_text:
            problems.append(f"scenario '{name}' is not described in {scenarios_doc.relative_to(ROOT)}")

    matrix = ROOT / "docs" / "compliance" / "traceability.md"
    matrix_text = matrix.read_text() if matrix.exists() else ""
    for n in range(1, 6):
        if f"REQ-00{n}" not in matrix_text:
            problems.append(f"REQ-00{n} is missing from the traceability matrix")

    if problems:
        print("DOCUMENTATION PROBLEMS:")
        for p in problems:
            print("  -", p)
        return 1
    print(f"[ok] {len(doc_files())} documents, {len(targets)} make targets, "
          f"{len(scenario_names())} scenarios: all references exist")
    return 0


sys.exit(main())