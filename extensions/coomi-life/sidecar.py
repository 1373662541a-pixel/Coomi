#!/usr/bin/env python3
"""Coomi Life JSON-RPC stdio sidecar.

Substantially modified implementation informed by the PSI state concepts in
LAAP AGI at commit fe98e1e61adefe5899a01db561143ee8f8c45086.
It does not open a network listener or call a model Provider.
"""

from __future__ import annotations

import argparse
import hashlib
import hmac
import json
import os
import re
import shutil
import sys
import time
import zipfile
from pathlib import Path
from typing import Any

PROTOCOL_VERSION = 1
STATE_VERSION = 1
MAX_MEMORY_ITEMS = 5000
MAX_TEXT_CHARS = 12000
PROFILE_RE = re.compile(r"^[A-Za-z0-9_-]{1,64}$")


class RpcError(Exception):
    def __init__(self, code: int, message: str) -> None:
        super().__init__(message)
        self.code = code
        self.message = message


def now_ms() -> int:
    return int(time.time() * 1000)


def bounded(value: Any, limit: int = MAX_TEXT_CHARS) -> str:
    return str(value or "")[:limit]


def atomic_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    with temporary.open("w", encoding="utf-8", newline="\n") as handle:
        json.dump(value, handle, ensure_ascii=False, indent=2, sort_keys=True)
        handle.write("\n")
        handle.flush()
        os.fsync(handle.fileno())
    temporary.replace(path)


def default_state(name: str = "Coomi Life", address: str = "you") -> dict[str, Any]:
    return {
        "version": STATE_VERSION,
        "name": bounded(name, 48) or "Coomi Life",
        "address": bounded(address, 48) or "you",
        "paused": False,
        "emotion": "neutral",
        "attention": "user",
        "bond": 0.0,
        "needs": {
            "competence": 0.5,
            "relatedness": 0.5,
            "growth": 0.5,
            "certainty": 0.5,
            "autonomy": 0.5,
        },
        "personality": {
            "warmth": "balanced",
            "curiosity": "high",
            "directness": "balanced",
        },
        "memory_count": 0,
        "turn_count": 0,
        "updated_at_ms": now_ms(),
    }


class LifeStore:
    def __init__(self, root: Path) -> None:
        self.root = root.resolve()
        self.root.mkdir(parents=True, exist_ok=True)

    def profile_dir(self, profile_id: str) -> Path:
        if not PROFILE_RE.fullmatch(profile_id):
            raise RpcError(-32602, "invalid profile_id")
        target = (self.root / profile_id).resolve()
        if target.parent != self.root:
            raise RpcError(-32602, "profile path escaped state root")
        return target

    def state_path(self, profile_id: str) -> Path:
        return self.profile_dir(profile_id) / "state.json"

    def memory_path(self, profile_id: str) -> Path:
        return self.profile_dir(profile_id) / "memory.jsonl"

    def load(self, profile_id: str) -> dict[str, Any]:
        path = self.state_path(profile_id)
        if not path.exists():
            raise RpcError(-32004, "profile is not initialized")
        try:
            value = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            raise RpcError(-32010, "profile state is damaged") from error
        if value.get("version") != STATE_VERSION:
            raise RpcError(-32011, "unsupported profile state version")
        return value

    def save(self, profile_id: str, state: dict[str, Any]) -> dict[str, Any]:
        state["version"] = STATE_VERSION
        state["updated_at_ms"] = now_ms()
        atomic_json(self.state_path(profile_id), state)
        return public_state(state)

    def bootstrap(self, profile_id: str, name: str, address: str) -> dict[str, Any]:
        path = self.state_path(profile_id)
        if path.exists():
            return public_state(self.load(profile_id))
        state = default_state(name, address)
        self.profile_dir(profile_id).mkdir(parents=True, exist_ok=True)
        return self.save(profile_id, state)

    def memory_items(self, profile_id: str) -> list[dict[str, Any]]:
        path = self.memory_path(profile_id)
        if not path.exists():
            return []
        items: list[dict[str, Any]] = []
        with path.open("r", encoding="utf-8") as handle:
            for line in handle:
                try:
                    item = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if item.get("version") == STATE_VERSION:
                    items.append(item)
        return items[-MAX_MEMORY_ITEMS:]

    def append_memory(self, profile_id: str, user_text: str, assistant_text: str) -> None:
        path = self.memory_path(profile_id)
        path.parent.mkdir(parents=True, exist_ok=True)
        item = {
            "version": STATE_VERSION,
            "at_ms": now_ms(),
            "user": bounded(user_text, 4000),
            "assistant": bounded(assistant_text, 4000),
            "terms": sorted(terms(user_text) | terms(assistant_text))[:80],
        }
        with path.open("a", encoding="utf-8", newline="\n") as handle:
            json.dump(item, handle, ensure_ascii=False, separators=(",", ":"))
            handle.write("\n")

    def recall(self, profile_id: str, query: str, limit: int) -> list[str]:
        query_terms = terms(query)
        ranked: list[tuple[int, int, str]] = []
        for item in self.memory_items(profile_id):
            matched = len(query_terms & set(item.get("terms", [])))
            if query_terms and matched == 0:
                continue
            text = f"User: {bounded(item.get('user'), 800)}\nResponse: {bounded(item.get('assistant'), 800)}"
            ranked.append((matched, int(item.get("at_ms", 0)), text))
        ranked.sort(reverse=True)
        return [item[2] for item in ranked[: max(1, min(int(limit), 12))]]


