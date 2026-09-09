use serde::de::DeserializeOwned;
use tauri::{plugin::PluginApi, AppHandle, Runtime};

use crate::models::HapticPattern;

pub fn init<R: Runtime, C: DeserializeOwned>(
    app: &AppHandle<R>,
    _api: PluginApi<R, C>,
) -> crate::Result<Haptics<R>> {
    Ok(Haptics(app.clone()))
}

/// デスクトップでは振動デバイスが無いためno-op。
pub struct Haptics<R: Runtime>(AppHandle<R>);

impl<R: Runtime> Haptics<R> {
    pub fn vibrate(&self, _pattern: HapticPattern) -> crate::Result<()> {
        Ok(())
    }
}
