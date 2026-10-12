package com.ombhrum.fabushi

import java.net.InetAddress
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class TranscriptUrlCardMetadataTest {
    @Test
    fun htmlMetadataProjectionPrefersOpenGraphAndKeepsHostname() {
        val metadata = projectTranscriptLinkMetadataHtml(
            "https://example.test/path",
            """
                <html><head>
                <title>Fallback title</title>
                <meta property="og:title" content="Open Graph title">
                <meta name="description" content="Description &amp; detail">
                <meta property="og:site_name" content="Example Site">
                </head></html>
            """.trimIndent(),
        )!!
        assertEquals("Open Graph title", metadata.title)
        assertEquals("Description & detail", metadata.description)
        assertEquals("Example Site", metadata.siteName)
        assertEquals("example.test", metadata.hostname)
    }

    @Test
    fun metadataProjectionRejectsNonHttpUrls() {
        assertNull(projectTranscriptLinkMetadataHtml("file:///private/data", "<title>x</title>"))
        assertNull(projectTranscriptLinkMetadataHtml("javascript:alert(1)", "<title>x</title>"))
    }

    @Test
    fun addressFenceRejectsLocalAndPrivateNetworks() {
        assertFalse(isPublicTranscriptLinkAddress(InetAddress.getByName("127.0.0.1")))
        assertFalse(isPublicTranscriptLinkAddress(InetAddress.getByName("10.0.0.1")))
        assertFalse(isPublicTranscriptLinkAddress(InetAddress.getByName("192.168.1.2")))
        assertFalse(isPublicTranscriptLinkAddress(InetAddress.getByName("100.64.0.1")))
        assertFalse(isPublicTranscriptLinkAddress(InetAddress.getByName("::1")))
        assertFalse(isPublicTranscriptLinkAddress(InetAddress.getByName("fc00::1")))
        assertTrue(isPublicTranscriptLinkAddress(InetAddress.getByName("1.1.1.1")))
        assertTrue(isPublicTranscriptLinkAddress(InetAddress.getByName("2606:4700:4700::1111")))
    }
}
