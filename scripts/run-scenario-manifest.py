#!/usr/bin/env python3
"""Execute one required OS subset from the checked schema-1 evidence manifest."""

import argparse
import hashlib
import json
import os
import pathlib
import re
import shutil
import signal
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "dev-docs/testing/scenario-execution-manifest.json"
ISSUE493_PATH = "dev-docs/tmux-scenarios/v1/issue493-server-loss.json"
ISSUE493_UNIX_REASON = (
    "issue493 exercises Windows psmux shared-server loss; Unix runtimes reconcile "
    "individual sessions and cannot produce ServerLost"
)
INSTALL_NAME = re.compile(r"[A-Za-z0-9._-]{1,64}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--platform", required=True, choices=("linux", "macos"))
    parser.add_argument("--tmux-scenario", type=pathlib.Path)
    parser.add_argument("--jefe", type=pathlib.Path)
    parser.add_argument("--probe", type=pathlib.Path)
    parser.add_argument("--jsp-fixture", type=pathlib.Path)
    parser.add_argument("--shim", type=pathlib.Path)
    parser.add_argument("--reports", type=pathlib.Path)
    parser.add_argument("--shard-index", type=int, default=0)
    parser.add_argument("--shard-count", type=int, default=1)
    parser.add_argument("--verify-completion", type=pathlib.Path)
    parser.add_argument("--expected-shards", type=int)
    parser.add_argument(
        "--scenario",
        action="append",
        default=[],
        help="run one exact manifest path; repeat to select more than one",
    )
    return parser.parse_args()


def resolve_source(source: str, args: argparse.Namespace) -> pathlib.Path:
    kind, value = source.split(":", 1)
    if kind == "cargo-bin":
        binaries = {
            "jefe": args.jefe,
            "jefe-harness-probe": args.probe,
            "jefe-jsp-llxprt-fixture": args.jsp_fixture,
        }
        try:
            return binaries[value].resolve(strict=True)
        except KeyError as error:
            raise RuntimeError(f"unknown cargo binary source {source}") from error
    if kind == "repo":
        return (ROOT / value).resolve(strict=True)
    if kind == "host-path":
        resolved = shutil.which(value)
        if resolved is None:
            raise RuntimeError(f"required host command {value!r} is unavailable")
        return pathlib.Path(resolved).resolve(strict=True)
    raise RuntimeError(f"unknown install source {source}")


def require_object(value: object, context: str, keys: set[str]) -> dict:
    if not isinstance(value, dict) or set(value) != keys:
        raise RuntimeError(f"{context} shape differs")
    return value


def validate_step_sequence(entry: dict, report_steps: list[dict], errors: list[str]) -> None:
    scenario = json.loads((ROOT / entry["path"]).read_text())
    expected_operations = [step["op"] for step in scenario["steps"]]
    expected = entry["expect"]
    failure = expected["failed_step"]
    expected_count = len(expected_operations) if failure is None else failure["index"] + 1
    if len(report_steps) != expected_count:
        errors.append(f"report step count={len(report_steps)}, expected={expected_count}")
        return
    for index, step in enumerate(report_steps):
        if step.get("index") != index or step.get("op") != expected_operations[index]:
            errors.append(
                f"report step {index} identity differs: "
                f"index={step.get('index')!r}, op={step.get('op')!r}"
            )
            continue
        if failure is not None and index == failure["index"]:
            error = step.get("error")
            if (
                step.get("status") != "failed"
                or step.get("op") != failure["op"]
                or not isinstance(error, str)
                or not error.startswith(failure["error_prefix"])
            ):
                errors.append(f"report failed step {index} differs from expectation")
        elif step.get("status") != "passed" or step.get("error") is not None:
            errors.append(f"report step {index} must be an error-free pass")


def validate_report(entry: dict, report: dict, returncode: int) -> None:
    expected = entry["expect"]
    errors = []
    if not isinstance(report, dict):
        raise RuntimeError("report must be a JSON object")
    if returncode != expected["exit_code"]:
        errors.append(f"exit={returncode}, expected={expected['exit_code']}")
    if report.get("schema") != 1:
        errors.append(f"report schema={report.get('schema')!r}, expected=1")
    if report.get("status") != expected["report_status"]:
        errors.append(
            f"report status={report.get('status')!r}, expected={expected['report_status']!r}"
        )
    steps = report.get("steps")
    if not isinstance(steps, list) or any(not isinstance(step, dict) for step in steps):
        raise RuntimeError("report steps must be a list of objects")
    validate_step_sequence(entry, steps, errors)
    captures = report.get("captures")
    if not isinstance(captures, list) or any(not isinstance(capture, dict) for capture in captures):
        raise RuntimeError("report captures must be a list of objects")
    capture_names = [capture.get("name") for capture in captures]
    if capture_names != expected["capture_names"]:
        errors.append(f"capture names={capture_names!r}, expected={expected['capture_names']!r}")
    if errors:
        raise RuntimeError("; ".join(errors))


