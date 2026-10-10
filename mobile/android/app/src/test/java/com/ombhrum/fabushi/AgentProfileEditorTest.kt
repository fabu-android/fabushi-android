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
    fun avatarPersonaChangeCreatesCanonicalProfileMutation() {
        val projected = committedAgentProfile(
            initialName = "Agent",
            initialDescription = "description",
            draftName = "Agent",
            draftDescription = "description",
            initialAvatarShape = null,
            initialAvatarColor = null,
            draftAvatarShape = " Circle ",
            draftAvatarColor = "#1a2b3c",
        )
        requireNotNull(projected)
        assertEquals("circle", projected.avatarShape)
        assertEquals("#1A2B3C", projected.avatarColor)
    }

    @Test
    fun avatarPersonaResetIsRepresentedAsNull() {
        val projected = committedAgentProfile(
            initialName = "Agent",
            initialDescription = "description",
            draftName = "Agent",
            draftDescription = "description",
            initialAvatarShape = "squircle",
            initialAvatarColor = "#1685F7",
            draftAvatarShape = null,
            draftAvatarColor = null,
        )
        requireNotNull(projected)
        assertNull(projected.avatarShape)
        assertNull(projected.avatarColor)
    }

    @Test
    fun titleIsTrimmedBoundedAndPartOfCanonicalProfileMutation() {
        val projected = committedAgentProfile(
            initialName = "Agent",
            initialTitle = "Old title",
            initialDescription = "description",
            draftName = "Agent",
            draftTitle = "  " + "T".repeat(140) + "  ",
            draftDescription = "description",
        )
        requireNotNull(projected)
        assertEquals(120, projected.title?.length)
    }

    @Test
    fun blankTitleClearsOptionalCanonicalTitle() {
        val projected = committedAgentProfile(
            initialName = "Agent",
            initialTitle = "Existing",
            initialDescription = "description",
            draftName = "Agent",
            draftTitle = "   ",
            draftDescription = "description",
        )
        requireNotNull(projected)
        assertNull(projected.title)
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
