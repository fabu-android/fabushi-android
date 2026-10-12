package com.ombhrum.fabushi.androidmain.coordinator
sealed interface CoordinatorAccountState {
    data object SignedOut : CoordinatorAccountState
    data class Authenticating(val attemptId: String) : CoordinatorAccountState
    data class Ready(val accountId: String) : CoordinatorAccountState
    data class Refused(val accountId: String, val reason: String) : CoordinatorAccountState
}
class CoordinatorAccountRuntime {
    var state: CoordinatorAccountState = CoordinatorAccountState.SignedOut
        private set
    fun begin(attemptId: String) { require(attemptId.isNotBlank()); state = CoordinatorAccountState.Authenticating(attemptId) }
    fun ready(accountId: String) { require(accountId.isNotBlank()); state = CoordinatorAccountState.Ready(accountId) }
    fun refuse(accountId: String, reason: String) { state = CoordinatorAccountState.Refused(accountId, reason) }
    fun signOut() { state = CoordinatorAccountState.SignedOut }
}
