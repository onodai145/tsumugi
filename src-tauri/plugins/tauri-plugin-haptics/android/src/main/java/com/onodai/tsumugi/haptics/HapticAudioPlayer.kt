package com.onodai.tsumugi.haptics

import android.media.AudioAttributes
import android.media.MediaDataSource
import android.media.MediaPlayer
import android.os.Build
import android.os.Handler
import android.os.Looper

/**
 * 触覚チャンネル入りの音声(ogg)を MediaPlayer で再生する(Issue #592)。
 *
 * - 最新の 1 つだけを保持する(後勝ち)。新しい要求は、再生中のものを止めてから始める。
 * - MediaPlayer の生成・操作・コールバックは、すべてメインスレッドで行う。
 * - onResult は、要求ごとに必ず 1 回だけ呼ぶ: 再生が始まった(または後続の要求に置き換えられた)ら null、
 *   失敗(準備エラー・タイムアウト・API 28 以下)なら例外。呼び出し側(Rust)をブロックしたままにしないため。
 */
internal class HapticAudioPlayer {
    private val main = Handler(Looper.getMainLooper())
    private var current: Session? = null // メインスレッドからだけ触る

    fun play(data: ByteArray, hapticsEnabled: Boolean, onResult: (Throwable?) -> Unit) {
        main.post { start(data, hapticsEnabled, onResult) }
    }

    private fun start(data: ByteArray, hapticsEnabled: Boolean, onResult: (Throwable?) -> Unit) {
        current?.release() // 後勝ち。置き換えられた要求は成功として終える
        current = null
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
            // setHapticChannelsMuted は API 29(Android 10)以降
            onResult(UnsupportedOperationException("haptic channels need API 29 or later"))
            return
        }
        val session = Session(data, hapticsEnabled, onResult)
        current = session
        session.begin()
    }

    private inner class Session(
        private val data: ByteArray,
        private val hapticsEnabled: Boolean,
        private val onResult: (Throwable?) -> Unit,
    ) {
        private val player = MediaPlayer()
        private var settled = false
        private var released = false
        private val timeout = Runnable {
            settle(IllegalStateException("MediaPlayer did not become ready within ${PREPARE_TIMEOUT_MS}ms"))
            release()
        }

        fun begin() {
            try {
                player.setAudioAttributes(
                    AudioAttributes.Builder()
                        .setUsage(AudioAttributes.USAGE_NOTIFICATION)
                        .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                        .setHapticChannelsMuted(!hapticsEnabled)
                        .build()
                )
                player.setDataSource(BytesDataSource(data))
                player.setOnPreparedListener {
                    if (!released) {
                        it.start()
                        settle(null)
                    }
                }
                player.setOnCompletionListener { release() }
                player.setOnErrorListener { _, what, extra ->
                    settle(IllegalStateException("MediaPlayer error what=$what extra=$extra"))
                    release()
                    true
                }
                main.postDelayed(timeout, PREPARE_TIMEOUT_MS)
                player.prepareAsync()
            } catch (e: Exception) {
                settle(e)
                release()
            }
        }

        /** 結果を 1 回だけ返す。2 回目以降は何もしない。 */
        private fun settle(error: Throwable?) {
            if (settled) return
            settled = true
            main.removeCallbacks(timeout)
            onResult(error)
        }

        fun release() {
            if (released) return
            released = true
            settle(null) // まだ結果を返していなければ(置き換えられたとき)成功として終える
            main.removeCallbacks(timeout)
            try {
                player.stop()
            } catch (_: IllegalStateException) {
                // 準備前などで stop できない状態。release だけすればよい
            }
            player.release()
            if (current === this) current = null
        }
    }

    /** メモリ上のバイト列を MediaPlayer に渡す。Context が要らず、ファイルも作らない。 */
    private class BytesDataSource(private val data: ByteArray) : MediaDataSource() {
        override fun readAt(position: Long, buffer: ByteArray, offset: Int, size: Int): Int {
            if (position >= data.size) return -1
            val n = minOf(size.toLong(), data.size - position).toInt()
            System.arraycopy(data, position.toInt(), buffer, offset, n)
            return n
        }

        override fun getSize(): Long = data.size.toLong()

        override fun close() {}
    }

    private companion object {
        const val PREPARE_TIMEOUT_MS = 5_000L
    }
}