def report_name(scenario_path: str) -> str:
    relative = pathlib.PurePosixPath(scenario_path)
    return "__".join(relative.with_suffix("").parts) + ".json"


def run_command(command: list[str], timeout: float) -> subprocess.CompletedProcess[str]:
    process = subprocess.Popen(
        command,
        cwd=ROOT,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        env={"PATH": os.environ.get("PATH", "")},
        start_new_session=True,
    )
    try:
        stdout, stderr = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGTERM)
        try:
            stdout, stderr = process.communicate(timeout=10)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            stdout, stderr = process.communicate()
            raise RuntimeError(
                "scenario exceeded its outer timeout and ignored the cleanup grace; "
                "the driver killed only its runner process group and aborted the shard"
            )
        raise RuntimeError(
            "scenario exceeded its outer timeout; the runner was interrupted for cleanup "
            f"and the shard was aborted: stderr={stderr!r}, stdout={stdout!r}"
        )
    return subprocess.CompletedProcess(command, process.returncode, stdout, stderr)


def run_entry(entry: dict, args: argparse.Namespace) -> None:
    scenario = (ROOT / entry["path"]).resolve(strict=True)
    command = [
        str(args.tmux_scenario.resolve(strict=True)),
        "--scenario",
        str(scenario),
        "--shim-bin",
        str(args.shim.resolve(strict=True)),
    ]
    for install in entry["command"]["installs"]:
        source = resolve_source(install["source"], args)
        command.extend(("--install", f"{install['name']}={source}"))
    timeout = entry["timeout_ms"] / 1000
    completed = run_command(command, timeout)
    if completed.stdout:
        try:
            report = json.loads(completed.stdout)
        except json.JSONDecodeError as error:
            raise RuntimeError(f"invalid report JSON: {error}: {completed.stdout!r}") from error
        report_path = args.reports / report_name(entry["path"])
        report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
        validate_report(entry, report, completed.returncode)
    else:
        raise RuntimeError(
            f"missing report, exit={completed.returncode}, stderr={completed.stderr!r}"
        )


def validate_expected_projection(entry: dict, expected: dict) -> None:
    try:
        scenario = json.loads((ROOT / entry["path"]).read_text())
        steps = scenario["steps"]
    except (OSError, json.JSONDecodeError, KeyError) as error:
        raise RuntimeError(f"{entry['path']}: cannot load scenario projection: {error}") from error
    if not isinstance(steps, list) or any(not isinstance(step, dict) for step in steps):
        raise RuntimeError(f"{entry['path']}: scenario steps must be objects")
    operations = [step.get("op") for step in steps]
    if (
        len(steps) != expected["steps_total"]
        or any(not isinstance(operation, str) for operation in operations)
        or sorted(set(operations)) != expected["operations"]
    ):
        raise RuntimeError(f"{entry['path']}: expected step projection differs from scenario")
    capture_names = [
        step.get("name") for step in steps if step.get("op") == "capture"
    ]
    if capture_names != expected["capture_names"]:
        raise RuntimeError(f"{entry['path']}: expected captures differ from scenario")
    failure = expected["failed_step"]
    if failure is not None and (
        failure["index"] >= len(steps)
        or operations[failure["index"]] != failure["op"]
    ):
        raise RuntimeError(f"{entry['path']}: expected failed step differs from scenario")


