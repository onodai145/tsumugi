package com.onodai.tsumugi

import android.os.Bundle
import io.crates.keyring.Keyring

class MainActivity : TauriActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // android-native-keyring-store (トークンの安全な保存先) が使う ndk-context を
        // 初期化する。Tauri は自動で行わないため、ここで明示的に呼ぶ必要がある。
        Keyring.initializeNdkContext(applicationContext)
    }
}
