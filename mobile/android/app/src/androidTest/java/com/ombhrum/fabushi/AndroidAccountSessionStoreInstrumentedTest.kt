package com.ombhrum.fabushi

import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.ombhrum.fabushi.androidmain.security.AccountSessionRevocationFence
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
    private val revocationMarker =
        File(context.noBackupFilesDir, "fabushi-account-session.revoked.v1")

    @After
    fun cleanup() {
        runCatching { store.clear() }
        ciphertext.deleteRecursively()
        revocationMarker.deleteRecursively()
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
        // Keystore key and persisted the revocation fence before reporting failure. Cold start
        // therefore refuses the old credential without needing to decrypt or mutate it.
        assertNull(AndroidAccountSessionStore(context).readSessionJson())
        assertTrue(revocationMarker.exists())
        assertTrue(ciphertext.exists())
    }

    @Test
    fun durableRevocationFenceBlocksColdStartUntilReplacementSessionPersists() {
        val oldSession = """{"accessToken":"old-access","refreshToken":"old-refresh","accessTokenExpiresAt":2000000000000,"refreshTokenExpiresAt":2100000000000,"sessionId":"session-old","deviceId":"device-old","username":"user@example.com","userId":"user-old"}"""
        val replacementSession = """{"accessToken":"new-access","refreshToken":"new-refresh","accessTokenExpiresAt":2200000000000,"refreshTokenExpiresAt":2300000000000,"sessionId":"session-new","deviceId":"device-new","username":"user@example.com","userId":"user-new"}"""
        store.writeSessionJson(oldSession)

        AccountSessionRevocationFence(revocationMarker).markRevoked()

        assertNull(AndroidAccountSessionStore(context).readSessionJson())
        assertTrue(ciphertext.exists())
        assertTrue(revocationMarker.exists())

        store.writeSessionJson(replacementSession)

        assertFalse(revocationMarker.exists())
        assertEquals(replacementSession, AndroidAccountSessionStore(context).readSessionJson())
    }
}
