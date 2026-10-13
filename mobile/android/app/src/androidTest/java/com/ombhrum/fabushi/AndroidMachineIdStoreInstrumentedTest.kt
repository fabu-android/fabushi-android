package com.ombhrum.fabushi

import android.content.Context
import android.content.ContextWrapper
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.ombhrum.fabushi.androidmain.security.AndroidMachineIdStore
import java.io.File
import java.util.UUID
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class AndroidMachineIdStoreInstrumentedTest {
    private lateinit var root: File
    private lateinit var context: Context
    private val protectedFile get() = File(context.noBackupFilesDir, PROTECTED_FILE)
    private val legacyFile get() = File(context.filesDir, LEGACY_FILE)

    @Before
    fun setUp() {
        val application = ApplicationProvider.getApplicationContext<Context>()
        root = File(application.cacheDir, "machine-id-test-${UUID.randomUUID()}")
        context = IsolatedContext(application, root)
    }

    @After
    fun tearDown() {
        root.deleteRecursively()
    }

    @Test
    fun protectedMachineIdIsStableAndNeverStoredAsPlaintext() {
        val first = AndroidMachineIdStore(context).getOrCreate()
        val second = AndroidMachineIdStore(context).getOrCreate()

        assertEquals(first, second)
        assertTrue(first.isNotBlank())
        assertTrue(protectedFile.isFile)
        assertFalse(protectedFile.readText().contains(first))
        assertFalse(legacyFile.exists())
    }

    @Test
    fun legacyPlaintextIdentityMigratesOnceAndIsRemoved() {
        val legacy = "legacy-machine-id-${UUID.randomUUID()}"
        legacyFile.parentFile?.mkdirs()
        legacyFile.writeText(legacy)

        val resolved = AndroidMachineIdStore(context).getOrCreate()

        assertEquals(legacy, resolved)
        assertFalse(legacyFile.exists())
        assertTrue(protectedFile.isFile)
        assertFalse(protectedFile.readText().contains(legacy))
        assertEquals(legacy, AndroidMachineIdStore(context).getOrCreate())
    }

    @Test
    fun concurrentStoreInstancesConvergeOnOneDurableMachineId() {
        val workers = Executors.newFixedThreadPool(8)
        val start = CountDownLatch(1)
        try {
            val futures = (0 until 16).map {
                workers.submit<String> {
                    start.await()
                    AndroidMachineIdStore(context).getOrCreate()
                }
            }
            start.countDown()

            val ids = futures.map { it.get(10, TimeUnit.SECONDS) }.toSet()
            assertEquals(1, ids.size)
            assertEquals(ids.single(), AndroidMachineIdStore(context).getOrCreate())
            assertTrue(protectedFile.isFile)
            assertFalse(File(context.noBackupFilesDir, "${PROTECTED_FILE}.tmp").exists())
        } finally {
            workers.shutdownNow()
        }
    }

    @Test
    fun corruptProtectedIdentityFailsClosedInsteadOfRotating() {
        protectedFile.parentFile?.mkdirs()
        protectedFile.writeText("fabushi-machine-id-v1\nnot-base64\nstill-not-base64")

        assertThrows(IllegalStateException::class.java) {
            AndroidMachineIdStore(context).getOrCreate()
        }
        assertTrue(protectedFile.exists())
        assertFalse(legacyFile.exists())
    }

    private class IsolatedContext(
        base: Context,
        private val root: File,
    ) : ContextWrapper(base) {
        override fun getApplicationContext(): Context = this

        override fun getFilesDir(): File =
            File(root, "files").also { it.mkdirs() }

        override fun getNoBackupFilesDir(): File =
            File(root, "no-backup").also { it.mkdirs() }
    }

    private companion object {
        const val PROTECTED_FILE = "fabushi-machine-id.v1"
        const val LEGACY_FILE = "machine-id"
    }
}
