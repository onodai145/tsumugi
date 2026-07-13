package io.crates.keyring

import android.content.Context

// android-native-keyring-store クレートが要求する ndk-context 初期化用ブリッジ。
// JNI関数はアプリ本体の libtsumugi_lib.so に静的リンクされているため、
// 別途 .so をロードする必要はない(MainActivity 側で既にロード済み)。
class Keyring {
    companion object {
        external fun initializeNdkContext(context: Context)
    }
}
