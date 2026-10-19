package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.media.AndroidCallMediaPermissionStatus
import com.ombhrum.fabushi.androidpreload.media.projectAndroidHumanCallMediaPermissions
import org.junit.Assert.assertEquals
import org.junit.Test

class HumanCallMediaPermissionProjectionTest {
    @Test
    fun projectsPromptBeforeSystemRequestAndDenialAfterRequest() {
        val before = projectAndroidHumanCallMediaPermissions(
            audioRequested = true,
            videoRequested = true,
            microphoneGranted = false,
            cameraGranted = false,
            afterSystemRequest = false,
        )
        assertEquals(AndroidCallMediaPermissionStatus.PROMPT, before.microphone)
        assertEquals(AndroidCallMediaPermissionStatus.PROMPT, before.camera)

        val after = projectAndroidHumanCallMediaPermissions(
            audioRequested = true,
            videoRequested = true,
            microphoneGranted = true,
            cameraGranted = false,
            afterSystemRequest = true,
        )
        assertEquals(AndroidCallMediaPermissionStatus.GRANTED, after.microphone)
        assertEquals(AndroidCallMediaPermissionStatus.DENIED, after.camera)
    }

    @Test
    fun preservesNotRequestedForDisabledMediaKinds() {
        val projection = projectAndroidHumanCallMediaPermissions(
            audioRequested = false,
            videoRequested = true,
            microphoneGranted = false,
            cameraGranted = true,
            afterSystemRequest = true,
        )
        assertEquals(AndroidCallMediaPermissionStatus.NOT_REQUESTED, projection.microphone)
        assertEquals(AndroidCallMediaPermissionStatus.GRANTED, projection.camera)
    }
}
