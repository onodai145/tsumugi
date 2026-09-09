use serde::de::DeserializeOwned;
use tauri::{
    plugin::{PluginApi, PluginHandle},
    AppHandle, Runtime,
};

use crate::models::{HapticPattern, VibrateRequest};

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
}
