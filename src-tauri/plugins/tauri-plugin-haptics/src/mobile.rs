use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::de::DeserializeOwned;
use tauri::{
    plugin::{PluginApi, PluginHandle},
    AppHandle, Runtime,
};

use crate::models::{HapticPattern, PlayHapticAudioRequest, VibrateRequest};

pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> crate::Result<Haptics<R>> {
    #[cfg(target_os = "android")]
    let handle = api.register_android_plugin("com.onodai.tsumugi.haptics", "HapticsPlugin")?;
    Ok(Haptics(handle))
}

pub struct Haptics<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> Haptics<R> {
    pub fn vibrate(&self, pattern: HapticPattern) -> crate::Result<()> {
        self.0
            .run_mobile_plugin("vibrate", VibrateRequest { pattern })
            .map_err(Into::into)
    }

    /// 触覚入りの音声(ogg)を、触覚つきで再生する(Issue #592)。
    /// `haptics_enabled` が false のときは触覚をミュートして音だけ鳴らす。
    /// Kotlin 側が再生を始めたら `Ok`、失敗(準備エラーや、Kotlin 側の 5 秒の準備タイムアウト)なら `Err`。
    /// 呼び出しは同期でブロックする。Rust 側にタイムアウトはなく、Kotlin が必ず結果を返すことに依存する
    /// (`HapticAudioPlayer` が、どの経路でも 1 回だけ返す)。
    pub fn play_haptic_audio(&self, data: &[u8], haptics_enabled: bool) -> crate::Result<()> {
        self.0
            .run_mobile_plugin(
                "playHapticAudio",
                PlayHapticAudioRequest {
                    data: STANDARD.encode(data),
                    haptics_enabled,
                },
            )
            .map_err(Into::into)
    }
}