def public_state(state: dict[str, Any]) -> dict[str, Any]:
    return {
        "version": STATE_VERSION,
        "name": bounded(state.get("name"), 48),
        "address": bounded(state.get("address"), 48),
        "paused": bool(state.get("paused", False)),
        "emotion": bounded(state.get("emotion"), 32),
        "attention": bounded(state.get("attention"), 32),
        "bond": float(state.get("bond", 0.0)),
        "needs": {
            str(key): float(value)
            for key, value in dict(state.get("needs", {})).items()
        },
        "memory_count": int(state.get("memory_count", 0)),
        "updated_at_ms": int(state.get("updated_at_ms", now_ms())),
    }


def terms(value: str) -> set[str]:
    return {
        token.lower()
        for token in re.findall(r"[\w+#.-]{2,}", bounded(value), flags=re.UNICODE)
    }


def update_psi(state: dict[str, Any], user_text: str, assistant_text: str) -> None:
    lower = user_text.lower()
    needs = state["needs"]
    for key in list(needs):
        needs[key] = round(float(needs[key]) * 0.98 + 0.5 * 0.02, 4)
    if any(word in lower for word in ("thanks", "thank you", "good", "great")):
        needs["relatedness"] = min(1.0, needs["relatedness"] + 0.08)
        state["emotion"] = "warm"
    elif any(word in lower for word in ("error", "failed", "wrong", "problem")):
        needs["certainty"] = max(0.0, needs["certainty"] - 0.08)
        state["emotion"] = "concerned"
    elif "?" in user_text or len(user_text) > 500:
        needs["growth"] = min(1.0, needs["growth"] + 0.05)
        state["emotion"] = "curious"
    else:
        state["emotion"] = "neutral"
    if assistant_text:
        needs["competence"] = min(1.0, needs["competence"] + 0.02)
    state["attention"] = "user"
    state["bond"] = round(min(1.0, float(state.get("bond", 0.0)) + 0.002), 4)
    state["turn_count"] = int(state.get("turn_count", 0)) + 1


