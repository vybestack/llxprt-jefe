#!/usr/bin/env python3
"""Derive first-agent tutorial publication assets from one canonical runner report."""

import argparse
import html
import json
import os
import pathlib
import re
import socket

CAPTURES = (
    ("first-agent-dashboard", ("Agent Types", "Code Puppy  Installed", "LLxprt  Installed"), (), "last"),
    ("first-agent-new-repository", ("New Repository", "LLxprt Jefe", "vybestack/llxprt-jefe"), (), "last"),
    ("first-agent-new-agent", ("New Agent", "Tutorial LLxprt", "core.llxprt"), (), "last"),
    ("first-agent-terminal-ready", ("tutorial-shim: ready", "LLxprt Jefe (1)"), ("response:",), "first"),
    ("first-agent-terminal-response", ("tutorial-shim: response: hello from the", "LLxprt Jefe (1)"), ("received issue",), "first"),
    ("first-agent-result", ("tutorial-shim: response: hello from the", "LLxprt Jefe (1)"), ("received issue",), "last"),
    ("first-agent-code-puppy", ("New Agent", "Tutorial Puppy", "core.code-puppy"), (), "last"),
    ("first-agent-issues", ("#352", "OPEN", "labels: documentation, enhancement"), ("received issue",), "last"),
    ("first-agent-issue-send", ("tutorial-shim: received issue 352", "LLxprt Jefe (2)"), (), "last"),
    ("first-agent-pull-request", ("#353", "decision: APPROVED", "OPEN"), ("Merge Pull Request #353",), "last"),
    ("first-agent-pr-merge", ("Merge Pull Request #353", "Squash and merge"), (), "last"),
)
PUBLISHED = {
    "first-agent-new-repository",
    "first-agent-new-agent",
    "first-agent-result",
    "first-agent-code-puppy",
    "first-agent-issues",
    "first-agent-issue-send",
    "first-agent-pull-request",
    "first-agent-pr-merge",
}
SECRET_PATTERN = re.compile(
    r"(?i)(authorization:|bearer\s|gh[pousr]_[A-Za-z0-9_]+|github_pat_|api[_-]?key|access[_-]?token|password\s*[=:])"
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--report", required=True, type=pathlib.Path)
    parser.add_argument("--root", required=True, type=pathlib.Path)
    return parser.parse_args()


def select_frame(frames: list[dict], required: tuple[str, ...], absent: tuple[str, ...], choice: str) -> list[str]:
    matches = []
    for frame in frames:
        lines = frame.get("lines")
        if not isinstance(lines, list) or not all(isinstance(line, str) for line in lines):
            raise RuntimeError("report frame lines must be strings")
        text = "\n".join(lines)
        if all(value in text for value in required) and not any(value in text for value in absent):
            matches.append(lines)
    if not matches:
        raise RuntimeError(f"canonical report has no frame matching {required!r}")
    return matches[0] if choice == "first" else matches[-1]


def sanitize(lines: list[str], workspace: str) -> list[str]:
    home = os.path.expanduser("~")
    replacements = (
        (f"{workspace}/home", "~"),
        (workspace, "<workspace>"),
        (str(pathlib.Path(__file__).resolve().parents[1]), "<repository>"),
        (home, "~"),
        (os.environ.get("USER", ""), "<user>"),
        (socket.gethostname(), "<host>"),
    )
    result = []
    for line in lines:
        for source, target in replacements:
            if source:
                line = line.replace(source, target)
        line = re.sub(r"(?:/private)?/tmp/jefe-harness-[^\s│]*", "<workspace>", line)
        line = re.sub(
            r"pid:([0-9]+)", lambda match: f"pid:{'x' * len(match.group(1))}", line
        )
        line = re.sub(
            r"status:([0-9:]+)",
            lambda match: f"status:{'x' * len(match.group(1))}",
            line,
        )
        line = line.rstrip()
        if len(line) > 100:
            raise RuntimeError(f"sanitized tutorial line exceeds 100 columns: {line!r}")
        if SECRET_PATTERN.search(line):
            raise RuntimeError("credential-like text found in tutorial publication frame")
        result.append(line)
    if len(result) != 32:
        raise RuntimeError(f"tutorial frame must contain exactly 32 rows, found {len(result)}")
    return result


def render_svg(lines: list[str]) -> str:
    rows = []
    for index, line in enumerate(lines):
        escaped = html.escape(line, quote=False)
        rows.append(f'  <text x="24" y="{38 + index * 18}" xml:space="preserve">{escaped}</text>')
    body = "\n".join(rows)
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="848" height="610" viewBox="0 0 848 610" role="img" aria-label="Jefe terminal capture">
<rect width="848" height="610" rx="12" fill="#111827"/>
<g fill="#e5e7eb" font-family="Menlo, Monaco, Consolas, monospace" font-size="12">
{body}
</g>
</svg>
'''


def main() -> int:
    args = parse_args()
    report = json.loads(args.report.read_text())
    if report.get("schema") != 1 or report.get("status") != "passed":
        raise RuntimeError("tutorial publication requires a passed schema-1 report")
    frames = report.get("frames")
    workspace = report.get("workspace")
    if not isinstance(frames, list) or not isinstance(workspace, str) or not workspace:
        raise RuntimeError("tutorial report is missing frames or workspace identity")
    evidence = args.root / "evidence"
    private = args.root / "private"
    publication = args.root / "publication"
    for directory in (evidence, private, publication):
        directory.mkdir(parents=True, exist_ok=True)
    for name, required, absent, choice in CAPTURES:
        lines = sanitize(select_frame(frames, required, absent, choice), workspace)
        body = "\n".join(lines) + "\n"
        (evidence / f"{name}.screen.txt").write_text(body)
        if name in PUBLISHED:
            (private / f"{name}.publication.txt").write_text(body)
            (publication / f"{name}.svg").write_text(render_svg(lines))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
