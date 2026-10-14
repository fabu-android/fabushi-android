package com.ombhrum.fabushi.androidmain.security

import java.io.File
import java.nio.file.Files
import org.junit.After
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AccountSessionRevocationFenceTest {
    private val root = Files.createTempDirectory("fabushi-account-revocation-fence").toFile()
    private val marker = File(root, "revoked")

    @After
    fun cleanup() {
        root.deleteRecursively()
    }

    @Test
    fun markerIsDurableIdempotentAndExplicitlyCleared() {
        val fence = AccountSessionRevocationFence(marker)
        assertFalse(fence.isRevoked())

        fence.markRevoked()
        fence.markRevoked()

        assertTrue(fence.isRevoked())
        assertTrue(AccountSessionRevocationFence(marker).isRevoked())

        fence.clear()

        assertFalse(fence.isRevoked())
        assertFalse(AccountSessionRevocationFence(marker).isRevoked())
    }
}
