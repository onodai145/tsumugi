import java.io.FileInputStream
import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

android {
    compileSdk = 36
    namespace = "com.onodai.tsumugi"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        applicationId = "com.onodai.tsumugi"
        // rodio(通知音のネイティブ再生, Issue #12)のAndroidバックエンド(Oboe)がビルド時に
        // AAudioを無条件でリンクするため、AAudioが存在しないAPI24/25はビルドできない
        // (リンクエラー: unable to find library -laaudio)。AAudioはAPI26(Android 8.0)から。
        minSdk = 26
        targetSdk = 36
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
    }
    // リリース署名鍵。ローカル開発では keystore.properties が無くても
    // (release ビルドタイプを使わない限り)壊れないよう null許容で読む。
    // CI では release.yml が gen/android/keystore.properties を都度生成する。
    signingConfigs {
        create("release") {
            val keystorePropertiesFile = rootProject.file("keystore.properties")
            val keystoreProperties = Properties()
            if (keystorePropertiesFile.exists()) {
                keystoreProperties.load(FileInputStream(keystorePropertiesFile))
            }
            keyAlias = keystoreProperties["keyAlias"] as String?
            keyPassword = keystoreProperties["password"] as String?
            storeFile = (keystoreProperties["storeFile"] as String?)?.let { file(it) }
            storePassword = keystoreProperties["password"] as String?
        }
    }
    buildTypes {
        getByName("debug") {
            applicationIdSuffix = ".debug"
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            signingConfig = signingConfigs.getByName("release")
            isMinifyEnabled = true
            proguardFiles(
                *fileTree(".") { include("**/*.pro") }
                    .plus(getDefaultProguardFile("proguard-android-optimize.txt"))
                    .toList().toTypedArray()
            )
        }
    }
    kotlinOptions {
        jvmTarget = "1.8"
    }
    buildFeatures {
        buildConfig = true
    }
}

// 内蔵 VOICEVOX CORE(Issue #625 段階3)の libvoicevox_core.so は、共有の libc++_shared.so を要求する。
// アプリ本体は静的 libc++ なので、NDK のものを、ビルド時にコピーして APK に入れる(バイナリはリポジトリに入れない)。
// 64bit の ABI だけ(内蔵は arm64 と x86_64 でだけ使える)。NDK は CI・ローカルとも、環境変数で r27c を指す(CLAUDE.md)。
val voicevoxLibcxxDir = layout.buildDirectory.dir("generated/voicevoxLibcxx")
// コピー元の NDK。環境変数があればそれ、無ければ AGP の既定の NDK。ただし後者は、r27c(27.x)でなければ止める
// (別の版の libc++_shared.so が黙って入るのを防ぐ。llama.cpp の NDK の件と同じ種類の不一致)。
// プロバイダーにして、タスクが要るときにだけ評価する(NDK が無くても、`gradlew tasks` などは動く)。
val voicevoxNdkRoot = providers.provider {
    val fromEnv = listOf("ANDROID_NDK_ROOT", "ANDROID_NDK_HOME", "NDK_HOME")
        .firstNotNullOfOrNull { System.getenv(it)?.takeIf { v -> v.isNotBlank() } }
    if (fromEnv != null) {
        File(fromEnv).absolutePath
    } else {
        val dir = try { android.ndkDirectory } catch (e: Exception) { null }
        if (dir == null || !dir.name.startsWith("27.")) {
            throw GradleException("NDK が r27c(27.x)として見つかりません(AGP の既定: ${dir ?: "未インストール"})。ANDROID_NDK_ROOT で r27c を指してください(CLAUDE.md の Android の節)")
        }
        dir.absolutePath
    }
}
val copyLibcxxShared by tasks.registering {
    val outDir = voicevoxLibcxxDir
    val ndkRoot = voicevoxNdkRoot
    outputs.dir(outDir)
    // NDK を替えたら、コピーし直す(入力に無いと、出力が残っている間は UP-TO-DATE のまま、古い版の libc++ が入り続ける)。
    inputs.property("ndkRoot", ndkRoot)
    doLast {
        val ndk = File(ndkRoot.get())
        val prebuilt = File(ndk, "toolchains/llvm/prebuilt").listFiles()?.firstOrNull()
            ?: throw GradleException("NDK の toolchains/llvm/prebuilt が見つかりません: $ndk")
        val out = outDir.get().asFile
        mapOf("arm64-v8a" to "aarch64-linux-android", "x86_64" to "x86_64-linux-android").forEach { (abi, triple) ->
            val src = File(prebuilt, "sysroot/usr/lib/$triple/libc++_shared.so")
            if (!src.isFile) throw GradleException("libc++_shared.so が見つかりません: $src")
            File(out, abi).mkdirs()
            src.copyTo(File(out, "$abi/libc++_shared.so"), overwrite = true)
        }
    }
}
android.sourceSets.getByName("main").jniLibs.srcDir(voicevoxLibcxxDir)
tasks.configureEach {
    if (name.startsWith("merge") && name.endsWith("JniLibFolders")) dependsOn(copyLibcxxShared)
}

rust {
    rootDirRel = "../../../"
}

dependencies {
    implementation("androidx.webkit:webkit:1.14.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = "tauri.build.gradle.kts")