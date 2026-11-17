package com.onodai.tsumugi.haptics

import android.app.Activity
import android.content.Context
import android.os.Build
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import android.util.Base64
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

@InvokeArg
class VibrateArgs {
    var pattern: String? = null
}

@InvokeArg
class PlayHapticAudioArgs {
    var data: String? = null
    var hapticsEnabled: Boolean = true
}

@TauriPlugin
class HapticsPlugin(private val activity: Activity) : Plugin(activity) {
    private val hapticAudio = HapticAudioPlayer()

    private val vibrator: Vibrator by lazy {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            val manager = activity.getSystemService(Context.VIBRATOR_MANAGER_SERVICE) as VibratorManager
            manager.defaultVibrator
        } else {
            @Suppress("DEPRECATION")
            activity.getSystemService(Context.VIBRATOR_SERVICE) as Vibrator
        }
    }

    // duration(ms)は単発、timings配列は[待ち, ON, OFF, ON, ...]のパルス列(createWaveform)。
    // 値はRust側 HapticPattern のドキュメントコメント(models.rs)と対応させること。
    private fun effectFor(pattern: String?): VibrationEffect = when (pattern) {
        "light" -> VibrationEffect.createOneShot(10, VibrationEffect.DEFAULT_AMPLITUDE)
        "success" -> VibrationEffect.createWaveform(longArrayOf(0, 15, 40, 15), -1)
        "warning" -> VibrationEffect.createWaveform(longArrayOf(0, 25, 80, 25), -1)
        "error" -> VibrationEffect.createOneShot(60, VibrationEffect.DEFAULT_AMPLITUDE)
        else -> VibrationEffect.createOneShot(35, VibrationEffect.DEFAULT_AMPLITUDE) // "medium" とその他既定
    }

    @Command
    fun vibrate(invoke: Invoke) {
        val args = invoke.parseArgs(VibrateArgs::class.java)
        vibrator.vibrate(effectFor(args.pattern))
        invoke.resolve()
    }

    /// 触覚入りの音声を MediaPlayer で再生する(Issue #592)。再生が始まったら resolve、失敗したら reject。
    @Command
    fun playHapticAudio(invoke: Invoke) {
        val args = invoke.parseArgs(PlayHapticAudioArgs::class.java)
        val bytes = try {
            Base64.decode(args.data ?: "", Base64.DEFAULT)
        } catch (e: IllegalArgumentException) {
            invoke.reject("invalid base64 audio data")
            return
        }
        if (bytes.isEmpty()) {
            invoke.reject("empty audio data")
            return
        }
        hapticAudio.play(bytes, args.hapticsEnabled) { error ->
            if (error == null) invoke.resolve() else invoke.reject(error.message ?: "haptic audio playback failed")
        }
    }
}
