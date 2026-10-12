package com.ombhrum.fabushi

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class RootShellNotificationHostTest {
    @Test fun projectsOnlyErrorsAndAtMostThreeSafeActions() {
        val actions = JSONArray()
            .put(JSONObject().put("kind", "open-url").put("label", "Docs").put("url", "https://example.test"))
            .put(JSONObject().put("kind", "open-url").put("label", "Bad").put("url", "javascript:alert(1)"))
            .put(JSONObject().put("kind", "dashboard-action").put("label", "Upgrade").put("action", "upgrade").put("args", JSONObject().put("plan", "pro")))
            .put(JSONObject().put("kind", "open-url").put("label", "Help").put("url", "http://example.test/help"))
            .put(JSONObject().put("kind", "open-url").put("label", "Extra").put("url", "https://example.test/extra"))
        val tray = projectRootShellNotificationTray(
            JSONObject()
                .put("kind", "error")
                .put("id", "tray-1")
                .put("title", "Error")
                .put("count", 3)
                .put("actions", actions),
        )!!
        assertEquals(3, tray.actions.size)
        assertEquals(3L, tray.count)
        assertNull(projectRootShellNotificationTray(JSONObject().put("kind", "info").put("id", "x").put("title", "Info")))
    }

    @Test fun reducerReplacesPushAndHandlesDismissAndClear() {
        val first = RootShellNotificationTray("one", "First")
        val replacement = JSONObject()
            .put("type", "tray.pushed")
            .put("tray", JSONObject().put("kind", "error").put("id", "one").put("title", "Updated"))
        val updated = reduceRootShellNotificationEvent(listOf(first), replacement)
        assertEquals(listOf("Updated"), updated.map { it.title })
        assertTrue(
            reduceRootShellNotificationEvent(
                updated,
                JSONObject().put("type", "tray.dismissed").put("id", "one"),
            ).isEmpty(),
        )
        assertTrue(
            reduceRootShellNotificationEvent(
                listOf(first),
                JSONObject().put("type", "tray.cleared"),
            ).isEmpty(),
        )
    }

    @Test fun accountScopeRequiresSettledIdentityAndFencesStaleEpoch() {
        val first = RootShellNotificationAccountScope(7L, 4L, "account-a")
        val sameAccountNewGeneration = RootShellNotificationAccountScope(8L, 4L, "account-a")
        val switched = RootShellNotificationAccountScope(1L, 5L, "account-b")
        assertTrue(first != sameAccountNewGeneration)
        assertTrue(switched.accountEpoch > first.accountEpoch)
        assertEquals("account-b", switched.accountIdentity)
    }

    @Test fun dashboardArgsFailClosedWhenAnyValueIsNotString() {
        assertNull(
            projectRootShellNotificationAction(
                JSONObject()
                    .put("kind", "dashboard-action")
                    .put("label", "Upgrade")
                    .put("action", "upgrade")
                    .put("args", JSONObject().put("plan", 2)),
            ),
        )
    }
}