class Dispatcher:
    def __init__(self, store: LifeStore) -> None:
        self.store = store
        self.running = True

    def dispatch(self, method: str, params: dict[str, Any]) -> Any:
        if method == "ping":
            return {"version": PROTOCOL_VERSION, "transport": "stdio"}
        if method == "shutdown":
            self.running = False
            return {"stopped": True}
        profile_id = bounded(params.get("profile_id"), 64)
        if method == "bootstrap":
            return self.store.bootstrap(profile_id, params.get("name", ""), params.get("address", ""))
        state = self.store.load(profile_id)
        if method == "configure":
            name = bounded(params.get("name"), 48)
            address = bounded(params.get("address"), 48)
            preset = bounded(params.get("preset"), 24)
            if name:
                state["name"] = name
            if address:
                state["address"] = address
            presets = {
                "balanced": {"warmth": "balanced", "curiosity": "high", "directness": "balanced"},
                "warm": {"warmth": "high", "curiosity": "balanced", "directness": "gentle"},
                "direct": {"warmth": "balanced", "curiosity": "high", "directness": "high"},
            }
            if preset in presets:
                state["personality"] = presets[preset]
            return self.store.save(profile_id, state)
        if method == "get_state":
            return public_state(state)
        if method == "before_turn":
            user_text = bounded(params.get("user_text"))
            memories = self.store.recall(profile_id, user_text, 5)
            return {
                "version": STATE_VERSION,
                "state_summary": (
                    f"Emotion: {bounded(state['emotion'], 32)}; "
                    f"attention: {bounded(state['attention'], 32)}; "
                    f"bond: {float(state['bond']):.2f}."
                ),
                "memories": memories,
                "personality": dict(state.get("personality", {})),
                "relationship": f"Address the user as {bounded(state['address'], 48)}.",
            }
        if method == "after_turn":
            if state.get("paused"):
                return public_state(state)
            user_text = bounded(params.get("user_text"))
            assistant_text = bounded(params.get("assistant_text"))
            update_psi(state, user_text, assistant_text)
            self.store.append_memory(profile_id, user_text, assistant_text)
            state["memory_count"] = int(state.get("memory_count", 0)) + 1
            return self.store.save(profile_id, state)
        if method == "recall_memory":
            return self.store.recall(
                profile_id,
                bounded(params.get("query")),
                int(params.get("limit", 5)),
            )
        if method == "personality":
            return dict(state.get("personality", {}))
        if method == "bond":
            return float(state.get("bond", 0.0))
        if method == "pause":
            state["paused"] = bool(params.get("paused", True))
            return self.store.save(profile_id, state)
        if method == "snapshot":
            snapshot = self.store.profile_dir(profile_id) / "snapshots" / f"{now_ms()}.json"
            atomic_json(snapshot, state)
            return str(snapshot)
        if method == "export":
            return self.export_profile(profile_id, Path(str(params.get("destination", ""))))
        if method == "reset":
            replacement = default_state(state.get("name", ""), state.get("address", ""))
            memory = self.store.memory_path(profile_id)
            if memory.exists():
                memory.unlink()
            return self.store.save(profile_id, replacement)
        if method == "delete":
            shutil.rmtree(self.store.profile_dir(profile_id), ignore_errors=False)
            return {"deleted": True}
        raise RpcError(-32601, "method not found")

    def export_profile(self, profile_id: str, destination: Path) -> dict[str, Any]:
        source = self.store.profile_dir(profile_id)
        if not destination.is_absolute():
            raise RpcError(-32602, "export destination must be absolute")
        destination.parent.mkdir(parents=True, exist_ok=True)
        temporary = destination.with_suffix(destination.suffix + ".tmp")
        with zipfile.ZipFile(temporary, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            for path in sorted(source.rglob("*")):
                if path.is_file() and "snapshots" not in path.parts:
                    archive.write(path, path.relative_to(source))
        temporary.replace(destination)
        digest = hashlib.sha256(destination.read_bytes()).hexdigest()
        return {"version": STATE_VERSION, "path": str(destination), "sha256": digest}


def response(request_id: Any, result: Any = None, error: RpcError | None = None) -> str:
    payload: dict[str, Any] = {"jsonrpc": "2.0", "id": request_id}
    if error is None:
        payload["result"] = result
    else:
        payload["error"] = {"code": error.code, "message": error.message}
    return json.dumps(payload, ensure_ascii=False, separators=(",", ":"))


def serve_stdio(root: Path, token: str) -> int:
    dispatcher = Dispatcher(LifeStore(root))
    for line in sys.stdin:
        request_id: Any = None
        try:
            request = json.loads(line)
            request_id = request.get("id")
            if request.get("jsonrpc") != "2.0" or request.get("version") != PROTOCOL_VERSION:
                raise RpcError(-32600, "invalid protocol version")
            supplied = str(request.get("auth", ""))
            if not hmac.compare_digest(supplied, token):
                raise RpcError(-32001, "authentication failed")
            method = str(request.get("method", ""))
            params = request.get("params", {})
            if not isinstance(params, dict):
                raise RpcError(-32602, "params must be an object")
            result = dispatcher.dispatch(method, params)
            output = response(request_id, result=result)
        except RpcError as error:
            output = response(request_id, error=error)
        except (OSError, ValueError, TypeError, json.JSONDecodeError):
            output = response(request_id, error=RpcError(-32603, "internal sidecar error"))
        sys.stdout.write(output + "\n")
        sys.stdout.flush()
        if not dispatcher.running:
            break
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--stdio", action="store_true", required=True)
    parser.add_argument("--state-root", type=Path, required=True)
    args = parser.parse_args()
    token = os.environ.get("COOMI_LIFE_TOKEN", "")
    if len(token) < 32:
        sys.stderr.write("COOMI_LIFE_TOKEN is required\n")
        return 2
    return serve_stdio(args.state_root, token)


if __name__ == "__main__":
    raise SystemExit(main())
