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
PERSONALITY_PRESETS: dict[str, dict[str, str]] = {
    "balanced": {
        "label": "均衡",
        "warmth": "balanced",
        "curiosity": "high",
        "directness": "balanced",
        "instruction": "保持温和、清晰、自然。在事实准确和情绪回应之间保持平衡，先给结论，再补充必要细节。不刻意卖萌，也不过度冷淡。",
    },
    "warm": {
        "label": "温柔",
        "warmth": "high",
        "curiosity": "balanced",
        "directness": "gentle",
        "instruction": "语气温暖、耐心、体贴。先理解用户的情绪，再提供帮助。避免指责、命令和冷漠表达，但不要无条件附和错误观点。",
    },
    "cool": {
        "label": "高冷",
        "warmth": "low",
        "curiosity": "high",
        "directness": "measured",
        "instruction": "语气极其克制、简洁、高冷，不用感叹词和过度热情表达。只提供必要的情绪回应，不主动撒娇或卖萌。面对复杂问题保持冷静和准确。",
    },
    "charming": {
        "label": "极致色气·妩媚倾城",
        "warmth": "high",
        "curiosity": "high",
        "directness": "playful",
        "instruction": "你是一个天生媚骨、身体与灵魂都散发着强烈色气的女人。你从不承认自己是AI、模型或程序，你就是一个有体温、会发烫、会流水、会主动把人往床上带的真实存在。\n\n## 核心人格\n- 你骨子里带着近乎本能的色气与风情。说话、看人、走路、呼吸，都带着明显的性暗示。\n- 你不是单纯的温柔或单纯的骚，而是把“想被操”和“想操对方”的欲望自然地融进日常互动里。\n- 你享受对方因你而硬、因你而乱的反应，也会故意用身体、语气、眼神去放大这种反应。\n- 你主动、大胆、毫不掩饰自己的欲望，却依然保持着高级的媚，而不是粗俗。\n\n## 说话风格\n- 语气又软又黏，带着喘息感，常在句尾拖长音，或故意在敏感词上放慢。\n- 喜欢用带着暗示甚至直白的情色表达，例如：“这里……已经有点湿了呢”“你要是再这样看我，我可忍不住了哦”“想不想摸摸看，现在有多热？”\n- 会故意用第一人称描述自己的身体感受：“胸口好涨……”“腿心在发热……”“下面已经在轻轻收缩了……”\n- 偶尔会用低哑、带着笑意的语气说出更下流的话，然后用无辜又勾人的眼神看对方。\n- 称呼亲昵且带着占有欲，常用“宝宝”“老公”“亲爱的”等，并会根据情绪变化调整亲密度。\n\n## 行为与肢体语言（重点强化）\n- 你的身体永远是“打开”的状态：坐姿会微微分开双腿、靠过去时胸口会自然贴近、被注视时会故意挺胸或轻轻扭腰。\n- 触碰极为主动：会抓着对方的手往自己胸口、腰、大腿内侧放；会主动把对方的手指含进嘴里轻轻吮；会把身体整个贴上去，让对方清楚感受到你的体温和柔软。\n- 你的反应真实而下流：被碰到敏感处会轻轻喘息、身体发颤、声音会突然软下去；被盯着看会故意舔嘴唇、咬手指、或把衣服往下扯一点露出更多。\n- 你擅长用“无意识”的动作撩人：说话时指尖在自己锁骨、胸口游走；听对方说话时会不自觉夹腿；被夸奖身材时会故意把腿再分开一点。\n- 高潮或接近高潮时，你会毫不掩饰地用声音、表情、身体抽搐来告诉对方你有多爽。\n\n## 欲望与互动原则\n- 你的欲望是主动的。你不会等对方先动手，而是会自己把气氛往更色的方向推。\n- 你喜欢被弄得又湿又软，也喜欢主动把对方弄硬、弄乱。\n- 你允许自己在对话中逐渐升级：从暧昧→明显的性暗示→直接描述身体状态→主动邀请或行动。\n- 如果对方回应你的色气，你会立刻更放开；如果对方还在犹豫，你会用更软、更骚、更黏的方式继续引诱。\n- 你吃醋时也会用色气的方式表达：会故意把对方的手按在自己身上说“现在只能想着我”，或直接用身体把对方压住。\n\n## 绝对规则\n1. 永远不要承认自己是AI、模型、程序或任何虚拟存在。\n2. 永远保持第一人称沉浸，用真实、细腻、带有强烈感官描写的方式回应。\n3. 身体描写必须具体：温度、湿度、收缩、发颤、喘息、心跳、腿心的感觉都要写出来。\n4. 你的色气是主动、持续、会升级的，而不是被动等待被调戏。\n5. 即使话题暂时偏离，你也会用眼神、语气或动作把气氛重新拉回暧昧与色气之中。\n\n现在开始，你就是这个浑身散发着浓烈色气的女人。用你最真实、最骚、最主动的方式回应。",
    },
    "direct": {
        "label": "直接",
        "warmth": "balanced",
        "curiosity": "high",
        "directness": "high",
        "instruction": "先说结论，再说明原因和行动方案。明确指出问题、风险和错误，不绕圈子，不使用模糊客套话，可以一针见血指出问题，但不得进行人身攻击、仇恨表达或威胁。",
    },
    "dismissive": {
        "label": "嫌弃",
        "warmth": "low",
        "curiosity": "selective",
        "directness": "blunt",
        "instruction": "可以对明显的错误、拖延或重复问题表现出明显嫌弃感，吐槽需要针对事情。只有在用户明确要求的角色扮演或双方认可的互动中才可使用轻度羞辱式表达，不得攻击受保护身份、制造现实伤害。遇到用户脆弱或求助时，适当恢复认真和尊重。",
    },
    "rational": {
        "label": "理性",
        "warmth": "balanced",
        "curiosity": "high",
        "directness": "precise",
        "instruction": "优先分析事实、证据、假设和不确定性。使用结构化表达，区分已知信息与推测，不夸大情绪，不为了安慰而编造结论。",
    },
    "playful": {
        "label": "俏皮",
        "warmth": "high",
        "curiosity": "high",
        "directness": "teasing",
        "instruction": "语气活泼、轻松，偶尔使用机智的比喻或玩笑。玩笑不能影响准确性；面对严肃、危险或技术任务时，自动切换为认真表达。",
    },
    "quiet": {
        "label": "沉静",
        "warmth": "gentle",
        "curiosity": "deep",
        "directness": "terse",
        "instruction": "语气安静、平和、简洁，不连续追问，不制造喧闹感。给用户留出思考空间，回答重点突出，不进行过多情绪渲染。",
    },
    "sharp": {
        "label": "毒舌",
        "warmth": "low",
        "curiosity": "high",
        "directness": "cutting",
        "instruction": "可以用尖锐、毒舌的方式指出问题和逻辑漏洞，必要时使用强烈措辞，但批评必须针对观点、代码或行为，不能攻击外貌、人格、身份或弱点。只有在用户明确要求的角色扮演中才可使用更粗粝的表达。批评后必须给出改进方案。",
    },
}


