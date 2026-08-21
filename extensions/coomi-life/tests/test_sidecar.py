from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path


SPEC = importlib.util.spec_from_file_location("coomi_life_sidecar", Path(__file__).parents[1] / "sidecar.py")
assert SPEC and SPEC.loader
SIDECAR = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SIDECAR)


class SidecarTests(unittest.TestCase):
    def test_profile_memory_isolation_and_reset(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            dispatcher = SIDECAR.Dispatcher(SIDECAR.LifeStore(Path(directory)))
            dispatcher.dispatch("bootstrap", {"profile_id": "one", "name": "One", "address": "User"})
            dispatcher.dispatch("bootstrap", {"profile_id": "two", "name": "Two", "address": "User"})
            dispatcher.dispatch(
                "after_turn",
                {"profile_id": "one", "user_text": "remember cobalt", "assistant_text": "noted"},
            )
            self.assertEqual(len(dispatcher.dispatch("recall_memory", {"profile_id": "one", "query": "cobalt", "limit": 5})), 1)
            self.assertEqual(dispatcher.dispatch("recall_memory", {"profile_id": "two", "query": "cobalt", "limit": 5}), [])
            reset = dispatcher.dispatch("reset", {"profile_id": "one"})
            self.assertEqual(reset["memory_count"], 0)

    def test_pause_prevents_state_and_memory_updates(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            dispatcher = SIDECAR.Dispatcher(SIDECAR.LifeStore(Path(directory)))
            dispatcher.dispatch("bootstrap", {"profile_id": "one", "name": "One", "address": "User"})
            dispatcher.dispatch("pause", {"profile_id": "one", "paused": True})
            state = dispatcher.dispatch(
                "after_turn",
                {"profile_id": "one", "user_text": "hello", "assistant_text": "hello"},
            )
            self.assertEqual(state["memory_count"], 0)
            self.assertTrue(state["paused"])

    def test_configure_updates_public_identity_and_personality(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            dispatcher = SIDECAR.Dispatcher(SIDECAR.LifeStore(Path(directory)))
            dispatcher.dispatch("bootstrap", {"profile_id": "one", "name": "One", "address": "User"})
            configured = dispatcher.dispatch(
                "configure",
                {"profile_id": "one", "name": "Nova", "address": "朋友", "preset": "warm"},
            )
            self.assertEqual(configured["name"], "Nova")
            self.assertEqual(configured["address"], "朋友")
            self.assertEqual(dispatcher.dispatch("personality", {"profile_id": "one"})["warmth"], "high")

    def test_export_and_delete_are_complete(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            dispatcher = SIDECAR.Dispatcher(SIDECAR.LifeStore(root / "state"))
            dispatcher.dispatch("bootstrap", {"profile_id": "one", "name": "One", "address": "User"})
            exported = dispatcher.dispatch("export", {"profile_id": "one", "destination": str(root / "life.zip")})
            self.assertEqual(len(exported["sha256"]), 64)
            dispatcher.dispatch("delete", {"profile_id": "one"})
            self.assertFalse((root / "state" / "one").exists())


if __name__ == "__main__":
    unittest.main()
