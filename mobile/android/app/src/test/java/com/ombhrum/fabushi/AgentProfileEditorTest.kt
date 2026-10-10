package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class AgentProfileEditorTest {
    @Test
    fun profileCommitNormalizesNameAndBoundsDescription() {
        val projected = committedAgentProfile(
            initialName = "Old",
            initialDescription = "old description",
            draftName = "  New   Agent  ",
            draftDescription = "  " + "x".repeat(300) + "  ",
        )
        requireNotNull(projected)
        assertEquals("New Agent", projected.name)
        assertEquals(240, projected.description.length)
    }

    @Test
    fun profileCommitRejectsBlankName() {
        assertNull(
            committedAgentProfile(
                initialName = "Agent",
                initialDescription = "description",
                draftName = "   ",
                draftDescription = "updated",
            ),
        )
    }

    @Test
    fun unchangedProfileDoesNotCreateMutation() {
        assertNull(
            committedAgentProfile(
                initialName = "Agent",
                initialDescription = "description",
                draftName = "Agent",
                draftDescription = "description",
            ),
        )
    }

    @Test
    fun descriptionOnlyChangeCreatesCanonicalMutation() {
        val projected = committedAgentProfile(
            initialName = "Agent",
            initialDescription = "old",
            draftName = "Agent",
            draftDescription = "  new description  ",
        )
        requireNotNull(projected)
        assertEquals("Agent", projected.name)
        assertEquals("new description", projected.description)
    }
}
