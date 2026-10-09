package com.ombhrum.fabushi

import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.speech.RecognitionListener
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import java.util.Locale
import java.util.UUID
import java.util.concurrent.atomic.AtomicLong
import java.util.concurrent.atomic.AtomicReference

/**
 * On-device-only speech transcription. It never constructs the ordinary SpeechRecognizer,
 * therefore there is no network/cloud fallback. Unsupported devices fail closed.
 */
internal class NativeOfflineSpeechTranscriber(private val context: Context) {
    private val ownerId = "offline-asr:${UUID.randomUUID()}"
    private val generation = AtomicLong(0)
    private val activeRecognizer = AtomicReference<SpeechRecognizer?>(null)

    fun isAvailable(): Boolean =
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.S &&
            SpeechRecognizer.isOnDeviceRecognitionAvailable(context)

    fun start(
        locale: Locale = Locale.getDefault(),
        onResult: (Result<String>) -> Unit,
    ): Result<Unit> = runCatching {
        check(Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            "此设备的 Android 版本不支持系统离线语音识别"
        }
        check(SpeechRecognizer.isOnDeviceRecognitionAvailable(context)) {
            "此设备未提供可用的系统离线语音识别模型"
        }
        check(activeRecognizer.get() == null) { "离线语音识别已经开始" }
        check(MicrophoneLease.acquire(ownerId)) { "麦克风正在被其他 Fabushi 录音功能使用" }

        val currentGeneration = generation.incrementAndGet()
        val recognizer = try {
            SpeechRecognizer.createOnDeviceSpeechRecognizer(context)
        } catch (error: Throwable) {
            MicrophoneLease.release(ownerId)
            throw error
        }
        activeRecognizer.set(recognizer)
        val delivered = AtomicReference(false)

        fun finish(result: Result<String>) {
            if (generation.get() != currentGeneration) return
            if (!delivered.compareAndSet(false, true)) return
            activeRecognizer.compareAndSet(recognizer, null)
            runCatching { recognizer.destroy() }
            MicrophoneLease.release(ownerId)
            onResult(result)
        }

        recognizer.setRecognitionListener(object : RecognitionListener {
            override fun onReadyForSpeech(params: Bundle?) = Unit
            override fun onBeginningOfSpeech() = Unit
            override fun onRmsChanged(rmsdB: Float) = Unit
            override fun onBufferReceived(buffer: ByteArray?) = Unit
            override fun onEndOfSpeech() = Unit
            override fun onPartialResults(partialResults: Bundle?) = Unit
            override fun onEvent(eventType: Int, params: Bundle?) = Unit

            override fun onError(error: Int) {
                finish(Result.failure(IllegalStateException(errorMessage(error))))
            }

            override fun onResults(results: Bundle?) {
                val text = results
                    ?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)
                    ?.firstOrNull { it.isNotBlank() }
                    ?.trim()
                if (text.isNullOrBlank()) {
                    finish(Result.failure(IllegalStateException("离线语音识别没有返回文本")))
                } else {
                    finish(Result.success(text))
                }
            }
        })

        val intent = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH)
            .putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
            .putExtra(RecognizerIntent.EXTRA_LANGUAGE, locale.toLanguageTag())
            .putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, false)
            .putExtra(RecognizerIntent.EXTRA_PREFER_OFFLINE, true)
            .putExtra(RecognizerIntent.EXTRA_MAX_RESULTS, 3)
        recognizer.startListening(intent)
    }.onFailure {
        activeRecognizer.getAndSet(null)?.let { recognizer -> runCatching { recognizer.destroy() } }
        MicrophoneLease.release(ownerId)
    }

    fun cancel() {
        generation.incrementAndGet()
        activeRecognizer.getAndSet(null)?.let { recognizer ->
            runCatching { recognizer.cancel() }
            runCatching { recognizer.destroy() }
        }
        MicrophoneLease.release(ownerId)
    }

    private fun errorMessage(error: Int): String = when (error) {
        SpeechRecognizer.ERROR_AUDIO -> "离线语音识别无法读取麦克风"
        SpeechRecognizer.ERROR_INSUFFICIENT_PERMISSIONS -> "离线语音识别缺少麦克风权限"
        SpeechRecognizer.ERROR_NO_MATCH -> "没有识别到可转写的语音"
        SpeechRecognizer.ERROR_RECOGNIZER_BUSY -> "离线语音识别服务正忙"
        SpeechRecognizer.ERROR_SPEECH_TIMEOUT -> "没有检测到语音"
        SpeechRecognizer.ERROR_LANGUAGE_NOT_SUPPORTED -> "当前语言不支持离线识别"
        SpeechRecognizer.ERROR_LANGUAGE_UNAVAILABLE -> "当前语言的离线模型不可用"
        else -> "离线语音识别失败（$error）"
    }
}
