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
    @Test
    fun avatarByteCacheCopiesValuesAndEvictsLeastRecentlyUsedEntry() {
        val cache = AccountAvatarByteCache(maxEntries = 2)
        val first = byteArrayOf(1, 2, 3)
        cache.put("https://example.com/first.png", first)
        first[0] = 9

        val cached = cache.get("https://example.com/first.png")!!
        assertEquals(listOf<Byte>(1, 2, 3), cached.toList())
        cached[1] = 8
        assertEquals(listOf<Byte>(1, 2, 3), cache.get("https://example.com/first.png")!!.toList())

        cache.put("https://example.com/second.png", byteArrayOf(4))
        cache.get("https://example.com/first.png")
        cache.put("https://example.com/third.png", byteArrayOf(5))

        assertEquals(2, cache.size())
        assertNull(cache.get("https://example.com/second.png"))
        assertEquals(listOf<Byte>(1, 2, 3), cache.get("https://example.com/first.png")!!.toList())
    }

    @Test(expected = IllegalArgumentException::class)
    fun avatarByteCacheRejectsUnboundedPayloads() {
        AccountAvatarByteCache(maxEntries = 1)
            .put("https://example.com/too-large.png", ByteArray(ACCOUNT_AVATAR_MAX_BYTES + 1))
    }

}
