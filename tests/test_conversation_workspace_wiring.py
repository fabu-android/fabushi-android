import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
SURFACE = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/features/conversations/ConversationSurfaces.kt"
WORKSPACE = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/recovered/features/conversation/workspace"


class ConversationWorkspaceWiringTest(unittest.TestCase):
    def test_shipping_surface_uses_workspace_transcript_and_composer(self):
        text = SURFACE.read_text(encoding="utf-8")
        self.assertIn("ConversationTranscript(", text)
        self.assertIn("ConversationComposer(", text)
        self.assertIn("ConversationChatHeader(", text)
        self.assertIn("FindInChatBar(", text)
        self.assertIn("coordinatorComposerDraftSnapshot(", text)
        self.assertNotIn("items(messages.filter { chatSearchQuery", text)
        self.assertNotIn('DropdownMenu(expanded = showAttachmentMenu', text)

    def test_exact_ledger_targets_exist(self):
        for name in (
            "transcript_view.kt",
            "composer_view.kt",
            "reply-preview_view.kt",
            "chat-header_view.kt",
            "find-in-chat-controller.kt",
            "find-in-chat_view.kt",
            "draft-state.kt",
            "media-viewer_view.kt",
        ):
            path = WORKSPACE / name
            self.assertTrue(path.is_file(), path)
            self.assertGreater(path.stat().st_size, 500, path)

    def test_legacy_media_viewer_path_is_removed(self):
        legacy = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/features/media/AndroidMediaViewer.kt"
        self.assertFalse(legacy.exists(), legacy)


if __name__ == "__main__":
    unittest.main()
