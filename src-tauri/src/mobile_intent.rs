//! 共有インテント(Android の ACTION_SEND/ACTION_SEND_MULTIPLE)受信用の橋渡し。
//! `MainActivity.kt` から JNI 経由で呼ばれ、プロセスグローバルな1件分の保留領域に
//! 格納する。フロントは `commands::app::get_pending_share` でポーリングして取り出す
//! (Issue #116)。他OSでは常に `None` を返す no-op。

use crate::domain::ShareReceived;

#[cfg(target_os = "android")]
mod android {
    use super::ShareReceived;
    use jni::objects::{JObject, JObjectArray, JString};
    use jni::JNIEnv;
    use std::sync::Mutex;

    static PENDING_SHARE: Mutex<Option<ShareReceived>> = Mutex::new(None);

    /// `MainActivity.kt` の `private external fun nativeShareReceived(...)` から呼ばれる。
    /// `text` は無ければ Java 側で null、`file_paths` は要素0件の配列で渡ってくる想定。
    #[no_mangle]
    pub extern "system" fn Java_com_onodai_tsumugi_MainActivity_nativeShareReceived<'local>(
        mut env: JNIEnv<'local>,
        _this: JObject<'local>,
        text: JString<'local>,
        file_paths: JObjectArray<'local>,
    ) {
        let text = if text.is_null() {
            None
        } else {
            env.get_string(&text).ok().map(|s| s.into())
        };

        let len = env.get_array_length(&file_paths).unwrap_or(0).max(0);
        let mut paths = Vec::with_capacity(len as usize);
        for i in 0..len {
            let Ok(obj) = env.get_object_array_element(&file_paths, i) else {
                continue;
            };
            let jstr = JString::from(obj);
            if let Some(s) = env.get_string(&jstr).ok().map(|s| s.into()) {
                paths.push(s);
            }
        }

        if text.is_none() && paths.is_empty() {
            return;
        }

        *PENDING_SHARE.lock().unwrap() = Some(ShareReceived {
            text,
            file_paths: paths,
        });
    }

    pub fn take_pending_share() -> Option<ShareReceived> {
        PENDING_SHARE.lock().unwrap().take()
    }
}

#[cfg(target_os = "android")]
pub fn take_pending_share() -> Option<ShareReceived> {
    android::take_pending_share()
}

#[cfg(not(target_os = "android"))]
pub fn take_pending_share() -> Option<ShareReceived> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(target_os = "android"))]
    #[test]
    fn take_pending_share_is_noop_off_android() {
        assert_eq!(take_pending_share(), None);
    }
}
