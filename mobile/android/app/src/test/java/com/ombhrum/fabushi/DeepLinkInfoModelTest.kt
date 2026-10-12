package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.deeplink.AndroidDeepLink
import com.ombhrum.fabushi.androidpreload.deeplink.AndroidDeepLinkSource
import org.junit.Assert.assertEquals
import org.junit.Test

class DeepLinkInfoModelTest {
    @Test
    fun protocolAndHttpsSourcesProjectToNativeOverlayLabels() {
        val protocol = AndroidDeepLink.Info(
            source = AndroidDeepLinkSource.PROTOCOL,
            topic = "deep-links",
        ).toDeepLinkInfo()
        val https = AndroidDeepLink.Info(
            source = AndroidDeepLinkSource.HTTPS,
            topic = "deep-links",
        ).toDeepLinkInfo()

        assertEquals("fabushi://app/v1/info?topic=deep-links", deepLinkRoute(protocol))
        assertEquals("Custom protocol (fabushi://)", deepLinkSourceLabel(protocol.source))
        assertEquals("HTTPS link", deepLinkSourceLabel(https.source))
        assertEquals("deep-links", https.topic)
    }
}
