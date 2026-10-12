package com.ombhrum.fabushi.androidpreload
import com.ombhrum.fabushi.androidpreload.runtime.AndroidCoordinatorPort
class CoordinatorPortBridge(private val port: AndroidCoordinatorPort) {
    private var claimedBy: String? = null
    @Synchronized fun claim(owner: String): AndroidCoordinatorPort {
        require(owner.isNotBlank()) { "coordinator port owner must not be blank" }
        check(claimedBy == null || claimedBy == owner) { "coordinator port already claimed by another owner" }
        claimedBy = owner
        return port
    }
    @Synchronized fun release(owner: String) { if (claimedBy == owner) claimedBy = null }
    @Synchronized fun currentOwner(): String? = claimedBy
}
