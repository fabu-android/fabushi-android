package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Test

class SigninBootGateProjectionTest {
    @Test
    fun bootGateUsesCanonicalAuthAndOnboardingState() {
        assertEquals("sign-in", signinBootGate(loggedIn = false, onboardingStep = 0))
        assertEquals("sign-in", signinBootGate(loggedIn = false, onboardingStep = 3))
        assertEquals("onboarding", signinBootGate(loggedIn = true, onboardingStep = 0))
        assertEquals("onboarding", signinBootGate(loggedIn = true, onboardingStep = 2))
        assertEquals("shell", signinBootGate(loggedIn = true, onboardingStep = 3))
        assertEquals("shell", signinBootGate(loggedIn = true, onboardingStep = 99))
    }
}
