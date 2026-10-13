package com.ombhrum.fabushi.androidmain.media

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.ComponentActivity
import androidx.core.content.ContextCompat
import com.ombhrum.fabushi.androidpreload.media.AndroidHumanCallMediaPermissionProjection
import com.ombhrum.fabushi.androidpreload.media.projectAndroidHumanCallMediaPermissions
import java.lang.ref.WeakReference

/**
 * Process-scoped owner for the one outstanding Android Human Call media permission request.
 *
 * The Activity supplies only the framework permission prompt/result. Product call state and media
 * lifetime remain below the typed runtime, and microphone resource ownership remains with
 * MicrophoneLease once a call actually starts capturing.
 */
internal class AndroidHumanCallMediaPermissionOwner {
    private data class Pending(
        val activity: WeakReference<ComponentActivity>,
        val audioRequested: Boolean,
        val videoRequested: Boolean,
        val listener: (AndroidHumanCallMediaPermissionProjection) -> Unit,
    )

    @Volatile
    private var pending: Pending? = null

    @Synchronized
    fun request(
        activity: ComponentActivity,
        audioRequested: Boolean,
        videoRequested: Boolean,
        listener: (AndroidHumanCallMediaPermissionProjection) -> Unit,
    ): Boolean {
        if (!audioRequested && !videoRequested) {
            listener(
                projectAndroidHumanCallMediaPermissions(
                    audioRequested = false,
                    videoRequested = false,
                    microphoneGranted = false,
                    cameraGranted = false,
                    afterSystemRequest = false,
                ),
            )
            return true
        }
        if (pending != null) return false

        val microphoneGranted =
            ContextCompat.checkSelfPermission(activity, Manifest.permission.RECORD_AUDIO) ==
                PackageManager.PERMISSION_GRANTED
        val cameraGranted =
            ContextCompat.checkSelfPermission(activity, Manifest.permission.CAMERA) ==
                PackageManager.PERMISSION_GRANTED
        val projection = projectAndroidHumanCallMediaPermissions(
            audioRequested = audioRequested,
            videoRequested = videoRequested,
            microphoneGranted = microphoneGranted,
            cameraGranted = cameraGranted,
            afterSystemRequest = false,
        )
        val permissions = buildList {
            if (audioRequested && !microphoneGranted) add(Manifest.permission.RECORD_AUDIO)
            if (videoRequested && !cameraGranted) add(Manifest.permission.CAMERA)
        }
        if (permissions.isEmpty()) {
            listener(projection)
            return true
        }

        pending = Pending(
            activity = WeakReference(activity),
            audioRequested = audioRequested,
            videoRequested = videoRequested,
            listener = listener,
        )
        return runCatching {
            activity.requestPermissions(permissions.toTypedArray(), REQUEST_CODE)
            true
        }.getOrElse {
            pending = null
            false
        }
    }

    @Synchronized
    fun handleResult(
        activity: ComponentActivity,
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray,
    ): Boolean {
        if (requestCode != REQUEST_CODE) return false
        val active = pending ?: return false
        if (active.activity.get() !== activity) {
            pending = null
            return true
        }
        val grants = permissions.indices.associate { index ->
            permissions[index] to (grantResults.getOrNull(index) == PackageManager.PERMISSION_GRANTED)
        }
        pending = null
        active.listener(
            projectAndroidHumanCallMediaPermissions(
                audioRequested = active.audioRequested,
                videoRequested = active.videoRequested,
                microphoneGranted = if (active.audioRequested) {
                    grants[Manifest.permission.RECORD_AUDIO]
                        ?: (ContextCompat.checkSelfPermission(
                            activity,
                            Manifest.permission.RECORD_AUDIO,
                        ) == PackageManager.PERMISSION_GRANTED)
                } else {
                    false
                },
                cameraGranted = if (active.videoRequested) {
                    grants[Manifest.permission.CAMERA]
                        ?: (ContextCompat.checkSelfPermission(
                            activity,
                            Manifest.permission.CAMERA,
                        ) == PackageManager.PERMISSION_GRANTED)
                } else {
                    false
                },
                afterSystemRequest = true,
            ),
        )
        return true
    }

    @Synchronized
    fun detach(activity: ComponentActivity) {
        if (pending?.activity?.get() === activity) pending = null
    }

    @Synchronized
    fun close() {
        pending = null
    }

    private companion object {
        const val REQUEST_CODE = 4113
    }
}
