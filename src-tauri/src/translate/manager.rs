//! モデルの遅延ロード、アイドル時のアンロード、翻訳要求の直列化（Issue #603）。

use super::engine::LocalTranslator;
use super::{invalid, ERR_UNSUPPORTED};
use crate::error::{Error, Result};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// 推論エンジンを作る関数。ブロッキング（モデルの読み込み）なので `spawn_blocking` で呼ぶ。
pub type Loader = Arc<dyn Fn() -> Result<Arc<dyn LocalTranslator>> + Send + Sync>;

struct Loaded {
    translator: Arc<dyn LocalTranslator>,
    last_used: Instant,
}

pub struct LocalTranslate {
    loader: Loader,
    loaded: Mutex<Option<Loaded>>,
}

impl LocalTranslate {
    pub fn new(loader: Loader) -> Self {
        Self { loader, loaded: Mutex::new(None) }
    }

    /// ローカル翻訳に対応していない構成（ビルドで無効、モバイル、モデルの置き場が未設定）。
    pub fn unconfigured() -> Self {
        Self::new(Arc::new(|| Err(invalid(ERR_UNSUPPORTED))))
    }

    pub async fn translate(&self, text: &str, plamo_lang: &str) -> Result<String> {
        // ロックを推論の終わりまで持つ: 翻訳要求を直列に処理し、ロード中の二重ロードも防ぐ。
        let mut guard = self.loaded.lock().await;
        let translator = match guard.as_ref() {
            Some(l) => l.translator.clone(),
            None => {
                let loader = self.loader.clone();
                // ロードの失敗は覚えない（モデルが後から置かれたら、再起動なしで次の翻訳から使える）。
                let t = tokio::task::spawn_blocking(move || loader()).await.map_err(join_err)??;
                *guard = Some(Loaded { translator: t.clone(), last_used: Instant::now() });
                t
            }
        };
        let (text, lang) = (text.to_string(), plamo_lang.to_string());
        let out = tokio::task::spawn_blocking(move || translator.translate(&text, &lang))
            .await
            .map_err(join_err)?;
        if let Some(l) = guard.as_mut() {
            l.last_used = Instant::now();
        }
        out
    }

    /// 最後に使ってから `idle` 以上たっていたらモデルを解放する。解放したら true。
    pub async fn unload_if_idle(&self, now: Instant, idle: Duration) -> bool {
        let mut guard = self.loaded.lock().await;
        match guard.as_ref() {
            Some(l) if now.saturating_duration_since(l.last_used) >= idle => {
                *guard = None;
                true
            }
            _ => false,
        }
    }

    pub async fn is_loaded(&self) -> bool {
        self.loaded.lock().await.is_some()
    }
}

fn join_err(e: tokio::task::JoinError) -> Error {
    // 本文は含めない（ログ・エラーに本文を出さない）。
    Error::Invalid(format!("local translate task failed: {e}"))
}

