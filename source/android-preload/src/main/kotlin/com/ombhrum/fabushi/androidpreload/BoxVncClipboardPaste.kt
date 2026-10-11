package com.ombhrum.fabushi.androidpreload
object BoxVncClipboardPaste {
    fun normalizeForRemote(text: String): String = text.replace("\r\n", "\n").replace("\r", "\n")
    fun shouldSync(local: String?, remote: String?): Boolean = local != null && normalizeForRemote(local) != remote
}
