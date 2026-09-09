use tauri::{command, AppHandle, Runtime};

use crate::models::VibrateRequest;
use crate::HapticsExt;
use crate::Result;

#[command]
pub(crate) async fn vibrate<R: Runtime>(app: AppHandle<R>, payload: VibrateRequest) -> Result<()> {
    app.haptics().vibrate(payload.pattern)
}
