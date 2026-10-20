package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.PreloadBrowserBase
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PreloadBrowserBaseTest {
    @Test
    fun enterpriseIdentityAllowlistUsesBoundedSuffixMatching() {
        assertTrue(PreloadBrowserBase.isAllowlistedIdentityHost("tenant.okta.com"))
        assertTrue(PreloadBrowserBase.isAllowlistedIdentityHost("login.microsoftonline.com"))
        assertTrue(PreloadBrowserBase.isAllowlistedIdentityHost("tenant.auth0.com"))
        assertTrue(PreloadBrowserBase.isAllowlistedIdentityHost("accounts.google.com"))
        assertTrue(PreloadBrowserBase.isAllowlistedIdentityHost("GITHUB.COM."))

        assertFalse(PreloadBrowserBase.isAllowlistedIdentityHost("okta.com"))
        assertFalse(PreloadBrowserBase.isAllowlistedIdentityHost("evilokta.com"))
        assertFalse(PreloadBrowserBase.isAllowlistedIdentityHost("okta.com.evil.example"))
        assertFalse(PreloadBrowserBase.isAllowlistedIdentityHost(""))
    }

    @Test
    fun loopbackPolicyRemainsExplicit() {
        assertTrue(PreloadBrowserBase.isLoopbackHost("localhost"))
        assertTrue(PreloadBrowserBase.isLoopbackHost("127.0.0.1"))
        assertTrue(PreloadBrowserBase.isLoopbackHost("::1"))
        assertFalse(PreloadBrowserBase.isLoopbackHost("localhost.example"))
    }
}
