package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.deeplink.AndroidDeepLink
import com.ombhrum.fabushi.androidpreload.deeplink.AndroidDeepLinkSource

internal enum class DeepLinkSource {
    PROTOCOL,
    HTTPS,
}

internal data class DeepLinkInfo(
    val version: Int = 1,
    val source: DeepLinkSource,
    val topic: String = "deep-links",
)

internal fun AndroidDeepLink.Info.toDeepLinkInfo(): DeepLinkInfo =
    DeepLinkInfo(
        source = when (source) {
            AndroidDeepLinkSource.PROTOCOL -> DeepLinkSource.PROTOCOL
            AndroidDeepLinkSource.HTTPS -> DeepLinkSource.HTTPS
        },
        topic = topic,
    )

internal fun deepLinkRoute(link: DeepLinkInfo): String =
    "fabushi://app/v1/info?topic=" + link.topic

internal fun deepLinkSourceLabel(source: DeepLinkSource): String =
    when (source) {
        DeepLinkSource.PROTOCOL -> "Custom protocol (fabushi://)"
        DeepLinkSource.HTTPS -> "HTTPS link"
    }
