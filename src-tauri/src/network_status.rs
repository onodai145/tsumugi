//! 回線の種類と従量制（Issue #603 段階3）。Android の `MainActivity.kt` が `ConnectivityManager` の変化を
//! JNI で push し、フロントは `get_network_status` で読む（`mobile_intent.rs` と同じ構造）。
//! Android 以外は常に `Unknown`（デスクトップの判定は範囲外）。

use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Transport {
    Wifi,
    Ethernet,
    Cellular,
    Other,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Metered {
    Metered,
    Unmetered,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NetworkStatus {
    pub transport: Transport,
    pub metered: Metered,
}

impl NetworkStatus {
    pub const UNKNOWN: NetworkStatus = NetworkStatus {
        transport: Transport::Unknown,
        metered: Metered::Unknown,
    };

    /// Kotlin から渡る整数を変換する。範囲外は `Unknown`（将来 Kotlin 側に値を足しても Rust が落ちない）。
    pub fn from_raw(transport: i32, metered: i32) -> Self {
        NetworkStatus {
            transport: match transport {
                1 => Transport::Wifi,
                2 => Transport::Ethernet,
                3 => Transport::Cellular,
                4 => Transport::Other,
                _ => Transport::Unknown,
            },
            metered: match metered {
                1 => Metered::Metered,
                2 => Metered::Unmetered,
                _ => Metered::Unknown,
            },
        }
    }
}

static CURRENT: Mutex<NetworkStatus> = Mutex::new(NetworkStatus::UNKNOWN);

/// 最後に Kotlin から届いた値。まだ届いていなければ `Unknown`。
pub fn current() -> NetworkStatus {
    *CURRENT.lock().unwrap()
}

#[cfg(any(target_os = "android", test))]
pub(crate) fn set(s: NetworkStatus) {
    *CURRENT.lock().unwrap() = s;
}

/// 静的変数 `CURRENT` に触るテストを直列にする（テストは並列に走るので、別のテストの `set` が見えてしまう）。
/// 戻り値を持っている間だけ他のテストは待たされ、手放すとき(パニックでも)に `Unknown` へ戻す。
#[cfg(test)]
pub(crate) fn serialize_tests() -> TestGuard {
    static TEST_LOCK: Mutex<()> = Mutex::new(());
    TestGuard(TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner()))
}

#[cfg(test)]
pub(crate) struct TestGuard(#[allow(dead_code)] std::sync::MutexGuard<'static, ()>);

#[cfg(test)]
impl Drop for TestGuard {
    fn drop(&mut self) {
        set(NetworkStatus::UNKNOWN);
    }
}

#[cfg(target_os = "android")]
mod android {
    use super::{set, NetworkStatus};
    use jni::objects::JObject;
    use jni::sys::jint;
    use jni::EnvUnowned;

    /// `MainActivity.kt` の `private external fun nativeNetworkChanged(transport: Int, metered: Int)` から呼ばれる。
    #[no_mangle]
    pub extern "system" fn Java_com_onodai_tsumugi_MainActivity_nativeNetworkChanged<'local>(
        _env: EnvUnowned<'local>,
        _this: JObject<'local>,
        transport: jint,
        metered: jint,
    ) {
        set(NetworkStatus::from_raw(transport, metered));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_raw_maps_every_known_value() {
        assert_eq!(
            NetworkStatus::from_raw(1, 2),
            NetworkStatus {
                transport: Transport::Wifi,
                metered: Metered::Unmetered
            }
        );
        assert_eq!(NetworkStatus::from_raw(2, 2).transport, Transport::Ethernet);
        assert_eq!(
            NetworkStatus::from_raw(3, 1),
            NetworkStatus {
                transport: Transport::Cellular,
                metered: Metered::Metered
            }
        );
        assert_eq!(NetworkStatus::from_raw(4, 1).transport, Transport::Other);
        assert_eq!(NetworkStatus::from_raw(0, 0), NetworkStatus::UNKNOWN);
    }

    #[test]
    fn from_raw_turns_out_of_range_values_into_unknown() {
        assert_eq!(NetworkStatus::from_raw(99, -1), NetworkStatus::UNKNOWN);
        assert_eq!(
            NetworkStatus::from_raw(i32::MAX, i32::MIN),
            NetworkStatus::UNKNOWN
        );
    }

    #[test]
    fn serializes_with_camel_case_values() {
        let j = serde_json::to_value(NetworkStatus::from_raw(3, 1)).unwrap();
        assert_eq!(
            j,
            serde_json::json!({"transport": "cellular", "metered": "metered"})
        );
    }

    #[test]
    fn current_is_unknown_until_set_and_then_follows_the_last_value() {
        let _serial = serialize_tests();
        assert_eq!(current(), NetworkStatus::UNKNOWN);
        for _ in 0..200 {
            set(NetworkStatus::from_raw(3, 1));
            std::thread::yield_now();
            assert_eq!(current().transport, Transport::Cellular);
            set(NetworkStatus::UNKNOWN);
            std::thread::yield_now();
            assert_eq!(current(), NetworkStatus::UNKNOWN);
        }
    }
}
