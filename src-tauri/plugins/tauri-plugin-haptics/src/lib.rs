use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime,
};

pub use models::HapticPattern;

#[cfg(desktop)]
mod desktop;
#[cfg(mobile)]
mod mobile;

mod commands;
mod error;
mod models;

pub use error::{Error, Result};

#[cfg(desktop)]
use desktop::Haptics;
#[cfg(mobile)]
use mobile::Haptics;

/// [`tauri::App`]/[`tauri::AppHandle`]/[`tauri::Window`] からハプティクスAPIへアクセスする拡張。
pub trait HapticsExt<R: Runtime> {
    fn haptics(&self) -> &Haptics<R>;
}

impl<R: Runtime, T: Manager<R>> HapticsExt<R> for T {
    fn haptics(&self) -> &Haptics<R> {
        self.state::<Haptics<R>>().inner()
    }
}

/// プラグインを初期化する。
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("haptics")
        .invoke_handler(tauri::generate_handler![commands::vibrate])
        .setup(|app, api| {
            #[cfg(mobile)]
            let haptics = mobile::init(app, api)?;
            #[cfg(desktop)]
            let haptics = desktop::init(app, api)?;
            app.manage(haptics);
            Ok(())
        })
        .build()
}