def validate_manifest(manifest: object) -> list[dict]:
    manifest = require_object(manifest, "execution manifest", {"schema", "scenarios"})
    if manifest["schema"] != 1:
        raise RuntimeError("execution manifest schema must be 1")
    entries = manifest["scenarios"]
    if not isinstance(entries, list) or not entries:
        raise RuntimeError("execution manifest scenarios must be a nonempty list")
    entry_keys = {
        "path",
        "scenario_schema",
        "criteria",
        "platforms",
        "ci_job",
        "command",
        "timeout_ms",
        "expect",
    }
    for index, entry in enumerate(entries):
        require_object(entry, f"manifest scenario {index}", entry_keys)
    paths = [entry["path"] for entry in entries]
    if any(not isinstance(path, str) or not path for path in paths):
        raise RuntimeError("every manifest scenario path must be a nonempty string")
    if paths != sorted(paths) or len(paths) != len(set(paths)):
        raise RuntimeError("manifest scenario paths must be sorted and unique")
    for entry in entries:
        path = pathlib.PurePosixPath(entry["path"])
        if path.is_absolute() or ".." in path.parts:
            raise RuntimeError(f"unsafe manifest scenario path {entry['path']!r}")
        if entry["scenario_schema"] != 1:
            raise RuntimeError(f"{entry['path']}: scenario schema must be 1")
        criteria = entry["criteria"]
        if (
            not isinstance(criteria, list)
            or not criteria
            or any(not isinstance(value, str) for value in criteria)
            or criteria != sorted(set(criteria))
        ):
            raise RuntimeError(f"{entry['path']}: criteria must be sorted unique strings")
        if not isinstance(entry["ci_job"], str) or not entry["ci_job"]:
            raise RuntimeError(f"{entry['path']}: CI job must be a nonempty string")
        command = require_object(
            entry["command"], f"{entry['path']}: command", {"binary", "installs"}
        )
        if command["binary"] != "tmux_scenario":
            raise RuntimeError(f"{entry['path']}: command binary must be tmux_scenario")
        installs = command["installs"]
        if not isinstance(installs, list):
            raise RuntimeError(f"{entry['path']}: installs must be a list")
        for index, install in enumerate(installs):
            require_object(
                install, f"{entry['path']}: install {index}", {"name", "source"}
            )
            if not all(
                isinstance(install[field], str) and install[field]
                for field in ("name", "source")
            ):
                raise RuntimeError(f"{entry['path']}: install values must be strings")
            name = install["name"]
            if not INSTALL_NAME.fullmatch(name) or name in (".", ".."):
                raise RuntimeError(f"{entry['path']}: install name is invalid")
            if ":" not in install["source"]:
                raise RuntimeError(f"{entry['path']}: install source is invalid")
        names = [install["name"] for install in installs]
        if names != sorted(names) or len(names) != len(set(names)):
            raise RuntimeError(f"{entry['path']}: install names must be sorted and unique")
        platforms = require_object(
            entry["platforms"],
            f"{entry['path']}: platforms",
            {"linux", "macos", "windows"},
        )
        required_count = 0
        for platform in ("linux", "macos", "windows"):
            platform_entry = platforms[platform]
            if not isinstance(platform_entry, dict) or set(platform_entry) not in (
                {"disposition"},
                {"disposition", "reason"},
            ):
                raise RuntimeError(f"{entry['path']}: invalid {platform} shape")
            disposition = platform_entry["disposition"]
            if disposition not in ("required", "unsupported"):
                raise RuntimeError(f"{entry['path']}: invalid {platform} disposition")
            if disposition == "required":
                required_count += 1
                if "reason" in platform_entry:
                    raise RuntimeError(f"{entry['path']}: required {platform} has a reason")
            elif not isinstance(platform_entry.get("reason"), str) or not platform_entry["reason"]:
                raise RuntimeError(f"{entry['path']}: unsupported {platform} needs a reason")
        windows_owned_issue493 = (
            entry["path"] == ISSUE493_PATH
            and required_count == 0
            and entry["ci_job"] == "windows_native"
            and platforms["linux"].get("reason") == ISSUE493_UNIX_REASON
            and platforms["macos"].get("reason") == ISSUE493_UNIX_REASON
        )
        if required_count != 1 and not windows_owned_issue493:
            raise RuntimeError(f"{entry['path']}: exactly one platform must be required")
        if type(entry["timeout_ms"]) is not int or entry["timeout_ms"] <= 0:
            raise RuntimeError(f"{entry['path']}: timeout must be a positive integer")
        expected = require_object(
            entry["expect"],
            f"{entry['path']}: expectation",
            {
                "exit_code",
                "report_status",
                "steps_total",
                "operations",
                "assertions",
                "captures",
                "capture_names",
                "failed_step",
            },
        )
        if (
            type(expected["exit_code"]) is not int
            or expected["exit_code"] not in (0, 3, 4, 124)
            or expected["report_status"] not in ("passed", "failed")
            or type(expected["steps_total"]) is not int
            or expected["steps_total"] < 1
            or type(expected["captures"]) is not int
            or expected["captures"] < 0
        ):
            raise RuntimeError(f"{entry['path']}: expectation scalar values are invalid")
        operations = expected["operations"]
        if (
            not isinstance(operations, list)
            or any(not isinstance(operation, str) for operation in operations)
            or operations != sorted(set(operations))
        ):
            raise RuntimeError(f"{entry['path']}: expected operations are invalid")
        assertions = expected["assertions"]
        if not isinstance(assertions, dict) or any(
            operation not in ("assert-frame", "assert-capture", "assert-file")
            or type(count) is not int
            or count < 1
            for operation, count in assertions.items()
        ):
            raise RuntimeError(f"{entry['path']}: expected assertions are invalid")
        capture_names = expected["capture_names"]
        if (
            not isinstance(capture_names, list)
            or any(not isinstance(name, str) or not name for name in capture_names)
            or len(capture_names) != expected["captures"]
            or len(capture_names) != len(set(capture_names))
        ):
            raise RuntimeError(f"{entry['path']}: expected capture names are invalid")
        failed_step = expected["failed_step"]
        if failed_step is not None:
            failed_step = require_object(
                failed_step,
                f"{entry['path']}: failed step",
                {"index", "op", "error_prefix"},
            )
            if (
                type(failed_step["index"]) is not int
                or failed_step["index"] < 0
                or not isinstance(failed_step["op"], str)
                or not failed_step["op"]
                or not isinstance(failed_step["error_prefix"], str)
                or not failed_step["error_prefix"].startswith("HAR-E")
            ):
                raise RuntimeError(f"{entry['path']}: failed step is invalid")
        if expected["exit_code"] == 0 and failed_step is not None:
            raise RuntimeError(f"{entry['path']}: passing expectation cannot declare a failure")
        if expected["exit_code"] != 0 and failed_step is None:
            raise RuntimeError(f"{entry['path']}: report failure expectation is incomplete")
        validate_expected_projection(entry, expected)
    return entries


