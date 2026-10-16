package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class SettingsConfigurationModelsTest {
    @Test
    fun cursorAuthSubjectParsingMatchesDesktopContract() {
        assertEquals("subject-7", parseCursorAuthSubject("auth0|subject-7"))
        assertNull(parseCursorAuthSubject("invalid"))
        assertNull(parseCursorAuthSubject("|missing-prefix"))
        assertNull(parseCursorAuthSubject("missing-suffix|"))
    }

    @Test
    fun accountAvatarCandidatesMatchDesktopSecurityAndGithubFallbackContract() {
        assertEquals(
            listOf(
                "https://images.example.com/avatar.png",
                "https://avatars.githubusercontent.com/u/42?v=4&s=192",
            ),
            accountAvatarCandidateUrls(
                authId = "github|42",
                preferredUrl = "https://images.example.com/avatar.png",
            ),
        )
        assertEquals(
            listOf("https://avatars.githubusercontent.com/u/42?v=4&s=192"),
            accountAvatarCandidateUrls(
                authId = "github|42",
                preferredUrl = "http://images.example.com/avatar.png",
            ),
        )
        assertEquals(emptyList<String>(), accountAvatarCandidateUrls("google|42", "javascript:alert(1)"))
        assertNull(githubAccountAvatarUrl("github|not-numeric"))
    }

    @Test
    fun instructionRowsPreserveIdentityDuplicateRejectionAndCrossListMoves() {
        val base = AutoReviewInstructions(
            allowInstructions = listOf("read files", "run tests"),
            blockInstructions = listOf("delete files"),
        )
        assertEquals(
            listOf(
                AutoReviewInstructionRow(AutoReviewInstructionBehavior.ALLOW, "read files", 0),
                AutoReviewInstructionRow(AutoReviewInstructionBehavior.ALLOW, "run tests", 1),
                AutoReviewInstructionRow(AutoReviewInstructionBehavior.ASK, "delete files", 0),
            ),
            autoReviewInstructionRows(base),
        )
        assertNull(
            saveAutoReviewInstruction(
                base,
                "run tests",
                AutoReviewInstructionBehavior.ALLOW,
                editing = null,
            ),
        )

        val edited = saveAutoReviewInstruction(
            base,
            "run checks",
            AutoReviewInstructionBehavior.ALLOW,
            AutoReviewInstructionRow(AutoReviewInstructionBehavior.ALLOW, "run tests", 1),
        )!!
        assertEquals(listOf("read files", "run checks"), edited.allowInstructions)

        val moved = saveAutoReviewInstruction(
            base,
            "delete files",
            AutoReviewInstructionBehavior.ALLOW,
            AutoReviewInstructionRow(AutoReviewInstructionBehavior.ASK, "delete files", 0),
        )!!
        assertEquals(listOf("read files", "run tests", "delete files"), moved.allowInstructions)
        assertEquals(emptyList<String>(), moved.blockInstructions)
    }

    @Test
    fun reconcileTracksMovedIdentityAndTwentyItemLimitFailsClosed() {
        val instructions = AutoReviewInstructions(
            allowInstructions = listOf("a", "b"),
            blockInstructions = emptyList(),
        )
        assertEquals(
            AutoReviewInstructionRow(AutoReviewInstructionBehavior.ALLOW, "b", 1),
            reconcileAutoReviewInstructionRow(
                instructions,
                AutoReviewInstructionRow(AutoReviewInstructionBehavior.ALLOW, "b", 0),
            ),
        )
        assertNull(
            reconcileAutoReviewInstructionRow(
                instructions,
                AutoReviewInstructionRow(AutoReviewInstructionBehavior.ALLOW, "missing", 0),
            ),
        )
        val full = AutoReviewInstructions(
            allowInstructions = (0 until MAX_AUTO_REVIEW_INSTRUCTIONS_PER_BEHAVIOR).map { "rule-$it" },
            blockInstructions = emptyList(),
        )
        assertNull(
            saveAutoReviewInstruction(
                full,
                "overflow",
                AutoReviewInstructionBehavior.ALLOW,
                editing = null,
            ),
        )
    }

    @Test
    fun routerPreferenceIsVersionedAndFailsClosedToCursor() {
        assertEquals(
            listOf("cursor", "claude-code", "codex", "openrouter"),
            ROUTER_PROVIDERS_ANDROID.map { it.id.wireId },
        )
        assertEquals(RouterProviderId.CURSOR, parseRouterProviderPreferenceAndroid(null))
        assertEquals(RouterProviderId.CURSOR, parseRouterProviderPreferenceAndroid("bad-json"))
        assertEquals(
            RouterProviderId.CURSOR,
            parseRouterProviderPreferenceAndroid("""{"schemaVersion":2,"provider":"codex"}"""),
        )
        assertEquals(
            RouterProviderId.CURSOR,
            parseRouterProviderPreferenceAndroid("""{"schemaVersion":1,"provider":"unknown"}"""),
        )
        val encoded = encodeRouterProviderPreferenceAndroid(RouterProviderId.CODEX)
        assertEquals(RouterProviderId.CODEX, parseRouterProviderPreferenceAndroid(encoded))
        assertEquals("Codex", routerProviderByIdAndroid(RouterProviderId.CODEX).label)
    }
}
