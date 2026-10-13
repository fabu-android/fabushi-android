package com.ombhrum.fabushi

import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.ombhrum.fabushi.androidmain.security.AndroidAccountSessionStore
import java.io.File
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class AndroidAccountSessionStoreInstrumentedTest {
    private val context = ApplicationProvider.getApplicationContext<android.content.Context>()
    private val store = AndroidAccountSessionStore(context)
    private val ciphertext = File(context.noBackupFilesDir, "fabushi-account-session.v1")

    @After
    fun cleanup() {
        store.clear()
    }

    @Test
    fun keystoreRoundTripKeepsBearerOutOfDiskPlaintext() {
        val token = "abcdefghijklmnopqrstuvwxyz0123456789"
        val session = """{"accessToken":"$token","refreshToken":"refresh-abcdefghijklmnopqrstuvwxyz","accessTokenExpiresAt":2000000000000,"refreshTokenExpiresAt":2100000000000,"sessionId":"session-1","deviceId":"device-1","username":"user@example.com","userId":"user-1"}"""

        store.writeSessionJson(session)

        assertEquals(session, store.readSessionJson())
        val raw = ciphertext.readText()
        assertTrue(raw.startsWith("fabushi-account-session-v1\n"))
        assertFalse(raw.contains(token))
        assertFalse(raw.contains("refresh-abcdefghijklmnopqrstuvwxyz"))
    }

    @Test
    fun corruptCiphertextFailsClosedAndIsRemoved() {
        ciphertext.parentFile?.mkdirs()
        ciphertext.writeText("fabushi-account-session-v1\nnot-base64\nstill-not-base64")

        assertNull(store.readSessionJson())
        assertFalse(ciphertext.exists())
    }

    @Test
    fun clearCryptographicallyErasesSessionAcrossStoreReopen() {
        val session = """{"accessToken":"access-token","refreshToken":"refresh-token","accessTokenExpiresAt":2000000000000,"refreshTokenExpiresAt":2100000000000,"sessionId":"session-clear","deviceId":"device-clear","username":"user@example.com","userId":"user-clear"}"""
        store.writeSessionJson(session)

        store.clear()

        assertFalse(ciphertext.exists())
        assertNull(AndroidAccountSessionStore(context).readSessionJson())
    }

    @Test
    fun failedCiphertextDeletionStillDestroysKeyAndPreventsSessionResurrection() {
        val session = """{"accessToken":"access-token","refreshToken":"refresh-token","accessTokenExpiresAt":2000000000000,"refreshTokenExpiresAt":2100000000000,"sessionId":"session-retained","deviceId":"device-retained","username":"user@example.com","userId":"user-retained"}"""
        store.writeSessionJson(session)
        val encrypted = ciphertext.readBytes()

        assertTrue(ciphertext.delete())
        assertTrue(ciphertext.mkdir())
        File(ciphertext, "keep").writeText("force-delete-failure")

        val failure = runCatching { store.clear() }.exceptionOrNull()
        assertTrue(failure is IllegalStateException)

        assertTrue(ciphertext.deleteRecursively())
        ciphertext.writeBytes(encrypted)

        // The ciphertext survived the simulated deletion failure, but clear() destroyed the
        // Keystore key before reporting failure. A cold-start store must therefore reject it.
        assertNull(AndroidAccountSessionStore(context).readSessionJson())
        assertFalse(ciphertext.exists())
    }
}
