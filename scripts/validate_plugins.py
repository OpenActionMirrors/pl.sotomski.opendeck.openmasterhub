#!/usr/bin/env python3
"""Validate plugin.json manifests.

A missing plugins directory is success: the tree does not ship installed plugins yet.
Pass another directory, such as ``examples``, to check manifests there.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

PERMISSIONS = {"fs:scoped", "network:read", "daemon:connect"}
RUNTIMES = {"hook", "subprocess"}
ID_RE = re.compile(r"^[a-z0-9-]+\.[a-z0-9-]+$")
VERSION_RE = re.compile(
    r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)"
    r"(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$"
)
REQ_VERSION_RE = re.compile(
    r"^(0|[1-9]\d*)(?:\.(0|[1-9]\d*)){0,2}"
    r"(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$"
)
REQUIRED = (
    "id",
    "name",
    "version",
    "description",
    "entrypoint",
    "author",
    "min_core_version",
    "dependencies",
    "permissions",
    "hooks",
)


def main() -> int:
    root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("plugins")
    if not root.exists():
        print(f"No {root} directory; nothing to validate.")
        return 0

    manifests = sorted(path for path in root.glob("*/plugin.json") if path.is_file())
    if not manifests:
        print("No plugin manifests found.")
        return 0

    failed = False
    for path in manifests:
        problems = validate_file(path)
        if problems:
            failed = True
            for problem in problems:
                print(f"{path}: {problem}")
            continue
        print(f"{path}: ok")
    return 1 if failed else 0


def validate_file(path: Path) -> list[str]:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        return [f"invalid JSON ({exc})"]
    if not isinstance(data, dict):
        return ["manifest must be a JSON object"]
    problems: list[str] = []
    missing = [key for key in REQUIRED if key not in data]
    if missing:
        problems.append("missing " + ", ".join(missing))
        return problems
    problems.extend(check_text(data, "id"))
    if isinstance(data.get("id"), str) and not ID_RE.match(data["id"]):
        problems.append("id must match author.plugin-name")
    problems.extend(check_text(data, "name"))
    problems.extend(check_text(data, "description"))
    if not isinstance(data.get("version"), str) or not VERSION_RE.match(data["version"]):
        problems.append("version must be semver")
    problems.extend(check_entrypoint(data.get("entrypoint")))
    problems.extend(check_author(data.get("author")))
    problems.extend(check_requirement(data.get("min_core_version")))
    problems.extend(check_dependencies(data.get("dependencies"), data.get("id")))
    problems.extend(check_permissions(data.get("permissions")))
    problems.extend(check_hooks(data.get("hooks")))
    problems.extend(check_actions(data.get("actions")))
    problems.extend(check_context(data.get("context"), data.get("actions")))
    problems.extend(check_activation(data.get("activation")))
    if "defaults" in data:
        problems.append("defaults was removed; use context.defaults")
    if "runtime" in data and data["runtime"] not in RUNTIMES:
        problems.append("runtime must be hook or subprocess")
    return problems


def check_text(data: dict, field: str) -> list[str]:
    value = data.get(field)
    if not isinstance(value, str) or not value.strip():
        return [f"{field} must be a non-empty string"]
    return []


def check_entrypoint(value: object) -> list[str]:
    if not isinstance(value, str) or not value.strip():
        return ["entrypoint must be a non-empty string"]
    path = Path(value)
    if path.is_absolute() or ".." in path.parts:
        return ["entrypoint must be a relative path without .."]
    return []


def check_author(value: object) -> list[str]:
    if isinstance(value, str):
        if not value.strip():
            return ["author must be a non-empty string"]
        return []
    if isinstance(value, dict):
        name = value.get("name")
        if not isinstance(name, str) or not name.strip():
            return ["author.name must be a non-empty string"]
        for optional in ("email", "url"):
            if optional in value and (
                not isinstance(value[optional], str) or not value[optional].strip()
            ):
                return [f"author.{optional} must be a non-empty string"]
        return []
    return ["author must be a string or an object"]


def check_requirement(value: object) -> list[str]:
    if not isinstance(value, str) or not valid_requirement(value.strip()):
        return ["min_core_version must be a semver requirement"]
    return []


def valid_requirement(value: str) -> bool:
    if value == "*":
        return True
    if not value:
        return False
    for part in value.split(","):
        part = part.strip()
        for operator in (">=", "<=", ">", "<", "=", "^", "~"):
            if part.startswith(operator):
                part = part[len(operator) :].strip()
                break
        if not REQ_VERSION_RE.match(part):
            return False
    return True


def check_dependencies(value: object, plugin_id: object) -> list[str]:
    if not isinstance(value, list):
        return ["dependencies must be an array"]
    seen: set[str] = set()
    for dep in value:
        if not isinstance(dep, str) or not ID_RE.match(dep):
            return ["dependencies must be plugin ids"]
        if dep == plugin_id:
            return ["a plugin cannot depend on itself"]
        if dep in seen:
            return [f"duplicate dependency {dep}"]
        seen.add(dep)
    return []


def check_permissions(value: object) -> list[str]:
    if not isinstance(value, list):
        return ["permissions must be an array"]
    seen: set[str] = set()
    for permission in value:
        if permission not in PERMISSIONS:
            return [f"unknown permission {permission}"]
        if permission in seen:
            return [f"duplicate permission {permission}"]
        seen.add(permission)
    return []


def check_hooks(value: object) -> list[str]:
    if not isinstance(value, dict):
        return ["hooks must be an object"]
    for event, hook in value.items():
        if not isinstance(event, str) or not event.strip():
            return ["hook event names must be non-empty strings"]
        if not isinstance(hook, dict):
            return [f"hook {event} must be an object"]
        handler = hook.get("handler")
        if not isinstance(handler, str) or not handler.strip():
            return [f"hook {event} handler must be a non-empty string"]
        if not isinstance(hook.get("priority"), int) or isinstance(hook.get("priority"), bool):
            return [f"hook {event} priority must be an integer"]
    return []


OUTPUT_COMMANDS = {
    "set_button_image",
    "set_screen_image",
    "set_button_color",
    "set_screen_color",
    "set_backlight",
    "set_led_mode",
    "set_led_color",
    "set_led_brightness",
}
CONTROLS = {"button", "knob", "slider", "big_knob", "roller"}
ACTION_ID_RE = re.compile(r"^[a-z0-9_]+$")


def check_actions(value: object) -> list[str]:
    if value is None:
        return []
    if not isinstance(value, dict):
        return ["actions must be an object"]
    switches = 0
    for action_id, action in value.items():
        if not ACTION_ID_RE.match(action_id):
            return [f"action id is invalid: {action_id}"]
        if not isinstance(action, dict):
            return [f"action {action_id} must be an object"]
        label = action.get("label")
        if not isinstance(label, str) or not label.strip():
            return [f"action {action_id} label must be a non-empty string"]
        controls = action.get("controls")
        if not isinstance(controls, list) or not controls:
            return [f"action {action_id} must list at least one control"]
        if any(kind not in CONTROLS for kind in controls):
            return [f"action {action_id} has an unknown control"]
        if len(controls) != len(set(controls)):
            return [f"action {action_id} lists a control more than once"]
        role = action.get("role", "normal")
        if role not in {"normal", "switch_context"}:
            return [f"action {action_id} role must be normal or switch_context"]
        if role == "switch_context":
            switches += 1
            if controls != ["button"]:
                return [f"action {action_id} is switch_context and can only use a button"]
        problems = check_params(action_id, action.get("params"))
        if problems:
            return problems
    if switches > 1:
        return ["a plugin can declare only one switch_context action"]
    return []


def check_params(action_id: str, value: object) -> list[str]:
    if value is None:
        return []
    if not isinstance(value, list):
        return [f"action {action_id} params must be an array"]
    seen: list[str] = []
    for param in value:
        if not isinstance(param, dict):
            return [f"action {action_id} parameters must be objects"]
        param_id = param.get("id")
        label = param.get("label")
        kind = param.get("kind")
        if not isinstance(param_id, str) or not ACTION_ID_RE.match(param_id):
            return [f"action {action_id} parameter id is invalid"]
        if param_id in seen:
            return [f"action {action_id} lists parameter {param_id} more than once"]
        seen.append(param_id)
        if not isinstance(label, str) or not label.strip():
            return [f"action {action_id} parameter {param_id} label must be a non-empty string"]
        if kind == "text":
            unknown = set(param) - {"id", "label", "kind", "default"}
            if unknown:
                return [f"action {action_id} parameter {param_id} has an unknown field"]
            default = param.get("default", "")
            if not isinstance(default, str):
                return [f"action {action_id} parameter {param_id} default must be a string"]
            continue
        if kind != "choice":
            return [f"action {action_id} parameter {param_id} kind must be text or choice"]
        unknown = set(param) - {"id", "label", "kind", "default", "options"}
        if unknown:
            return [f"action {action_id} parameter {param_id} has an unknown field"]
        options = param.get("options")
        if not isinstance(options, list) or not options:
            return [f"action {action_id} parameter {param_id} needs a choice"]
        option_ids: list[str] = []
        for option in options:
            if not isinstance(option, dict):
                return [f"action {action_id} parameter {param_id} choices must be objects"]
            if set(option) - {"id", "label"}:
                return [f"action {action_id} parameter {param_id} choice has an unknown field"]
            option_id = option.get("id")
            option_label = option.get("label")
            if not isinstance(option_id, str) or not ACTION_ID_RE.match(option_id):
                return [f"action {action_id} parameter {param_id} has an invalid choice"]
            if not isinstance(option_label, str) or not option_label.strip():
                return [f"action {action_id} parameter {param_id} choice label must be a non-empty string"]
            if option_id in option_ids:
                return [f"action {action_id} parameter {param_id} lists {option_id} more than once"]
            option_ids.append(option_id)
        default = param.get("default", "")
        if not isinstance(default, str):
            return [f"action {action_id} parameter {param_id} default must be a string"]
        if default and default not in option_ids:
            return [f"action {action_id} parameter {param_id} default is not a choice"]
    return []


def check_context(value: object, actions: object) -> list[str]:
    if value is None:
        return []
    if not isinstance(value, dict):
        return ["context must be an object"]
    known = actions if isinstance(actions, dict) else {}
    if not any(
        isinstance(action, dict) and action.get("role") == "switch_context"
        for action in known.values()
    ):
        return ["context requires a switch_context action"]
    defaults = value.get("defaults", [])
    if not isinstance(defaults, list):
        return ["context.defaults must be an array"]
    seen: list[str] = []
    for item in defaults:
        if not isinstance(item, dict):
            return ["context defaults must be objects"]
        action_id = item.get("action")
        target = item.get("target")
        if not isinstance(action_id, str) or not isinstance(target, dict):
            return ["a context default needs an action and a target"]
        kind = target_kind(target)
        if kind is None:
            return ["a context default target is invalid"]
        if kind == "button" and target.get("index") == 0:
            return ["button 0 is reserved for returning to the main context"]
        key = json.dumps(target, sort_keys=True)
        if key in seen:
            return ["context defaults use the same control more than once"]
        seen.append(key)
        action = known.get(action_id)
        if not isinstance(action, dict):
            return [f"context default refers to unknown action {action_id}"]
        if action.get("role") == "switch_context":
            return ["switch_context cannot be a default inside a plugin context"]
        controls = action.get("controls")
        if not isinstance(controls, list) or kind not in controls:
            return [f"context default {action_id} does not support {kind}"]
    return []


def target_kind(target: dict) -> str | None:
    kind = target.get("kind")
    if kind == "big_knob" or kind == "roller":
        return kind if isinstance(kind, str) else None
    index = target.get("index")
    if isinstance(index, bool) or not isinstance(index, int) or index < 0:
        return None
    page = target.get("page")
    if (
        kind == "button"
        and index <= 14
        and not isinstance(page, bool)
        and isinstance(page, int)
        and page >= 0
    ):
        return "button"
    if kind == "knob" and index <= 2:
        return "knob"
    if kind == "slider" and index <= 4:
        return "slider"
    return None


def check_activation(value: object) -> list[str]:
    if value is None:
        return []
    if not isinstance(value, list):
        return ["activation must be an array"]
    for command in value:
        if not isinstance(command, dict):
            return ["activation entries must be objects"]
        action = command.get("action")
        if action not in OUTPUT_COMMANDS:
            return ["activation action must be an output command"]
        if not isinstance(command.get("payload"), dict):
            return ["activation payload must be an object"]
    return []


if __name__ == "__main__":
    raise SystemExit(main())
