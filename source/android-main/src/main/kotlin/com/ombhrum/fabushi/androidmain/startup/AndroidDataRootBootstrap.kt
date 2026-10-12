package com.ombhrum.fabushi.androidmain.startup

import android.content.Context
import java.io.File
import java.io.FileOutputStream
import java.io.IOException

internal const val ANDROID_DATA_ROOT_MARKER = ".fabushi-android-data-root-v1"
private const val ANDROID_DATA_ROOT_MARKER_BODY = "{\"version\":1}\n"

/**
 * Settles the canonical Android private data root before the Rust Host opens durable state.
 *
 * Android owns application placement and sandbox paths, so Desktop's home-directory renames and
 * PID probing do not apply. The product responsibility still applies: never start a canonical Host
 * on a file/symlink/foreign marker, and never expose a partially written migration marker.
 */
internal object AndroidDataRootBootstrap {
    fun settle(context: Context): File = settleRoot(context.filesDir)

    internal fun settleRoot(root: File): File {
        val absolute = root.absoluteFile
        if (!absolute.exists() && !absolute.mkdirs()) {
            throw IOException("Unable to create Fabushi Android data root")
        }
        require(absolute.isDirectory) { "Fabushi Android data root is not a directory" }

        val canonical = absolute.canonicalFile
        require(canonical == absolute) { "Fabushi Android data root must not be a symbolic link" }

        val marker = File(absolute, ANDROID_DATA_ROOT_MARKER)
        if (marker.exists()) {
            require(marker.isFile) { "Fabushi Android data-root marker is not a file" }
            require(marker.readText(Charsets.UTF_8) == ANDROID_DATA_ROOT_MARKER_BODY) {
                "Fabushi Android data-root marker has an unsupported version"
            }
            return absolute
        }

        val temporary = File(absolute, "$ANDROID_DATA_ROOT_MARKER.tmp-${android.os.Process.myPid()}")
        runCatching {
            FileOutputStream(temporary).use { output ->
                output.write(ANDROID_DATA_ROOT_MARKER_BODY.toByteArray(Charsets.UTF_8))
                output.fd.sync()
            }
            if (!temporary.renameTo(marker)) {
                // A peer in the same app process generation may have settled the root first.
                require(marker.isFile && marker.readText(Charsets.UTF_8) == ANDROID_DATA_ROOT_MARKER_BODY) {
                    "Unable to atomically settle Fabushi Android data root"
                }
            }
        }.onFailure {
            temporary.delete()
            throw it
        }
        temporary.delete()
        require(marker.isFile && marker.readText(Charsets.UTF_8) == ANDROID_DATA_ROOT_MARKER_BODY) {
            "Fabushi Android data-root settlement did not persist"
        }
        return absolute
    }
}
