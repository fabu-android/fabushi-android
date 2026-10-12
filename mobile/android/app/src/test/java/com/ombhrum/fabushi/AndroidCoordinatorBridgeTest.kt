package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.runtime.AndroidCoordinatorBridge
import com.ombhrum.fabushi.androidpreload.runtime.AndroidCoordinatorPort
import java.lang.reflect.Proxy
import java.util.Collections
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidCoordinatorBridgeTest {
    private fun fakePort(): AndroidCoordinatorPort =
        Proxy.newProxyInstance(
            AndroidCoordinatorPort::class.java.classLoader,
            arrayOf(AndroidCoordinatorPort::class.java),
        ) { _, method, _ ->
            when {
                method.returnType == java.lang.Boolean.TYPE -> false
                method.returnType == java.lang.Integer.TYPE -> 0
                method.returnType == java.lang.Long.TYPE -> 0L
                else -> null
            }
        } as AndroidCoordinatorPort

    @Test
    fun concurrentInstallKeepsExactlyOneProcessOwner() {
        val first = fakePort()
        val second = fakePort()
        val start = CountDownLatch(1)
        val done = CountDownLatch(2)
        val successes = Collections.synchronizedList(mutableListOf<AndroidCoordinatorPort>())

        listOf(first, second).forEach { candidate ->
            Thread {
                start.await()
                if (runCatching { AndroidCoordinatorBridge.installTrustedRuntime(candidate) }.isSuccess) {
                    successes += candidate
                }
                done.countDown()
            }.start()
        }

        start.countDown()
        assertTrue(done.await(5, TimeUnit.SECONDS))
        assertEquals(1, successes.size)
        val winner = successes.single()
        assertSame(winner, AndroidCoordinatorBridge.presentation())
        AndroidCoordinatorBridge.clearTrustedRuntime(winner)
    }
}
