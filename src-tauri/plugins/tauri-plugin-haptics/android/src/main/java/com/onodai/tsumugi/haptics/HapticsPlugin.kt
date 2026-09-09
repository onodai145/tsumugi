package com.onodai.tsumugi.haptics

import android.app.Activity
import android.content.Context
import android.os.Build
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

@InvokeArg
class VibrateArgs {
    var pattern: String? = null
}

@TauriPlugin
class HapticsPlugin(private val activity: Activity) : Plugin(activity) {
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
}
