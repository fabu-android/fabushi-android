import org.json.JSONObject
package com.ombhrum.fabushi.androidpreload
import com.ombhrum.fabushi.androidpreload.runtime.AndroidCoordinatorPort
class CoordinatorPortBridge(private val port: AndroidCoordinatorPort) {
    fun listApprovedMiniApps(): JSONObject = port.listApprovedMiniApps()
    fun listInstalledMiniApps(): JSONObject = port.listInstalledMiniApps()
    fun installApprovedMiniApp(appId: String, operationId: String): JSONObject = port.installApprovedMiniApp(appId, operationId)
    fun getInstalledMiniAppResources(appId: String, operationId: String): JSONObject = port.getInstalledMiniAppResources(appId, operationId)
    fun readMiniAppResource(loadId: String, resourcePath: String): JSONObject = port.readMiniAppResource(loadId, resourcePath)
    fun disposeMiniAppLoad(loadId: String): JSONObject = port.disposeMiniAppLoad(loadId)
    fun uninstallMiniApp(appId: String, operationId: String): JSONObject = port.uninstallMiniApp(appId, operationId)
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
