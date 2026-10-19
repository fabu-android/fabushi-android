package com.ombhrum.fabushi

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CredentialVaultProjectionTest {
    @Test
    fun projectionAcceptsOnlyCurrentGenerationAndCurrentSignedInAccount() {
        assertTrue(
            shouldApplyCredentialVaultResult(
                requestGeneration = 7,
                currentGeneration = 7,
                expectedAuthId = "account-a",
                currentLoggedIn = true,
                currentAuthId = "account-a",
            ),
        )
        assertFalse(
            shouldApplyCredentialVaultResult(
                requestGeneration = 6,
                currentGeneration = 7,
                expectedAuthId = "account-a",
                currentLoggedIn = true,
                currentAuthId = "account-a",
            ),
        )
        assertFalse(
            shouldApplyCredentialVaultResult(
                requestGeneration = 7,
                currentGeneration = 7,
                expectedAuthId = "account-a",
                currentLoggedIn = true,
                currentAuthId = "account-b",
            ),
        )
        assertFalse(
            shouldApplyCredentialVaultResult(
                requestGeneration = 7,
                currentGeneration = 7,
                expectedAuthId = "account-a",
                currentLoggedIn = false,
                currentAuthId = "account-a",
            ),
        )
    }
}
