package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidmain.media.MAX_AGENT_AVATAR_OUTPUT_DIMENSION
import com.ombhrum.fabushi.androidmain.media.MAX_AGENT_AVATAR_SOURCE_BYTES
import com.ombhrum.fabushi.androidmain.media.MAX_AGENT_AVATAR_SOURCE_DIMENSION
import com.ombhrum.fabushi.androidmain.media.avatarDecodeSampleSize
import com.ombhrum.fabushi.androidmain.media.avatarDimensionsAllowed
import com.ombhrum.fabushi.androidpreload.runtime.ANDROID_AGENT_AVATAR_MAX_ZOOM
import com.ombhrum.fabushi.androidpreload.runtime.AndroidAvatarCrop
import com.ombhrum.fabushi.androidpreload.runtime.AndroidAvatarImageSource
import com.ombhrum.fabushi.androidpreload.runtime.clampAndroidAvatarCrop
import com.ombhrum.fabushi.androidpreload.runtime.initialAndroidAvatarCrop
import com.ombhrum.fabushi.androidpreload.runtime.panAndroidAvatarCrop
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AgentProfileEditorTest {
    @Test
    fun avatarImagePolicyKeepsDesktopSourceAndOutputBounds() {
        assertEquals(25 * 1024 * 1024, MAX_AGENT_AVATAR_SOURCE_BYTES)
        assertEquals(4096, MAX_AGENT_AVATAR_SOURCE_DIMENSION)
        assertEquals(1024, MAX_AGENT_AVATAR_OUTPUT_DIMENSION)
        assertTrue(avatarDimensionsAllowed(1, 1))
        assertTrue(avatarDimensionsAllowed(4096, 4096))
        assertFalse(avatarDimensionsAllowed(4097, 1024))
        assertFalse(avatarDimensionsAllowed(1024, 4097))
    }

    @Test
    fun avatarImageDecodeSamplingBoundsLargeSourcesBeforeCanonicalResize() {
        assertEquals(1, avatarDecodeSampleSize(1024, 1024))
        assertEquals(2, avatarDecodeSampleSize(2048, 1024))
        assertEquals(4, avatarDecodeSampleSize(4096, 2048))
    }

    @Test
    fun avatarCropStartsCenteredAndClampsZoomAndCenters() {
        val source = AndroidAvatarImageSource("data:image/png;base64,AA==", width = 800, height = 400)
        assertEquals(AndroidAvatarCrop(1f, 400f, 200f), initialAndroidAvatarCrop(source))
        val bounded = clampAndroidAvatarCrop(
            source,
            AndroidAvatarCrop(zoom = 99f, centerX = -500f, centerY = 9_000f),
        )
        assertEquals(ANDROID_AGENT_AVATAR_MAX_ZOOM, bounded.zoom)
        assertEquals(40f, bounded.centerX, 0.001f)
        assertEquals(360f, bounded.centerY, 0.001f)
    }

    @Test
    fun avatarCropPanMatchesDesktopDragDirectionAndBounds() {
        val source = AndroidAvatarImageSource("data:image/png;base64,AA==", width = 800, height = 400)
        val start = AndroidAvatarCrop(zoom = 2f, centerX = 400f, centerY = 200f)
        val moved = panAndroidAvatarCrop(
            source = source,
            crop = start,
            deltaStageX = 20f,
            deltaStageY = -10f,
            stageSizePx = 200f,
        )
        assertTrue(moved.centerX < start.centerX)
        assertTrue(moved.centerY > start.centerY)
        assertTrue(moved.centerX >= 100f)
        assertTrue(moved.centerY in 100f..300f)
    }

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
    @Test
    fun settingsMutationFenceDropsReplyAfterAgentSelectionChanges() {
        val fence = AgentSettingsMutationFence()
        fence.select("agent-a", "account-a")
        val token = fence.beginMutation("agent-a", "account-a")
        assertNotNull(token)
        fence.select("agent-b", "account-a")
        assertFalse(fence.isCurrent(requireNotNull(token)))
    }

    @Test
    fun settingsMutationFenceDropsReplyAfterAccountSwitch() {
        val fence = AgentSettingsMutationFence()
        fence.select("agent-a", "account-a")
        val token = requireNotNull(fence.beginMutation("agent-a", "account-a"))
        fence.accountChanged("account-b")
        assertFalse(fence.isCurrent(token))
        assertNull(fence.beginMutation("agent-a", "account-b"))
    }

    @Test
    fun settingsMutationFenceDropsReplyAfterPanelDispose() {
        val fence = AgentSettingsMutationFence()
        fence.select("agent-a", "account-a")
        val token = requireNotNull(fence.beginMutation("agent-a", "account-a"))
        fence.clear("agent-a")
        assertFalse(fence.isCurrent(token))

        fence.select("agent-a", "account-a")
        val disposeToken = requireNotNull(fence.beginMutation("agent-a", "account-a"))
        fence.dispose()
        assertFalse(fence.isCurrent(disposeToken))
    }

    @Test
    fun settingsMutationFenceSerializesPendingMutationAndAllowsNextAfterFinish() {
        val fence = AgentSettingsMutationFence()
        fence.select("agent-a", "account-a")
        val first = requireNotNull(fence.beginMutation("agent-a", "account-a"))
        assertNull(fence.beginMutation("agent-a", "account-a"))
        assertTrue(fence.isCurrent(first))
        fence.finish(first)
        val second = requireNotNull(fence.beginMutation("agent-a", "account-a"))
        assertTrue(fence.isCurrent(second))
    }

    @Test
    fun desktopOnboardingPersonaValuesRemainCanonicalAlongsideLegacyAndroidValues() {
        assertEquals("teardrop", normalizeAgentAvatarShape(" Teardrop "))
        assertEquals("blob", normalizeAgentAvatarShape("BLOB"))
        assertEquals("circle", normalizeAgentAvatarShape("circle"))
        assertNull(normalizeAgentAvatarShape("triangle"))
        assertEquals("blue", normalizeAgentAvatarColor(" Blue "))
        assertEquals("#1A2B3C", normalizeAgentAvatarColor("#1a2b3c"))
        assertNull(normalizeAgentAvatarColor("purple-ish"))
        assertEquals("#1084FE", DESKTOP_AGENT_AVATAR_COLORS[normalizeAgentAvatarColor("blue")])
    }

    @Test
    fun avatarExportDecodesOnlyBoundedImageDataUrls() {
        val payload = byteArrayOf(1, 2, 3, 4)
        val encoded = java.util.Base64.getEncoder().encodeToString(payload)
        assertTrue(
            decodeAgentAvatarExportBytes("data:image/png;base64,$encoded")
                .contentEquals(payload),
        )
    }

    @Test
    fun avatarExportRejectsNonImageAndMalformedPayloads() {
        assertTrue(
            runCatching {
                decodeAgentAvatarExportBytes("data:text/plain;base64,SGVsbG8=")
            }.isFailure,
        )
        assertTrue(
            runCatching {
                decodeAgentAvatarExportBytes("data:image/png;base64,%%%")
            }.isFailure,
        )
        val oversized = java.util.Base64.getEncoder().encodeToString(ByteArray(4_000_001))
        assertTrue(
            runCatching {
                decodeAgentAvatarExportBytes("data:image/png;base64,$oversized")
            }.isFailure,
        )
    }

}