def required_entries(entries: list[dict], platform: str) -> list[dict]:
    return [
        entry
        for entry in entries
        if entry["platforms"][platform]["disposition"] == "required"
    ]


def select_entries(entries: list[dict], args: argparse.Namespace) -> list[dict]:
    required = required_entries(entries, args.platform)
    if not required:
        raise RuntimeError(f"invalid {args.platform} manifest subset count")
    if args.shard_count < 1 or not 0 <= args.shard_index < args.shard_count:
        raise RuntimeError("shard index must be within the positive shard count")
    if args.scenario:
        if args.shard_count != 1 or args.shard_index != 0:
            raise RuntimeError("explicit scenario selection cannot be combined with sharding")
        if len(args.scenario) != len(set(args.scenario)):
            raise RuntimeError("selected scenario paths must be unique")
        requested = set(args.scenario)
        known = {entry["path"] for entry in entries}
        unknown = sorted(requested - known)
        if unknown:
            raise RuntimeError(f"scenario is absent from manifest: {unknown[0]}")
        selected = [entry for entry in entries if entry["path"] in requested]
        unsupported = [
            entry["path"]
            for entry in selected
            if entry["platforms"][args.platform]["disposition"] != "required"
        ]
        if unsupported:
            raise RuntimeError(
                f"scenario is not required on {args.platform}: {unsupported[0]}"
            )
        return selected
    selected = required[args.shard_index :: args.shard_count]
    if not selected:
        raise RuntimeError(
            f"shard {args.shard_index}/{args.shard_count} has no required scenarios"
        )
    return selected


def manifest_sha256() -> str:
    return hashlib.sha256(MANIFEST.read_bytes()).hexdigest()


def require_execution_args(args: argparse.Namespace) -> None:
    required = {
        "--tmux-scenario": args.tmux_scenario,
        "--jefe": args.jefe,
        "--probe": args.probe,
        "--jsp-fixture": args.jsp_fixture,
        "--shim": args.shim,
        "--reports": args.reports,
    }
    missing = [flag for flag, value in required.items() if value is None]
    if missing:
        raise RuntimeError(f"missing execution arguments: {', '.join(missing)}")
    if args.expected_shards is not None:
        raise RuntimeError("--expected-shards is valid only with --verify-completion")


def write_completion(
    entries: list[dict], selected: list[dict], args: argparse.Namespace
) -> None:
    completion = {
        "schema": 1,
        "manifest_sha256": manifest_sha256(),
        "platform": args.platform,
        "selection": "explicit" if args.scenario else "required-shard",
        "shard_index": args.shard_index,
        "shard_count": args.shard_count,
        "required_count": len(required_entries(entries, args.platform)),
        "executed_count": len(selected),
        "scenarios": [entry["path"] for entry in selected],
    }
    (args.reports / "_completion.json").write_text(
        json.dumps(completion, indent=2, sort_keys=True) + "\n"
    )


