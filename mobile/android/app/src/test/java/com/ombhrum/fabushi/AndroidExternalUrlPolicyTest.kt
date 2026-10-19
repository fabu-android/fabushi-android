package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidmain.adapters.AndroidAccountOAuthAdapter
import com.ombhrum.fabushi.androidmain.adapters.AndroidExternalUrlPolicy
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class AndroidExternalUrlPolicyTest {
    @Test
    fun ordinaryExternalUrlsStripForeignWebLoginCredentialsFromQueryAndFragment() {
        assertEquals(
            "https://api.ombhrum.com/login?keep=1#/route/?ok=2",
            AndroidExternalUrlPolicy.parseAllowedExternalUrl(
                "https://api.ombhrum.com/login?keep=1&tgWebAuthToken=foreign&%2561utologin_token=foreign-two#/route/?ok=2&autologin_token=foreign-three",
            ),
        )
    }

    @Test
    fun ordinaryPolicyCannotPreserveCredentialByCasingOrRepeatedEncoding() {
        assertEquals(
            "https://api.ombhrum.com/login?safe=1",
            AndroidExternalUrlPolicy.parseAllowedExternalUrl(
                "https://api.ombhrum.com/login?TGWEBAUTHanything=secret&%253FtgWebAuthToken=secret-two&safe=1",
            ),
        )
    }

    @Test
    fun serverAcceptedAuthPreservesExactFirstPartyUrlOnly() {
        val accepted = "https://api.ombhrum.com/loginDeepControl?autologin_token=server-issued&attempt=abc#done"
        assertEquals(
            accepted,
            AndroidExternalUrlPolicy.parseServerAcceptedAuthExternalUrl(
                accepted,
                "https://api.ombhrum.com",
            )?.toASCIIString(),
        )
        assertEquals(accepted, AndroidAccountOAuthAdapter.validateExternalAuthUrl(accepted)?.toASCIIString())
        assertNull(
            AndroidExternalUrlPolicy.parseServerAcceptedAuthExternalUrl(
                accepted,
                "https://other.example",
            ),
        )
        assertNull(
            AndroidExternalUrlPolicy.parseServerAcceptedAuthExternalUrl(
                "https://user:secret@api.ombhrum.com/login",
                "https://api.ombhrum.com",
            ),
        )
        assertNull(
            AndroidExternalUrlPolicy.parseServerAcceptedAuthExternalUrl(
                "http://api.ombhrum.com/login",
                "https://api.ombhrum.com",
            ),
        )
    }

    @Test
    fun nonHttpAllowlistRetainsSafeFamiliesAndRejectsScriptUrls() {
        assertEquals(
            "mailto:help@example.com",
            AndroidExternalUrlPolicy.parseAllowedExternalUrl("mailto:help@example.com"),
        )
        assertNull(AndroidExternalUrlPolicy.parseAllowedExternalUrl("javascript:alert(1)"))
    }
}
