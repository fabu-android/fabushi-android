package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidmain.auth.AndroidAuthCallbackRegistration
import com.ombhrum.fabushi.androidmain.deeplink.AndroidDeepLinkRouter
import com.ombhrum.fabushi.androidpreload.deeplink.AndroidDeepLink
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidAuthCallbackRegistrationTest {
    @Test
    fun registrationUsesTheManifestOwnedFabushiScheme() {
        val registration = AndroidAuthCallbackRegistration.registration()
        assertTrue(registration.registered)
        assertEquals("android-manifest", registration.registrationSource)
        assertEquals("fabushi", registration.redirectTarget)
        assertEquals("fabushi", registration.protocolScheme)
    }

    @Test
    fun configuredSchemeValidationFailsClosed() {
        assertEquals("fabushi", AndroidAuthCallbackRegistration.validateConfiguredScheme(null))
        assertEquals("custom+auth", AndroidAuthCallbackRegistration.validateConfiguredScheme(" Custom+Auth "))
        runCatching {
            AndroidAuthCallbackRegistration.validateConfiguredScheme("9 invalid")
        }.onSuccess {
            error("invalid protocol token unexpectedly accepted")
        }
    }

    @Test
    fun authRouterAcceptsOnlyTheRegisteredScheme() {
        val parsed = AndroidDeepLinkRouter.parse(
            "fabushi://auth/complete?attemptId=attempt_123456&status=completed",
        )
        assertTrue(parsed is AndroidDeepLink.AuthCompletion)
        assertEquals(
            null,
            AndroidDeepLinkRouter.parse(
                "sand://auth/complete?attemptId=attempt_123456&status=completed",
            ),
        )
    }
}
