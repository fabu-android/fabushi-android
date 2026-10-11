package com.ombhrum.fabushi.androidmain.coordinator
object CoordinatorPortIpcGuard {
    const val ANDROID_APP_PACKAGE = "com.ombhrum.fabushi"
    fun isTrustedRequester(packageName: String?, sameProcess: Boolean): Boolean = sameProcess && packageName == ANDROID_APP_PACKAGE
    fun requireTrusted(packageName: String?, sameProcess: Boolean) {
        check(isTrustedRequester(packageName, sameProcess)) { "untrusted coordinator port requester" }
    }
}
