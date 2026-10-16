package com.ombhrum.fabushi

import java.net.URI
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ApplicationMenuReplacementPolicyTest {
    @Test
    fun help_center_preserves_current_desktop_target_and_is_https() {
        val uri = URI(FABUSHI_HELP_CENTER_URL)
        assertEquals("https", uri.scheme)
        assertEquals("cursor.com", uri.host)
        assertNull(uri.userInfo)
        assertEquals("/help", uri.path)
        assertTrue(uri.fragment.isNullOrEmpty())
    }
}
