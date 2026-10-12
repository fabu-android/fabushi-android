package com.ombhrum.fabushi

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class RootErrorBoundaryProjectionTest {
    @Test
    fun diagnosticsAreBoundedAndRedactCredentialLikeValues() {
        val error = IllegalStateException("token=abc123 password: hunter2 authorization=BearerSecret")
        val failure = projectFabushiRootFailure(error)
        assertTrue(failure.title.isNotBlank())
        assertTrue(failure.detail.contains("<redacted>"))
        assertFalse(failure.diagnostics.contains("abc123"))
        assertFalse(failure.diagnostics.contains("hunter2"))
        assertFalse(failure.diagnostics.contains("BearerSecret"))
        assertTrue(failure.diagnostics.length <= 8_192)
    }

    @Test
    fun emptyMessageStillProducesActionableDetail() {
        val failure = projectFabushiRootFailure(RuntimeException())
        assertTrue(failure.detail.contains("could not start"))
    }
}
