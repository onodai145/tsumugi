//! TQL(Tsumugi Query Language) の AST。docs/filter-dsl-design.md §8。
//! `from <sources> where <expr>` を表す。フロントには公開せず Core 内で扱う（specta 不要）。

/// 値・フィールドが取りうる型。パース時の型検査に使う（§8）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterType {
    Boolean,
    Numeric,
    String,
    Set,
}

/// where 節の式木（Bool を返す木）。
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Or(Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    Compare {
        lhs: Value,
        op: CompareOp,
        rhs: Value,
    },
    /// Boolean述語の単独出現（例: `renote`）
    Bare(Value),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    /// `->` : 左(集合/文字列)が右を含む
    Contains,
    /// `<-` : 左が右(集合)に含まれる
    In,
    StartsWith,
    EndsWith,
    /// 正規表現
    Match,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArithOp {
    Add,
    Sub,
    Mul,
    Div,
}

/// 値。算術は Value 側に持たせる（`reactions + renotes > 10` の左辺が Arith になる）。
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Field(Field),
    Str(String),
    Num(f64),
    Set(Vec<Value>),
    /// `@name` : 自分のアカウント参照（EvalContext で解決）
    Account(String),
    Arith {
        lhs: Box<Value>,
        op: ArithOp,
        rhs: Box<Value>,
    },
}

/// フィールド。各バリアントが対応する型集合を持つ（§8）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    // Boolean
    Renote,
    Quote,
    Reply,
    HasFiles,
    HasPoll,
    Cw,
    Sensitive,
    Local,
    Remote,
    Bot,
    Cat,
    Direct,
    ToMe,
    ReplyToMe,
    HasMention,
    HasLink,
    Pinned,
    Reacted,
    Renoted,
    Favorited,
    Mine,
    Following,
    // Numeric
    Reactions, // Numeric と Set の両面（§3.4）
    Renotes,
    Replies,
    Files,
    Length,
    CreatedAt,
    UserFollowers,
    UserFollowing,
    UserNotes,
    // String
    Text,
    CwText,
    Via,
    Host,
    VisibilityStr,
    Channel,
    Lang,
    ReplyId,
    RenoteId,
    UserUsername,
    UserAcct,
    UserName,
    UserId,
    // Set
    Tags,
    Mentions,
    Emojis,
    FileTypes,
}

impl Field {
    /// 識別子（エイリアス含む）から Field を引く。§3 の語彙表に対応。
    pub fn from_name(name: &str) -> Option<Field> {
        use Field::*;
        Some(match name {
            // Boolean
            "renote" => Renote,
            "quote" => Quote,
            "reply" => Reply,
            "has_files" | "has_media" => HasFiles,
            "has_poll" => HasPoll,
            "cw" | "has_cw" => Cw,
            "sensitive" => Sensitive,
            "local" => Local,
            "remote" => Remote,
            "bot" => Bot,
            "cat" => Cat,
            "direct" => Direct,
            "to_me" | "mentions_me" => ToMe,
            "reply_to_me" => ReplyToMe,
            "has_mention" => HasMention,
            "has_link" | "url" => HasLink,
            "pinned" => Pinned,
            "reacted" => Reacted,
            "renoted" => Renoted,
            "favorited" => Favorited,
            "mine" => Mine,
            "following" => Following,
            // Numeric
            "reactions" => Reactions,
            "renotes" => Renotes,
            "replies" => Replies,
            "files" => Files,
            "length" => Length,
            "created_at" => CreatedAt,
            "user.followers" => UserFollowers,
            "user.following" => UserFollowing,
            "user.notes" => UserNotes,
            // String
            "text" => Text,
            "cw_text" => CwText,
            "via" => Via,
            "host" => Host,
            "visibility" => VisibilityStr,
            "channel" => Channel,
            "lang" => Lang,
            "reply_id" => ReplyId,
            "renote_id" => RenoteId,
            "user.username" => UserUsername,
            "user.acct" => UserAcct,
            "user.name" => UserName,
            "user.id" => UserId,
            // Set
            "tags" | "hashtags" => Tags,
            "mentions" | "to" => Mentions,
            "emojis" => Emojis,
            "file_types" => FileTypes,
            _ => return None,
        })
    }

    /// このフィールドがサポートする型（§8 の SupportedTypes）。
    pub fn supported_types(self) -> &'static [FilterType] {
        use Field::*;
        use FilterType::*;
        match self {
            // reactions は数値(合計)と集合(絵文字キー)の両面
            Reactions => &[Numeric, Set],
            // Boolean
            Renote | Quote | Reply | HasFiles | HasPoll | Cw | Sensitive | Local | Remote | Bot
            | Cat | Direct | ToMe | ReplyToMe | HasMention | HasLink | Pinned | Reacted | Renoted
            | Favorited | Mine | Following => &[Boolean],
            // Numeric
            Renotes | Replies | Files | Length | CreatedAt | UserFollowers | UserFollowing
            | UserNotes => &[Numeric],
            // String
            Text | CwText | Via | Host | VisibilityStr | Channel | Lang | ReplyId | RenoteId
            | UserUsername | UserAcct | UserName | UserId => &[String],
            // Set
            Tags | Mentions | Emojis | FileTypes => &[Set],
        }
    }

    pub fn supports(self, t: FilterType) -> bool {
        self.supported_types().contains(&t)
    }
}

/// from 節のソース。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Home,
    Local,
    Hybrid,
    Global,
    Mentions,
    List(String),
    Antenna(String),
    Channel(String),
    /// @acct
    User(String),
    Tag(String),
    Search(String),
    /// 受信せずローカル SQLite キャッシュを検索
    Cache,
}

/// TQL クエリ全体。predicate が None なら全通し（`from home` のみ）。
#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    pub sources: Vec<Source>,
    pub predicate: Option<Expr>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_aliases_resolve() {
        assert_eq!(Field::from_name("has_media"), Some(Field::HasFiles));
        assert_eq!(Field::from_name("has_files"), Some(Field::HasFiles));
        assert_eq!(Field::from_name("url"), Some(Field::HasLink));
        assert_eq!(Field::from_name("hashtags"), Some(Field::Tags));
        assert_eq!(Field::from_name("to"), Some(Field::Mentions));
        assert_eq!(Field::from_name("mentions_me"), Some(Field::ToMe));
        assert_eq!(Field::from_name("user.followers"), Some(Field::UserFollowers));
        assert_eq!(Field::from_name("nope"), None);
    }

    #[test]
    fn reactions_is_numeric_and_set() {
        assert!(Field::Reactions.supports(FilterType::Numeric));
        assert!(Field::Reactions.supports(FilterType::Set));
        assert!(!Field::Reactions.supports(FilterType::Boolean));
    }

    #[test]
    fn field_type_classification() {
        assert!(Field::Renote.supports(FilterType::Boolean));
        assert!(Field::Length.supports(FilterType::Numeric));
        assert!(Field::Text.supports(FilterType::String));
        assert!(Field::Tags.supports(FilterType::Set));
        assert!(!Field::Text.supports(FilterType::Numeric));
    }
}
