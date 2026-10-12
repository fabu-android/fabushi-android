package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class AccountAvatarPolicyTest {
    @Test
    fun preferredHttpsAvatarPrecedesGithubFallback() {
        assertEquals(
            listOf(
                "https://cdn.example.com/avatar.png",
                "https://avatars.githubusercontent.com/u/12345?v=4&s=192",
            ),
            accountAvatarCandidateUrls(
                authId = "github|12345",
                preferredUrl = " https://cdn.example.com/avatar.png ",
            ),
        )
    }

    @Test
    fun githubIdentityProducesBoundedFallbackCandidate() {
        assertEquals(
            "https://avatars.githubusercontent.com/u/42?v=4&s=192",
            githubAccountAvatarUrl("github|42"),
        )
        assertNull(githubAccountAvatarUrl("github|not-a-number"))
        assertNull(githubAccountAvatarUrl("google|42"))
    }

    @Test
    fun unsafePreferredAvatarUrlsAreRejected() {
        assertEquals(
            emptyList<String>(),
            accountAvatarCandidateUrls(
                authId = null,
                preferredUrl = "http://example.com/avatar.png",
            ),
        )
        assertEquals(
            emptyList<String>(),
            accountAvatarCandidateUrls(
                authId = null,
                preferredUrl = "https://user:password@example.com/avatar.png",
            ),
        )
    }
}
