package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidmain.startup.ANDROID_DATA_ROOT_MARKER
import com.ombhrum.fabushi.androidmain.startup.AndroidDataRootBootstrap
import java.nio.file.Files
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidDataRootBootstrapTest {
    @Test
    fun freshRootSettlesOnceAndReopensWithoutMutation() {
        val parent = Files.createTempDirectory("fabushi-data-root-test").toFile()
        try {
            val root = parent.resolve("files")
            val first = AndroidDataRootBootstrap.settleRoot(root)
            val marker = root.resolve(ANDROID_DATA_ROOT_MARKER)
            assertEquals(root.absoluteFile, first)
            assertEquals("{\"version\":1}\n", marker.readText(Charsets.UTF_8))
            val firstModified = marker.lastModified()
            val second = AndroidDataRootBootstrap.settleRoot(root)
            assertEquals(root.absoluteFile, second)
            assertEquals(firstModified, marker.lastModified())
            assertTrue(root.listFiles().orEmpty().none { it.name.contains(".tmp-") })
        } finally {
            parent.deleteRecursively()
        }
    }

    @Test
    fun unsupportedMarkerFailsClosed() {
        val parent = Files.createTempDirectory("fabushi-data-root-marker").toFile()
        try {
            val root = parent.resolve("files").apply { mkdirs() }
            root.resolve(ANDROID_DATA_ROOT_MARKER).writeText("{\"version\":999}\n")
            assertThrows(IllegalArgumentException::class.java) {
                AndroidDataRootBootstrap.settleRoot(root)
            }
        } finally {
            parent.deleteRecursively()
        }
    }

    @Test
    fun fileCannotBecomeCanonicalDataRoot() {
        val parent = Files.createTempDirectory("fabushi-data-root-file").toFile()
        try {
            val root = parent.resolve("files").apply { writeText("foreign") }
            assertThrows(IllegalArgumentException::class.java) {
                AndroidDataRootBootstrap.settleRoot(root)
            }
        } finally {
            parent.deleteRecursively()
        }
    }
}
