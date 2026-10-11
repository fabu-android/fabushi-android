package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidmain.deeplink.AndroidDeepLinkController
import com.ombhrum.fabushi.androidmain.deeplink.AndroidDeepLinkRouter
import com.ombhrum.fabushi.androidpreload.deeplink.AndroidDeepLink
import com.ombhrum.fabushi.androidpreload.deeplink.AndroidDeepLinkSource
import com.ombhrum.fabushi.androidpreload.deeplink.AuthCompletionStatus
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidDeepLinkRouterTest {
    @Test
    fun authCompletionIsAllowlistedAndTyped() {
        val link = AndroidDeepLinkRouter.parse(
            "fabushi://auth/complete?attemptId=attempt_123456&status=completed",
        )
        assertEquals(
            AndroidDeepLink.AuthCompletion(
                attemptId = "attempt_123456",
                status = AuthCompletionStatus.COMPLETED,
            ),
            link,
        )
    }

    @Test
    fun duplicateOrUnknownAuthParametersFailClosed() {
        assertNull(
            AndroidDeepLinkRouter.parse(
                "fabushi://auth/complete?attemptId=attempt_123456&attemptId=attempt_999999",
            ),
        )
        assertNull(
            AndroidDeepLinkRouter.parse(
                "fabushi://auth/complete?attemptId=attempt_123456&token=secret",
            ),
        )
        assertNull(
            AndroidDeepLinkRouter.parse(
                "fabushi://auth/complete?attemptId=short",
            ),
        )
    }

    @Test
    fun credentialsFragmentsTraversalAndForeignSchemesAreRejected() {
        assertNull(AndroidDeepLinkRouter.parse("https://auth/complete?attemptId=attempt_123456"))
        assertNull(AndroidDeepLinkRouter.parse("fabushi://user:pass@auth/complete?attemptId=attempt_123456"))
        assertNull(AndroidDeepLinkRouter.parse("fabushi://auth/../complete?attemptId=attempt_123456"))
        assertNull(AndroidDeepLinkRouter.parse("fabushi://auth/complete?attemptId=attempt_123456#fragment"))
    }

    @Test
    fun agentAndSectionRoutesCarryOnlyTypedDomainData() {
        val agent = AndroidDeepLinkRouter.parse("fabushi://agent/agent-42")
        assertEquals(AndroidDeepLink.Agent("agent-42"), agent)

        val section = AndroidDeepLinkRouter.parse("fabushi://settings")
        assertEquals(AndroidDeepLink.AppSection("settings"), section)
        assertTrue(AndroidDeepLinkRouter.parse("fabushi://settings/extra") == null)
    }
    @Test
    fun controllerQueuesUntilReadyAndDedupesWithinWindow() {
        var now = 1_000L
        val delivered = mutableListOf<AndroidDeepLink>()
        val controller = AndroidDeepLinkController(
            dispatch = { delivered.add(it) },
            nowMs = { now },
        )

        assertTrue(
            controller.handleCandidate(
                "fabushi://agent/agent-42",
                "test",
            ),
        )
        assertTrue(controller.hasPendingActivation())
        assertTrue(
            !controller.handleCandidate(
                "fabushi://agent/agent-42",
                "duplicate",
            ),
        )
        assertTrue(delivered.isEmpty())

        controller.markReady()
        assertEquals(listOf(AndroidDeepLink.Agent("agent-42")), delivered)

        now += 2_001L
        assertTrue(
            controller.handleCandidate(
                "fabushi://agent/agent-42",
                "after-window",
            ),
        )
        assertEquals(2, delivered.size)
    }

    @Test
    fun controllerCapsPendingActivationQueue() {
        val controller = AndroidDeepLinkController(dispatch = {})
        repeat(16) { index ->
            assertTrue(
                controller.handleCandidate(
                    "fabushi://agent/agent-${index}",
                    "test",
                ),
            )
        }
        assertTrue(
            !controller.handleCandidate(
                "fabushi://agent/overflow",
                "test",
            ),
        )
    }

    @Test
    fun mcpOAuthCallbackIsTypedAndFailClosed() {
        val success = AndroidDeepLinkRouter.parse(
            "fabushi://mcp-oauth/callback?state=0123456789abcdef&code=code-1",
        )
        assertEquals(
            AndroidDeepLink.McpOAuthCallback(
                state = "0123456789abcdef",
                code = "code-1",
                error = null,
            ),
            success,
        )

        val failure = AndroidDeepLinkRouter.parse(
            "fabushi://mcp-oauth/callback?state=0123456789abcdef&error=access_denied",
        )
        assertEquals(
            AndroidDeepLink.McpOAuthCallback(
                state = "0123456789abcdef",
                code = null,
                error = "access_denied",
            ),
            failure,
        )

        assertNull(
            AndroidDeepLinkRouter.parse(
                "fabushi://mcp-oauth/callback?state=0123456789abcdef&code=a&error=b",
            ),
        )
        assertNull(
            AndroidDeepLinkRouter.parse(
                "fabushi://mcp-oauth/callback?state=short&code=a",
            ),
        )
        assertNull(
            AndroidDeepLinkRouter.parse(
                "fabushi://mcp-oauth/callback?state=0123456789abcdef&state=other-state-12345&code=a",
            ),
        )
    }

    @Test
    fun deepLinkInfoActivationIsTypedAndAllowlisted() {
        assertEquals(
            AndroidDeepLink.Info(
                source = AndroidDeepLinkSource.PROTOCOL,
                topic = "deep-links",
            ),
            AndroidDeepLinkRouter.parse(
                "fabushi://app/v1/info?topic=deep-links",
            ),
        )
        assertNull(AndroidDeepLinkRouter.parse("fabushi://app/v1/info?topic=other"))
        assertNull(
            AndroidDeepLinkRouter.parse(
                "fabushi://app/v1/info?topic=deep-links&extra=1",
            ),
        )
    }

}
