package com.ombhrum.fabushi

import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.ombhrum.fabushi.androidmain.security.AndroidMachineIdStore
import java.io.File
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
    private val context = ApplicationProvider.getApplicationContext<android.content.Context>()
    private val protectedFile = File(context.noBackupFilesDir, "fabushi-machine-id.v1")
    private val legacyFile = File(context.filesDir, "machine-id")

    @Before
    fun resetFiles() {
        protectedFile.delete()
        legacyFile.delete()
    }

    @After
    fun cleanup() {
        protectedFile.delete()
        legacyFile.delete()
    }

    @Test
    fun protectedMachineIdIsStableAndNeverStoredAsPlaintext() {
        val store = AndroidMachineIdStore(context)
        val first = store.getOrCreate()
        val second = store.getOrCreate()
        assertEquals(first, second)
        assertTrue(first.isNotBlank())
        assertTrue(protectedFile.exists())
        assertFalse(protectedFile.readText().contains(first))
        assertFalse(legacyFile.exists())
    }

    @Test
    fun legacyPlaintextIdentityMigratesOnceAndIsRemoved() {
        val legacy = "legacy-machine-id-123"
        legacyFile.parentFile?.mkdirs()
        legacyFile.writeText(legacy)
        val resolved = AndroidMachineIdStore(context).getOrCreate()
        assertEquals(legacy, resolved)
        assertFalse(legacyFile.exists())
        assertTrue(protectedFile.exists())
        assertFalse(protectedFile.readText().contains(legacy))
        assertEquals(legacy, AndroidMachineIdStore(context).getOrCreate())
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
}
