package com.ombhrum.fabushi

import android.content.Context
import android.media.MediaPlayer
import java.io.File

internal class NativeVoicePlayer(private val context: Context) {
    private var player: MediaPlayer? = null
    private var file: File? = null
    var playingMessageId: String? = null
        private set

    fun toggle(messageId: String, bytes: ByteArray, onFinished: () -> Unit = {}): Result<Boolean> = runCatching {
        if (playingMessageId == messageId) {
            stop()
            onFinished()
            return@runCatching false
        }
        stop()
        val directory = File(context.cacheDir, "fabushi-voice-playback").apply { mkdirs() }
        val local = File(directory, "voice-$messageId.m4a")
        local.writeBytes(bytes)
        val mediaPlayer = MediaPlayer().apply {
            setDataSource(local.absolutePath)
            setOnCompletionListener { stop(); onFinished() }
            setOnErrorListener { _, _, _ -> stop(); onFinished(); true }
            prepare()
            start()
        }
        file = local
        player = mediaPlayer
        playingMessageId = messageId
        true
    }

    fun stop() {
        runCatching { player?.stop() }
        runCatching { player?.release() }
        player = null
        file?.delete()
        file = null
        playingMessageId = null
    }
}
