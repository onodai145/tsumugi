package com.onodai.tsumugi

import android.content.Intent
import android.database.Cursor
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.OpenableColumns
import android.util.Log
import io.crates.keyring.Keyring
import java.io.File

class MainActivity : TauriActivity() {
    /// Rust側(mobile_intent.rs)へ共有インテントの内容を渡す(Issue #116)。
    /// `libtsumugi_lib.so` は generated/Rust.kt が既にロード済みなので追加ロード不要。
    private external fun nativeShareReceived(text: String?, filePaths: Array<String>)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // android-native-keyring-store (トークンの安全な保存先) が使う ndk-context を
        // 初期化する。Tauri は自動で行わないため、ここで明示的に呼ぶ必要がある。
        Keyring.initializeNdkContext(applicationContext)

        // 前回起動分の共有インテント一時ファイルの残骸を掃除する(Issue #116)。
        File(cacheDir, "shared-intents").deleteRecursively()
        handleShareIntent(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleShareIntent(intent)
    }

    /// 他アプリの共有シート(ACTION_SEND/ACTION_SEND_MULTIPLE)からのテキスト/画像/動画を
    /// 拾い、Rust側へ JNI 経由で渡す(Issue #116)。未対応MIME・パース失敗時は何もしない。
    private fun handleShareIntent(intent: Intent) {
        val type = intent.type ?: return
        val text = if (intent.action == Intent.ACTION_SEND && type == "text/plain") {
            intent.getStringExtra(Intent.EXTRA_TEXT)
        } else {
            null
        }

        val isMedia = type.startsWith("image/") || type.startsWith("video/")
        val uris: List<Uri> = if (!isMedia) {
            emptyList()
        } else when (intent.action) {
            Intent.ACTION_SEND -> listOfNotNull(getStreamExtra(intent))
            Intent.ACTION_SEND_MULTIPLE -> getStreamArrayExtra(intent)
            else -> emptyList()
        }

        val filePaths = uris.mapIndexedNotNull { index, uri -> copyToCache(uri, index) }

        if (text == null && filePaths.isEmpty()) return
        nativeShareReceived(text, filePaths.toTypedArray())
    }

    private fun getStreamExtra(intent: Intent): Uri? {
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
        } else {
            @Suppress("DEPRECATION")
            intent.getParcelableExtra(Intent.EXTRA_STREAM)
        }
    }

    private fun getStreamArrayExtra(intent: Intent): List<Uri> {
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM, Uri::class.java) ?: emptyList()
        } else {
            @Suppress("DEPRECATION")
            intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM) ?: emptyList()
        }
    }

    /// `content://` の共有ファイルを `cacheDir/shared-intents/<index>/` にコピーし、通常の
    /// ファイルパスとして扱えるようにする。`index` はバッチ内での位置(ACTION_SEND_MULTIPLE で
    /// 同名ファイルが複数来た場合の上書き事故を防ぐためのサブディレクトリ分離)。
    /// 失敗時はこの1件だけ諦めて null を返す。
    private fun copyToCache(uri: Uri, index: Int): String? {
        return try {
            val name = queryDisplayName(uri) ?: "shared-${System.currentTimeMillis()}"
            val dir = File(File(cacheDir, "shared-intents"), index.toString()).apply { mkdirs() }
            val dest = File(dir, name)
            val copied = contentResolver.openInputStream(uri)?.use { input ->
                dest.outputStream().use { output -> input.copyTo(output) }
                true
            } ?: false
            if (!copied) return null
            dest.absolutePath
        } catch (e: Exception) {
            Log.w("MainActivity", "failed to copy shared file: $uri", e)
            null
        }
    }

    /// `ContentResolver` から元のファイル名を引く。拡張子の当て推量はしない
    /// (誤った拡張子でMisskeyドライブに入るのを避けるため)。
    private fun queryDisplayName(uri: Uri): String? {
        var cursor: Cursor? = null
        try {
            cursor = contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
            if (cursor != null && cursor.moveToFirst()) {
                val idx = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                if (idx >= 0) return cursor.getString(idx)
            }
        } finally {
            cursor?.close()
        }
        return null
    }
}
