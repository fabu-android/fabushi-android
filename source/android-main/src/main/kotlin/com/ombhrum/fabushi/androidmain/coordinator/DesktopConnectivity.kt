package com.ombhrum.fabushi.androidmain.coordinator
data class ConnectivityStamps(val lastNetworkAvailableAtMs: Long?, val lastWakeAtMs: Long?)
class AndroidConnectivity {
    private var networkAt: Long? = null
    private var wakeAt: Long? = null
    fun onNetworkAvailable(nowMs: Long) { networkAt = nowMs }
    fun onProcessForeground(nowMs: Long) { wakeAt = nowMs }
    fun stamps(): ConnectivityStamps = ConnectivityStamps(networkAt, wakeAt)
    fun recentlyRecovered(nowMs: Long, windowMs: Long = 10_000): Boolean =
        listOfNotNull(networkAt, wakeAt).any { nowMs - it in 0..windowMs }
}
