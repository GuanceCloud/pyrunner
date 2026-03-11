---
name: pyrunner
description: Use this skill when working with the pyrunner CLI to register, search, retrieve, update, clean, or locally debug cached Python scripts for AI agents. Especially useful when an agent needs to reuse an existing Python script by calling `pyrunner search`, `pyrunner ai check`, `pyrunner ai get`, or `pyrunner register`.
---

# pyrunner

`pyrunner` is a local Python script cache for AI agents.

Its primary job is:

- cache scripts
- index scripts
- search reusable scripts
- return an absolute script path

Its secondary job is:

- locally run a cached script for debugging
- record lightweight usage history

## Use this skill when

- you want to avoid regenerating a Python script
- you need to find an existing script for a task
- you need a cached script's absolute path
- you want to register a newly created script for future reuse
- you want to inspect or clean the local cache

## Default workflow

For AI agents, prefer this flow:

1. `pyrunner ai check --query "..."`
2. if matched, `pyrunner ai get <script_id>`
3. run the returned `path` with the agent's Python tool
4. if not matched, create a new script and `pyrunner ai register`

Prefer agent-managed execution over `pyrunner run`.

## Core commands

### Search for reusable scripts

```bash
pyrunner search "csv statistics"
pyrunner ai search --query "csv statistics" --threshold 0.5
```

### Check whether reuse is possible

```bash
pyrunner ai check --query "download json and summarize" --threshold 0.5
```

If `exists=true`, use `path` directly.

### Get a script path

```bash
pyrunner get <script_id>
pyrunner ai get <script_id>
```

The returned `path` is absolute and intended for direct execution.

### Register a new script

From a file:

```bash
pyrunner register ./demo.py --desc "process csv" --tags "csv,data"
```

For AI agents:

```bash
pyrunner ai register --script-file ./demo.py --desc "process csv" --tags "csv,data"
pyrunner ai register --stdin --desc "process csv" < demo.py
pyrunner ai register --script-text "print('hello')" --desc "hello script"
```

### Update cache metadata

```bash
pyrunner update <script_id> --desc "new description" --tags "a,b"
```

### Inspect or clean cache

```bash
pyrunner list
pyrunner stats
pyrunner clean --dry-run
pyrunner clean --older-than 30
pyrunner delete <script_id> --yes
```

## `run` command guidance

`pyrunner run` is a convenience command for local debugging.

Use it when:

- quickly validating a cached script
- checking stdout/stderr
- recording a local debug run

Do not treat it as the primary execution model for agents unless explicitly requested.

## Output expectations

When using `ai` commands:

- expect JSON output
- use `path` as the main execution artifact
- treat `execute_command` as a convenience hint, not the primary interface

## Important behavior

- duplicate scripts are deduplicated by content hash
- search is keyword-oriented and score-based
- path values are absolute
- `delete --yes` removes both metadata and script file
- `clean --dry-run` previews candidates without deleting

## Practical rules

- write good `--desc` text; it strongly affects search quality
- use a few meaningful tags instead of many noisy tags
- lower threshold first when search misses
- prefer `ai check` for reuse decisions
- prefer `ai get` before execution