def verify_completion(entries: list[dict], args: argparse.Namespace) -> None:
    if args.expected_shards is None or args.expected_shards < 1:
        raise RuntimeError("verification requires a positive --expected-shards")
    if any(
        value is not None
        for value in (
            args.tmux_scenario,
            args.jefe,
            args.probe,
            args.jsp_fixture,
            args.shim,
            args.reports,
        )
    ) or args.scenario:
        raise RuntimeError("completion verification does not accept execution arguments")
    root = args.verify_completion.resolve(strict=True)
    completion_paths = sorted(root.rglob("_completion.json"))
    if len(completion_paths) != args.expected_shards:
        raise RuntimeError(
            f"completion records={len(completion_paths)}, expected={args.expected_shards}"
        )
    required = required_entries(entries, args.platform)
    by_path = {entry["path"]: entry for entry in required}
    seen_scenarios = []
    seen_reports = set()
    seen_indices = set()
    for completion_path in completion_paths:
        completion = json.loads(completion_path.read_text())
        expected_keys = {
            "schema",
            "manifest_sha256",
            "platform",
            "selection",
            "shard_index",
            "shard_count",
            "required_count",
            "executed_count",
            "scenarios",
        }
        if set(completion) != expected_keys:
            raise RuntimeError(f"{completion_path}: completion shape differs")
        index = completion["shard_index"]
        if (
            completion["schema"] != 1
            or completion["manifest_sha256"] != manifest_sha256()
            or completion["platform"] != args.platform
            or completion["selection"] != "required-shard"
            or completion["shard_count"] != args.expected_shards
            or completion["required_count"] != len(required)
            or index not in range(args.expected_shards)
            or index in seen_indices
        ):
            raise RuntimeError(f"{completion_path}: completion identity differs")
        expected_paths = [
            entry["path"] for entry in required[index :: args.expected_shards]
        ]
        if (
            completion["scenarios"] != expected_paths
            or completion["executed_count"] != len(expected_paths)
        ):
            raise RuntimeError(f"{completion_path}: shard scenario inventory differs")
        seen_indices.add(index)
        seen_scenarios.extend(expected_paths)
        for scenario_path in expected_paths:
            entry = by_path[scenario_path]
            evidence_path = completion_path.parent / report_name(scenario_path)
            report = json.loads(evidence_path.read_text())
            validate_report(entry, report, entry["expect"]["exit_code"])
            seen_reports.add(evidence_path.resolve())
    if seen_indices != set(range(args.expected_shards)):
        raise RuntimeError("completion shard indices are incomplete")
    required_paths = [entry["path"] for entry in required]
    if len(seen_scenarios) != len(set(seen_scenarios)) or sorted(seen_scenarios) != required_paths:
        raise RuntimeError("completion scenario union differs from required subset")
    actual_reports = {
        path.resolve()
        for path in root.rglob("*.json")
        if path.name != "_completion.json"
    }
    if actual_reports != seen_reports:
        raise RuntimeError("completion report file inventory differs")
    print(
        f"verified exactly {len(required)} required {args.platform} scenarios "
        f"across {args.expected_shards} shards"
    )


def main() -> int:
    try:
        args = parse_args()
        manifest = json.loads(MANIFEST.read_text())
        entries = validate_manifest(manifest)
        if args.verify_completion is not None:
            verify_completion(entries, args)
            return 0
        require_execution_args(args)
        selected = select_entries(entries, args)
        if args.reports.exists() and any(args.reports.iterdir()):
            raise RuntimeError(f"reports directory is not empty: {args.reports}")
        args.reports.mkdir(parents=True, exist_ok=True)
        failures = []
        for index, entry in enumerate(selected, start=1):
            print(f"[{index}/{len(selected)}] {entry['path']}", flush=True)
            try:
                run_entry(entry, args)
            except (RuntimeError, subprocess.TimeoutExpired) as error:
                failures.append(f"{entry['path']}: {error}")
        if failures:
            print("\n".join(failures), file=sys.stderr)
            return 1
        write_completion(entries, selected, args)
        print(
            f"executed exactly {len(selected)} required {args.platform} scenarios "
            f"in shard {args.shard_index}/{args.shard_count}"
        )
        return 0
    except (
        KeyError,
        OSError,
        RuntimeError,
        TypeError,
        ValueError,
        json.JSONDecodeError,
    ) as error:
        print(str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
