package com.ombhrum.fabushi.androidmain.coordinator
enum class CoordinatorExitClass { Clean, RequestedRestart, Crash, ProtocolFailure }
fun classifyCoordinatorExit(code: Int, requested: Boolean): CoordinatorExitClass = when {
    requested -> CoordinatorExitClass.RequestedRestart
    code == 0 -> CoordinatorExitClass.Clean
    code == 64 -> CoordinatorExitClass.ProtocolFailure
    else -> CoordinatorExitClass.Crash
}
fun coordinatorRelaunchDelayMs(failures: Int): Long {
    val exponent = failures.coerceIn(0, 6)
    return (250L shl exponent).coerceAtMost(16_000L)
}
