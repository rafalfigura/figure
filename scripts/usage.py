#!/usr/bin/env python3
"""How agents read code: figure calls vs raw source reads, per Claude Code session.

Reads the session transcripts Claude Code keeps in ~/.claude/projects/<project>/*.jsonl and
counts, per session, every way an agent looked at Rust source:

  figure    figure map/show/deps/howto/out/check, from the shell or the MCP tools
  ranged    Read of a .rs file with offset/limit (what `show`'s file:start-end is for)
  whole     Read of a whole .rs file
  shell     cat/head/tail/sed/grep/rg/less/nl on .rs files or src/
  grep      the Grep tool
  blocked   calls the figure guard hook turned away

The last column is (figure + ranged) / all of them: the share of code reading that went
through figure or a range it named. Run it before and after a change to compare.

  scripts/usage.py                    # the project of the current directory
  scripts/usage.py ~/dev/test2        # another project
  scripts/usage.py . --since 2026-09-01 --all
"""

import argparse
import glob
import json
import os
import re
import sys

FIGURE_SHELL = re.compile(r"(?:^|[\s;&|(])(?:figure|cargo run -q --)\s+(map|show|deps|howto|out|check)\b")
RAW_SHELL = re.compile(r"(?:^|[\s;&|(])(cat|head|tail|sed|grep|rg|less|nl|bat)\s[^;&|]*?(\.rs\b|\bsrc/)")
GUARD_MARK = "figure guard:"


def transcript_dir(project):
    """~/.claude/projects/<path with every non-alphanumeric char replaced by '-'>."""
    path = os.path.abspath(project)
    return os.path.expanduser("~/.claude/projects/" + re.sub(r"[^A-Za-z0-9]", "-", path))


def tool_uses(record):
    msg = record.get("message")
    if record.get("type") != "assistant" or not isinstance(msg, dict):
        return []
    content = msg.get("content")
    if not isinstance(content, list):
        return []
    return [c for c in content if isinstance(c, dict) and c.get("type") == "tool_use"]


def tool_results(record):
    msg = record.get("message")
    if record.get("type") != "user" or not isinstance(msg, dict):
        return []
    content = msg.get("content")
    if not isinstance(content, list):
        return []
    return [c for c in content if isinstance(c, dict) and c.get("type") == "tool_result"]


def result_text(result):
    content = result.get("content")
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        return " ".join(c.get("text", "") for c in content if isinstance(c, dict))
    return ""


def count(path, since):
    c = dict(figure=0, ranged=0, whole=0, shell=0, grep=0, blocked=0)
    first = None
    for line in open(path, encoding="utf-8", errors="replace"):
        try:
            record = json.loads(line)
        except ValueError:
            continue
        stamp = record.get("timestamp")
        if stamp and first is None:
            first = stamp[:10]
        for use in tool_uses(record):
            name, args = use.get("name", ""), use.get("input") or {}
            if name.startswith("mcp__") and "figure" in name:
                c["figure"] += 1
            elif name == "Bash":
                command = args.get("command") or ""
                c["figure"] += len(FIGURE_SHELL.findall(command))
                c["shell"] += len(RAW_SHELL.findall(command))
            elif name == "Read" and (args.get("file_path") or "").endswith(".rs"):
                c["ranged" if args.get("offset") or args.get("limit") else "whole"] += 1
            elif name == "Grep":
                c["grep"] += 1
        for result in tool_results(record):
            if GUARD_MARK in result_text(result):
                c["blocked"] += 1
    if since and (first or "") < since:
        return None, None
    return first, c


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("project", nargs="?", default=".", help="project directory (default: .)")
    parser.add_argument("--since", help="only sessions started on or after YYYY-MM-DD")
    parser.add_argument("--all", action="store_true", help="also list sessions that read no Rust")
    args = parser.parse_args()

    folder = transcript_dir(args.project)
    files = glob.glob(os.path.join(folder, "*.jsonl"))
    if not files:
        sys.exit(f"no transcripts in {folder}")

    rows = []
    for path in files:
        first, c = count(path, args.since)
        if c and (args.all or any(c.values())):
            rows.append((first or "?", os.path.basename(path)[:8], c))
    rows.sort()

    keys = ["figure", "ranged", "whole", "shell", "grep", "blocked"]
    print(f"{'date':<10}  {'session':<8}  " + "  ".join(f"{k:>7}" for k in keys) + "  figure%")
    total = dict.fromkeys(keys, 0)
    for first, name, c in rows:
        for k in keys:
            total[k] += c[k]
        print(f"{first:<10}  {name:<8}  " + "  ".join(f"{c[k]:>7}" for k in keys) + f"  {share(c):>7}")
    print(f"{'total':<10}  {len(rows):<8}  " + "  ".join(f"{total[k]:>7}" for k in keys) + f"  {share(total):>7}")


def share(c):
    via_figure = c["figure"] + c["ranged"]
    reads = via_figure + c["whole"] + c["shell"] + c["grep"]
    return f"{100 * via_figure // reads}%" if reads else "-"


if __name__ == "__main__":
    main()
