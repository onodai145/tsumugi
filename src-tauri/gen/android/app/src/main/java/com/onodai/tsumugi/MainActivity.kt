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
import java.util.concurrent.atomic.AtomicLong

class MainActivity : TauriActivity() {
    /// Rust側(mobile_intent.rs)へ共有インテントの内容を渡す(Issue #116)。
    /// `libtsumugi_lib.so` は generated/Rust.kt が既にロード済みなので追加ロード不要。
    private external fun nativeShareReceived(text: String?, filePaths: Array<String>)

    /// handleShareIntent() の呼び出し1回ごとに一意な値を振る(Issue #116 最終レビュー指摘: バッチを
    /// 跨いだファイル名衝突対策)。System.currentTimeMillis() だけだと同一ミリ秒内の連続呼び出しで
    /// 衝突しうるため、単調増加カウンタを併用する。
    private val shareInvocationCounter = AtomicLong(0)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // android-native-keyring-store (トークンの安全な保存先) が使う ndk-context を
        // 初期化する。Tauri は自動で行わないため、ここで明示的に呼ぶ必要がある。
        Keyring.initializeNdkContext(applicationContext)

        // 前回起動分の共有インテント一時ファイルの残骸を掃除する(Issue #116)。
        File(cacheDir, "shared-intents").deleteRecursively()

        // プロセスがバックグラウンドで再作成された後、Recents/タスクスイッチャーから単に
        // このActivityへ復帰しただけの場合、Androidは元のIntent(EXTRA_TEXT等を保持したまま)を
        // そのままonCreateへ渡してくる。これを共有の再受信として扱うと、過去に共有→投稿済みの
        // テキストが下書きへ再度湧いて出てしまう(Issue #116 最終レビュー指摘)。
        // FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY はまさにこの「履歴からの復帰」を示すフラグなので、
        // これが立っていない、新規の共有起動のときだけ処理する。
        if (intent.flags and Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY == 0) {
            handleShareIntent(intent)
        }
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

        if (uris.isEmpty()) {
            // テキストのみの共有はファイルI/Oが無いので、そのままメインスレッドで渡してよい。
            if (text == null) return
            nativeShareReceived(text, emptyArray())
            return
        }

        // 各共有(handleShareIntent呼び出し)ごとに一意なIDを振り、バッチを跨いだ同名ファイルの
        // 上書き事故を防ぐ(Issue #116 最終レビュー指摘)。
        val invocationId = "${System.currentTimeMillis()}-${shareInvocationCounter.incrementAndGet()}"

        // 大きな動画等のコピーがメインスレッドをブロックしてANRになるのを避けるため、
        // ファイルI/Oを伴う処理はバックグラウンドスレッドへ逃がす(Issue #116 最終レビュー指摘)。
        Thread {
            val filePaths = uris.mapIndexedNotNull { index, uri -> copyToCache(uri, invocationId, index) }
            if (text == null && filePaths.isEmpty()) return@Thread
            nativeShareReceived(text, filePaths.toTypedArray())
        }.start()
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

    /// `content://` の共有ファイルを `cacheDir/shared-intents/<invocationId>/<index>/` にコピーし、
    /// 通常のファイルパスとして扱えるようにする。`invocationId` は handleShareIntent() 呼び出し単位
    /// (共有アクション単位)、`index` はそのバッチ内での位置。両方を分離することで、同一バッチ内は
    /// もちろん、別々の共有(数分後に別ファイルが同名で来た場合等)でも同じパスに書き込まれることが
    /// ないようにしている(Issue #116 最終レビュー指摘)。失敗時はこの1件だけ諦めて null を返す。
    private fun copyToCache(uri: Uri, invocationId: String, index: Int): String? {
        return try {
            val name = sanitizeFileName(queryDisplayName(uri)) ?: "shared-${System.currentTimeMillis()}"
            val dir = File(File(File(cacheDir, "shared-intents"), invocationId), index.toString())
                .apply { mkdirs() }
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

    /// 共有元アプリが返す表示名(信頼できない入力)からパス区切りを除去し、ディレクトリトラバーサル
    /// (`../` 等で cacheDir/shared-intents/ の外、アプリ private storage 内の他ディレクトリへ書き込む)
    /// を防ぐ(Issue #116 最終レビュー指摘)。パス区切りを取り除いた結果が空、`.`、`..` になる場合は
    /// 呼び出し側のタイムスタンプ由来フォールバックに任せるため null を返す。
    private fun sanitizeFileName(name: String?): String? {
        if (name == null) return null
        val base = name.substringAfterLast('/').substringAfterLast('\\')
        if (base.isEmpty() || base == "." || base == "..") return null
        return base
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