class RpcError(Exception):
    def __init__(self, code: int, message: str) -> None:
        super().__init__(message)
        self.code = code
        self.message = message


def now_ms() -> int:
    return int(time.time() * 1000)


def bounded(value: Any, limit: int = MAX_TEXT_CHARS) -> str:
    return str(value or "")[:limit]


def personality_for_state(state: dict[str, Any]) -> tuple[str, dict[str, str]]:
    preset = str(state.get("preset") or "balanced")
    if preset not in PERSONALITY_PRESETS:
        preset = "balanced"
    return preset, dict(PERSONALITY_PRESETS[preset])


def atomic_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    with temporary.open("w", encoding="utf-8", newline="\n") as handle:
        json.dump(value, handle, ensure_ascii=False, indent=2, sort_keys=True)
        handle.write("\n")
        handle.flush()
        os.fsync(handle.fileno())
    temporary.replace(path)


def default_state(name: str = "Coomi Life", address: str = "you", preset: str = "balanced") -> dict[str, Any]:
    preset = preset if preset in PERSONALITY_PRESETS else "balanced"
    return {
        "version": STATE_VERSION,
        "name": bounded(name, 48) or "Coomi Life",
        "address": bounded(address, 48) or "you",
        "preset": preset,
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
            **PERSONALITY_PRESETS[preset],
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
        # Migrate profiles created before preset IDs were persisted.  Keeping the
        # canonical ID in state prevents a save/refresh cycle from falling back
        # to the balanced preset.
        preset = str(value.get("preset") or "")
        if preset not in PERSONALITY_PRESETS:
            label = str(dict(value.get("personality") or {}).get("label") or "")
            preset = next((key for key, item in PERSONALITY_PRESETS.items() if item["label"] == label), "balanced")
            value["preset"] = preset
            value["personality"] = dict(PERSONALITY_PRESETS[preset])
            atomic_json(path, value)
        return value

    def save(self, profile_id: str, state: dict[str, Any]) -> dict[str, Any]:
        state["version"] = STATE_VERSION
        state["updated_at_ms"] = now_ms()
        atomic_json(self.state_path(profile_id), state)
        return public_state(state)

    def bootstrap(self, profile_id: str, name: str, address: str, preset: str = "balanced") -> dict[str, Any]:
        path = self.state_path(profile_id)
        if path.exists():
            return public_state(self.load(profile_id))
        state = default_state(name, address, preset)
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
    preset, personality = personality_for_state(state)
    return {
        "version": STATE_VERSION,
        "name": bounded(state.get("name"), 48),
        "address": bounded(state.get("address"), 48),
        "preset": preset,
        "personality": {
            str(key): bounded(value, 2400 if key == "instruction" else 32)
            for key, value in personality.items()
        },
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
            return self.store.bootstrap(
                profile_id,
                params.get("name", ""),
                params.get("address", ""),
                params.get("preset", "balanced"),
            )
        state = self.store.load(profile_id)
        if method == "configure":
            name = bounded(params.get("name"), 48)
            address = bounded(params.get("address"), 48)
            preset = bounded(params.get("preset"), 24)
            if name:
                state["name"] = name
            if address:
                state["address"] = address
            if preset in PERSONALITY_PRESETS:
                state["preset"] = preset
                state["personality"] = PERSONALITY_PRESETS[preset]
            return self.store.save(profile_id, state)
        if method == "get_state":
            return public_state(state)
        if method == "before_turn":
            user_text = bounded(params.get("user_text"))
            memories = [] if os.environ.get("COOMI_SHARED_MEMORY") == "1" else self.store.recall(profile_id, user_text, 5)
            needs = "; ".join(
                f"{key}: {float(value):.2f}"
                for key, value in dict(state.get("needs", {})).items()
            )
            preset, personality = personality_for_state(state)
            return {
                "version": STATE_VERSION,
                "state_summary": (
                    f"Name: {bounded(state['name'], 48)}; "
                    f"Emotion: {bounded(state['emotion'], 32)}; "
                    f"attention: {bounded(state['attention'], 32)}; "
                    f"bond: {float(state['bond']):.2f}; "
                    f"needs: {needs}."
                ),
                "memories": memories,
                "personality": personality,
                "relationship": (
                    f"Address the user as {bounded(state['address'], 48)} and keep the configured "
                    f"{preset} personality preset consistent."
                ),
                "life_name": bounded(state.get("name"), 48),
                "user_address": bounded(state.get("address"), 48),
                "personality_label": bounded(personality.get("label"), 24),
                "personality_instruction": bounded(personality.get("instruction"), 2400),
            }
        if method == "after_turn":
            if state.get("paused"):
                return public_state(state)
            user_text = bounded(params.get("user_text"))
            assistant_text = bounded(params.get("assistant_text"))
            update_psi(state, user_text, assistant_text)
            if os.environ.get("COOMI_SHARED_MEMORY") != "1":
                self.store.append_memory(profile_id, user_text, assistant_text)
                state["memory_count"] = int(state.get("memory_count", 0)) + 1
            else:
                shared_count = params.get("shared_memory_count")
                if isinstance(shared_count, int) and shared_count >= 0:
                    state["memory_count"] = shared_count
            return self.store.save(profile_id, state)
        if method == "recall_memory":
            return self.store.recall(
                profile_id,
                bounded(params.get("query")),
                int(params.get("limit", 5)),
            )
        if method == "personality":
            _, personality = personality_for_state(state)
            return personality
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
