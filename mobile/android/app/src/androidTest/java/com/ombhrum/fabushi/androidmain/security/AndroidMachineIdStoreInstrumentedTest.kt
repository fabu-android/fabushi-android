package com.ombhrum.fabushi.androidmain.security

import android.content.Context
import android.content.ContextWrapper
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import java.io.File
import java.util.UUID
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.assertThrows
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class AndroidMachineIdStoreInstrumentedTest {
    private lateinit var root: File
    private lateinit var context: Context

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
    fun protectedIdentitySurvivesColdReopenWithoutPlaintextFallback() {
        val first = AndroidMachineIdStore(context).getOrCreate()
        val second = AndroidMachineIdStore(context).getOrCreate()

        assertEquals(first, second)
        assertTrue(first.isNotBlank())
        assertTrue(File(context.noBackupFilesDir, PROTECTED_FILE).isFile)
        assertFalse(File(context.filesDir, LEGACY_FILE).exists())
    }

    @Test
    fun legacyPlaintextMigratesOnlyAfterProtectedCommitAndIsThenRemoved() {
        val legacyId = "legacy-${UUID.randomUUID()}"
        val legacy = File(context.filesDir, LEGACY_FILE)
        legacy.parentFile?.mkdirs()
        legacy.writeText(legacyId)

        val migrated = AndroidMachineIdStore(context).getOrCreate()

        assertEquals(legacyId, migrated)
        assertFalse("legacy plaintext machine id must be removed", legacy.exists())
        assertTrue(File(context.noBackupFilesDir, PROTECTED_FILE).isFile)
        assertEquals(legacyId, AndroidMachineIdStore(context).getOrCreate())
    }

    @Test
    fun corruptProtectedStateFailsClosedInsteadOfRotatingIdentity() {
        val protected = File(context.noBackupFilesDir, PROTECTED_FILE)
        protected.parentFile?.mkdirs()
        protected.writeText("corrupt-protected-machine-id")

        assertThrows(IllegalArgumentException::class.java) {
            AndroidMachineIdStore(context).getOrCreate()
        }
        assertFalse(
            "corrupt protected state must not create a replacement plaintext identity",
            File(context.filesDir, LEGACY_FILE).exists(),
        )
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
