package com.ombhrum.fabushi

import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.ombhrum.fabushi.androidmain.security.AndroidCredentialVaultStore
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
class AndroidCredentialVaultStoreInstrumentedTest {
    private val context = ApplicationProvider.getApplicationContext<android.content.Context>()
    private val vaultFile = File(context.noBackupFilesDir, "fabushi-credential-vault.v1")

    @Before fun reset() { vaultFile.delete() }
    @After fun cleanup() { vaultFile.delete() }

    @Test fun metadataNeverRevealsPlaintextAndRotateKeepsStableIdentity() {
        var now = 10L
        val store = AndroidCredentialVaultStore(context) { now }
        val first = store.upsert("acct-a", "connector/prod", "Production", "canary-first", listOf("https://api.example.com"))
        assertTrue(first.getBoolean("configured")); assertFalse(first.getBoolean("revealable"))
        assertFalse(first.toString().contains("canary-first")); assertFalse(vaultFile.readText().contains("canary-first"))
        assertEquals("canary-first", store.resolveForOrigin("acct-a", "connector/prod", "https://api.example.com"))
        now = 20L
        val rotated = store.rotate("acct-a", "connector/prod", "Production", "canary-rotated", listOf("https://api.example.com"))
        assertEquals(first.getLong("createdAtMs"), rotated.getLong("createdAtMs"))
        assertTrue(rotated.getLong("updatedAtMs") >= first.getLong("createdAtMs"))
        assertFalse(store.listMetadata("acct-a").toString().contains("canary-rotated")); assertFalse(vaultFile.readText().contains("canary-rotated"))
    }

    @Test fun accountAndOriginFencesFailClosedAndRevokeRemovesMetadata() {
        val store = AndroidCredentialVaultStore(context)
        store.upsert("acct-a", "connector/prod", "", "secret-a", listOf("https://api.example.com"))
        assertEquals(0, store.listMetadata("acct-b").length())
        assertThrows(IllegalStateException::class.java) { store.resolveForOrigin("acct-a", "connector/prod", "https://other.example.com") }
        assertThrows(IllegalArgumentException::class.java) { store.upsert("acct-a", "bad", "", "secret", listOf("http://example.com")) }
        assertTrue(store.revoke("acct-a", "connector/prod")); assertEquals(0, store.listMetadata("acct-a").length()); assertFalse(store.revoke("acct-a", "connector/prod"))
    }

    @Test fun corruptCiphertextFailsClosedInsteadOfResettingTheVault() {
        vaultFile.parentFile?.mkdirs(); vaultFile.writeText("fabushi-credential-vault-v1\nnot-base64\nnot-base64")
        assertThrows(IllegalStateException::class.java) { AndroidCredentialVaultStore(context).listMetadata("acct-a") }
        assertTrue(vaultFile.exists())
    }
}