/// アイドル解放の定期確認を始める。`translate` がロックを持っている間は待つので、翻訳中に解放されない。
pub fn spawn_idle_sweeper(this: Arc<LocalTranslate>, idle: Duration, interval: Duration) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(interval).await;
            this.unload_if_idle(Instant::now(), idle).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// `"{lang}:{text}"` を返す偽の翻訳器。同時に動いた最大数を記録する。
    struct Fake {
        active: AtomicUsize,
        max_active: Arc<AtomicUsize>,
        fail_with: Option<&'static str>,
    }

    impl LocalTranslator for Fake {
        fn translate(&self, text: &str, plamo_lang: &str) -> Result<String> {
            let now = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(20));
            self.active.fetch_sub(1, Ordering::SeqCst);
            match self.fail_with {
                Some(code) => Err(invalid(code)),
                None => Ok(format!("{plamo_lang}:{text}")),
            }
        }
    }

    fn counting_loader(loads: Arc<AtomicUsize>, max_active: Arc<AtomicUsize>) -> Loader {
        Arc::new(move || {
            loads.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(Fake { active: AtomicUsize::new(0), max_active: max_active.clone(), fail_with: None })
                as Arc<dyn LocalTranslator>)
        })
    }

    fn assert_invalid(r: Result<String>, code: &str) {
        match r {
            Err(Error::Invalid(m)) => assert_eq!(m, code),
            other => panic!("expected Invalid({code}), got {other:?}"),
        }
    }

    #[tokio::test]
    async fn loads_lazily_and_only_once() {
        let loads = Arc::new(AtomicUsize::new(0));
        let lt = LocalTranslate::new(counting_loader(loads.clone(), Arc::new(AtomicUsize::new(0))));
        assert!(!lt.is_loaded().await);
        assert_eq!(loads.load(Ordering::SeqCst), 0);

        assert_eq!(lt.translate("a", "en").await.unwrap(), "en:a");
        assert_eq!(lt.translate("b", "ja").await.unwrap(), "ja:b");
        assert!(lt.is_loaded().await);
        assert_eq!(loads.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn load_failure_is_not_remembered() {
        // モデルが無い→後から置かれた、を再起動なしで拾えること。
        let attempts = Arc::new(AtomicUsize::new(0));
        let a = attempts.clone();
        let loader: Loader = Arc::new(move || {
            if a.fetch_add(1, Ordering::SeqCst) == 0 {
                Err(invalid(super::super::ERR_MODEL_NOT_READY))
            } else {
                Ok(Arc::new(Fake {
                    active: AtomicUsize::new(0),
                    max_active: Arc::new(AtomicUsize::new(0)),
                    fail_with: None,
                }) as Arc<dyn LocalTranslator>)
            }
        });
        let lt = LocalTranslate::new(loader);

        assert_invalid(lt.translate("a", "en").await, "LOCAL_MODEL_NOT_READY");
        assert!(!lt.is_loaded().await);
        assert_eq!(lt.translate("a", "en").await.unwrap(), "en:a");
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn translation_error_is_passed_through_and_keeps_the_model_loaded() {
        let loader: Loader = Arc::new(|| {
            Ok(Arc::new(Fake {
                active: AtomicUsize::new(0),
                max_active: Arc::new(AtomicUsize::new(0)),
                fail_with: Some("LOCAL_TEXT_TOO_LONG"),
            }) as Arc<dyn LocalTranslator>)
        });
        let lt = LocalTranslate::new(loader);
        assert_invalid(lt.translate("long", "en").await, "LOCAL_TEXT_TOO_LONG");
        assert!(lt.is_loaded().await, "入力が長すぎるだけでモデルを捨てない");
    }

    #[tokio::test]
    async fn concurrent_requests_run_one_at_a_time_without_mixing_results() {
        let max_active = Arc::new(AtomicUsize::new(0));
        let lt = Arc::new(LocalTranslate::new(counting_loader(
            Arc::new(AtomicUsize::new(0)),
            max_active.clone(),
        )));
        let (a, b, c, d) = tokio::join!(
            lt.translate("one", "en"),
            lt.translate("two", "ja"),
            lt.translate("three", "fr"),
            lt.translate("four", "de"),
        );
        assert_eq!(a.unwrap(), "en:one");
        assert_eq!(b.unwrap(), "ja:two");
        assert_eq!(c.unwrap(), "fr:three");
        assert_eq!(d.unwrap(), "de:four");
        assert_eq!(max_active.load(Ordering::SeqCst), 1, "同時に2つ以上の推論が走った");
    }

    #[tokio::test]
    async fn unloads_only_after_the_idle_time_and_reloads_on_next_use() {
        let loads = Arc::new(AtomicUsize::new(0));
        let lt = LocalTranslate::new(counting_loader(loads.clone(), Arc::new(AtomicUsize::new(0))));
        let idle = Duration::from_secs(600);

        // ロード前は解放するものがない。
        assert!(!lt.unload_if_idle(Instant::now() + idle * 2, idle).await);

        lt.translate("a", "en").await.unwrap();
        assert!(!lt.unload_if_idle(Instant::now() + Duration::from_secs(10), idle).await);
        assert!(lt.is_loaded().await);

        assert!(lt.unload_if_idle(Instant::now() + idle + Duration::from_secs(1), idle).await);
        assert!(!lt.is_loaded().await);

        lt.translate("b", "en").await.unwrap();
        assert_eq!(loads.load(Ordering::SeqCst), 2, "解放後の翻訳で再ロードされる");
    }

    #[tokio::test]
    async fn unconfigured_reports_unsupported() {
        let lt = LocalTranslate::unconfigured();
        assert_invalid(lt.translate("a", "en").await, "LOCAL_UNSUPPORTED");
        assert!(!lt.is_loaded().await);
    }
}
