//! Misskey Streaming(WebSocket) のメッセージ組み立て・解釈。OpenAPI 仕様外のため手書き。
//! 参考: Misskey Hub「Streaming API」。1接続内で複数チャンネルを id で多重化する。

use crate::api::normalize::RawNote;
use serde_json::{json, Value};

/// チャンネル購読を開始する `connect` メッセージ（JSON文字列）。
pub fn connect(channel: &str, id: &str, params: Value) -> String {
    json!({
        "type": "connect",
        "body": { "channel": channel, "id": id, "params": params }
    })
    .to_string()
}

/// チャンネル購読を解除する `disconnect` メッセージ。
pub fn disconnect(id: &str) -> String {
    json!({ "type": "disconnect", "body": { "id": id } }).to_string()
}

/// 表示中ノートのリアクション等を追う `subNote`（キャプチャ登録）。Phase 2 後半で使用。
#[allow(dead_code)]
pub fn sub_note(note_id: &str) -> String {
    json!({ "type": "subNote", "body": { "id": note_id } }).to_string()
}

/// キャプチャ解除 `unsubNote`。Phase 2 後半で使用。
#[allow(dead_code)]
pub fn unsub_note(note_id: &str) -> String {
    json!({ "type": "unsubNote", "body": { "id": note_id } }).to_string()
}

/// 受信メッセージを解釈した結果。Phase 2 で扱うもの以外は [`Incoming::Other`]。
/// `channel_id` / NoteUpdated の各フィールドはキャプチャ更新対応（Phase 2 後半）で参照する。
#[derive(Debug)]
#[allow(dead_code)]
pub enum Incoming {
    /// あるチャンネル(id)に新規ノートが流れてきた
    ChannelNote {
        channel_id: String,
        note: Box<RawNote>,
    },
    /// キャプチャ中ノートの更新（reacted / unreacted / pollVoted / deleted 等）
    NoteUpdated {
        note_id: String,
        kind: String,
        body: Value,
    },
    /// 未対応 or 解釈不能
    Other,
}

/// テキストフレームを [`Incoming`] へ解釈する。未知形式でも panic せず `Other`。
pub fn parse_incoming(text: &str) -> Incoming {
    let v: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => return Incoming::Other,
    };
    match v.get("type").and_then(Value::as_str) {
        Some("channel") => {
            let body = match v.get("body") {
                Some(b) => b,
                None => return Incoming::Other,
            };
            let channel_id = body.get("id").and_then(Value::as_str).unwrap_or("");
            let inner_type = body.get("type").and_then(Value::as_str).unwrap_or("");
            if inner_type == "note" {
                if let Some(note_val) = body.get("body") {
                    if let Ok(note) = serde_json::from_value::<RawNote>(note_val.clone()) {
                        return Incoming::ChannelNote {
                            channel_id: channel_id.to_string(),
                            note: Box::new(note),
                        };
                    }
                }
            }
            Incoming::Other
        }
        Some("noteUpdated") => {
            let body = v.get("body").cloned().unwrap_or(Value::Null);
            let note_id = body.get("id").and_then(Value::as_str).unwrap_or("").to_string();
            let kind = body.get("type").and_then(Value::as_str).unwrap_or("").to_string();
            let inner = body.get("body").cloned().unwrap_or(Value::Null);
            if note_id.is_empty() {
                Incoming::Other
            } else {
                Incoming::NoteUpdated {
                    note_id,
                    kind,
                    body: inner,
                }
            }
        }
        _ => Incoming::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_message_shape() {
        let s = connect("homeTimeline", "abc", json!({}));
        let v: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["type"], "connect");
        assert_eq!(v["body"]["channel"], "homeTimeline");
        assert_eq!(v["body"]["id"], "abc");
        assert!(v["body"]["params"].is_object());
    }

    #[test]
    fn disconnect_and_subnote_shape() {
        let d: Value = serde_json::from_str(&disconnect("abc")).unwrap();
        assert_eq!(d["type"], "disconnect");
        assert_eq!(d["body"]["id"], "abc");

        let s: Value = serde_json::from_str(&sub_note("note1")).unwrap();
        assert_eq!(s["type"], "subNote");
        assert_eq!(s["body"]["id"], "note1");
    }

    #[test]
    fn parse_channel_note() {
        let msg = r#"{"type":"channel","body":{"id":"col1","type":"note",
            "body":{"id":"n1","createdAt":"2026-07-05T00:00:00Z",
            "user":{"id":"u1","username":"a"}}}}"#;
        match parse_incoming(msg) {
            Incoming::ChannelNote { channel_id, note } => {
                assert_eq!(channel_id, "col1");
                assert_eq!(note.id, "n1");
            }
            other => panic!("expected ChannelNote, got {other:?}"),
        }
    }

    #[test]
    fn parse_note_updated() {
        let msg = r#"{"type":"noteUpdated","body":{"id":"n1","type":"reacted",
            "body":{"reaction":"👍","userId":"u2"}}}"#;
        match parse_incoming(msg) {
            Incoming::NoteUpdated { note_id, kind, body } => {
                assert_eq!(note_id, "n1");
                assert_eq!(kind, "reacted");
                assert_eq!(body["reaction"], "👍");
            }
            other => panic!("expected NoteUpdated, got {other:?}"),
        }
    }

    #[test]
    fn unknown_and_garbage_are_other() {
        assert!(matches!(parse_incoming(r#"{"type":"emitCaptureData"}"#), Incoming::Other));
        assert!(matches!(parse_incoming("not json"), Incoming::Other));
        assert!(matches!(parse_incoming(r#"{"no":"type"}"#), Incoming::Other));
    }
}
