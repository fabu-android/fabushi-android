package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.assertThrows
import org.junit.Test

class AndroidVersionTest {
    @Test
    fun newerSemanticVersionWinsEvenWithLowerBuildCode() {
        assertTrue(AndroidVersion.isNewer("1.0.5", 1, "1.0.4", 999_999))
        assertFalse(AndroidVersion.isNewer("1.0.3", 2_000_000, "1.0.4", 1))
    }

    @Test
    fun sameVersionUsesMonotonicAndroidVersionCode() {
        assertTrue(AndroidVersion.isNewer("1.0.4", 101, "1.0.4", 100))
        assertFalse(AndroidVersion.isNewer("1.0.4", 100, "1.0.4", 100))
    }

    @Test
    fun stableVersionSortsAfterPrerelease() {
        assertTrue(AndroidVersion.compare("2.0.0", "2.0.0-beta.4") > 0)
        assertTrue(AndroidVersion.compare("2.0.0-beta.10", "2.0.0-beta.2") > 0)
        assertTrue(AndroidVersion.compare("2.0.0-1", "2.0.0-alpha") < 0)
        assertTrue(AndroidVersion.compare("2.0.0-alpha", "2.0.0-BETA") > 0)
    }

    @Test
    fun parserMatchesDesktopStrictReleaseContract() {
        for (invalid in listOf(
            "v1.2.3",
            "1.2",
            "1.2.3.4",
            "1.2.3+build",
            "1.2.3-",
            "1.2.3-alpha.",
            " 1.2.3",
            "1.2.3 ",
            "one.two.three",
        )) {
            assertThrows(IllegalArgumentException::class.java) {
                AndroidVersion.compare(invalid, "1.2.3")
            }
        }
        assertEquals(0, AndroidVersion.compare("1.2.3", "1.2.3"))
        assertFalse(AndroidVersion.isPrerelease("1.2.3"))
        assertTrue(AndroidVersion.isPrerelease("1.2.3-alpha.1"))
        assertFalse(AndroidVersion.isPrerelease("v1.2.3-alpha"))
    }
}
