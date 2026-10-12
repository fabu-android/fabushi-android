import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
HOME = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/features/bots/GrokHomeSurface.kt"
VM = ROOT / "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/presentation/MobileBotViewModel.kt"
RUNTIME = ROOT / "source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/coordinator/AndroidCoordinatorRuntime.kt"


class AgentGroupMembersWiringTest(unittest.TestCase):
    def test_group_member_mutation_uses_durable_roster_owner(self):
        runtime = RUNTIME.read_text(encoding="utf-8")
        start = runtime.index("override fun agentSetGroupMembers")
        end = runtime.index("override fun agentUpdate", start)
        block = runtime[start:end]
        self.assertIn("durableAgentRosterMutation(", block)
        self.assertIn('.put("kind", "group-members")', block)
        self.assertIn('.put("memberIds", members)', block)
        self.assertNotIn('host.request("setGroupMembers"', block)

    def test_dialog_waits_for_durable_settlement_and_fences_duplicate_save(self):
        home = HOME.read_text(encoding="utf-8")
        vm = VM.read_text(encoding="utf-8")
        self.assertIn("groupMembersPending = botState.groupMembersUpdatingId == group.id", home)
        self.assertIn("if (!groupMembersPending) editingGroup = null", home)
        self.assertIn('Text(if (groupMembersPending) "Saving…" else "Save")', home)
        self.assertIn("enabled = !groupMembersPending", home)
        self.assertIn("mutableState.value.groupMembersUpdatingId != null", vm)
        self.assertIn("generation == groupMembersGeneration", vm)
        self.assertIn("mutableState.value.groupMembersUpdatingId == groupId", vm)
        self.assertIn("onUpdated?.invoke()", vm)

    def test_group_members_remain_bounded_and_nonempty(self):
        home = HOME.read_text(encoding="utf-8")
        vm = VM.read_text(encoding="utf-8")
        self.assertIn("editingGroupMemberIds.size < 6", home)
        self.assertIn("editingGroupMemberIds.isNotEmpty()", home)
        self.assertIn(".distinct().take(6)", vm)
        self.assertIn("members.isEmpty()", vm)


if __name__ == "__main__":
    unittest.main()
