use serde::{Deserialize, Serialize};
use specta::Type;

const DEFAULT_ROW_WEIGHT_PX: f32 = 300.0;
const DEFAULT_COLUMN_WEIGHT: f32 = 1.0;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SplitDirection {
    Row,
    Column,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum PaneNode {
    Leaf {
        id: String,
        group_id: String,
    },
    Split {
        id: String,
        direction: SplitDirection,
        children: Vec<PaneChild>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PaneChild {
    pub node: PaneNode,
    pub size: f32,
    pub auto: bool,
}

impl PaneNode {
    pub fn new_leaf(group_id: impl Into<String>) -> Self {
        PaneNode::Leaf {
            id: uuid::Uuid::new_v4().to_string(),
            group_id: group_id.into(),
        }
    }

    fn default_weight(direction: SplitDirection) -> f32 {
        match direction {
            SplitDirection::Row => DEFAULT_ROW_WEIGHT_PX,
            SplitDirection::Column => DEFAULT_COLUMN_WEIGHT,
        }
    }

    /// reference_group_id を持つ Leaf の直後に、new_group_id の Leaf を direction 方向へ
    /// 挿入する。reference の親が同じ direction の Split ならその子として差し込み、
    /// reference 自身の size を半分にして新規 Leaf に半分を渡す(他の兄弟は不変)。
    /// 異なる direction(またはルート直下の裸Leaf)なら reference の位置を新しい
    /// direction の Split でラップし、内部2子を既定ウェイトで均等にする。
    /// reference が見つからなければ false。
    pub fn insert_sibling(&mut self, reference_group_id: &str, new_group_id: &str, direction: SplitDirection) -> bool {
        if let PaneNode::Leaf { group_id, .. } = self {
            if group_id != reference_group_id {
                return false;
            }
            let old = std::mem::replace(self, PaneNode::new_leaf(String::new()));
            let w = Self::default_weight(direction);
            *self = PaneNode::Split {
                id: uuid::Uuid::new_v4().to_string(),
                direction,
                children: vec![
                    PaneChild { node: old, size: w, auto: false },
                    PaneChild { node: PaneNode::new_leaf(new_group_id), size: w, auto: false },
                ],
            };
            return true;
        }
        let PaneNode::Split { direction: my_dir, children, .. } = self else {
            unreachable!("Leaf case handled above")
        };
        if let Some(idx) = children
            .iter()
            .position(|c| matches!(&c.node, PaneNode::Leaf { group_id, .. } if group_id == reference_group_id))
        {
            if *my_dir == direction {
                let half = children[idx].size / 2.0;
                children[idx].size = half;
                children.insert(idx + 1, PaneChild { node: PaneNode::new_leaf(new_group_id), size: half, auto: false });
            } else {
                let old_child = children.remove(idx);
                let w = Self::default_weight(direction);
                let wrapped = PaneNode::Split {
                    id: uuid::Uuid::new_v4().to_string(),
                    direction,
                    children: vec![
                        PaneChild { node: old_child.node, size: w, auto: false },
                        PaneChild { node: PaneNode::new_leaf(new_group_id), size: w, auto: false },
                    ],
                };
                children.insert(idx, PaneChild { node: wrapped, size: old_child.size, auto: old_child.auto });
            }
            return true;
        }
        for child in children.iter_mut() {
            if child.node.insert_sibling(reference_group_id, new_group_id, direction) {
                return true;
            }
        }
        false
    }

    /// group_id を持つ Leaf を木から取り除く。空いた size は直前の兄弟(無ければ直後の
    /// 兄弟)にすべて譲る(他の兄弟は不変)。親の子が1つになったら、その親 Split を
    /// 残った子で置き換えて畳む(size/autoは畳まれたSplit自身が親から見て持っていた
    /// 値を引き継ぐ)。見つかって除去できたら true。ルート自体がLeafの場合は
    /// 除去対象が無い(最後の1ペインは呼び出し側で別途扱う)ので false。
    pub fn remove_group(&mut self, group_id: &str) -> bool {
        let PaneNode::Split { children, .. } = self else {
            return false;
        };
        if let Some(idx) = children
            .iter()
            .position(|c| matches!(&c.node, PaneNode::Leaf { group_id: g, .. } if g == group_id))
        {
            let removed = children.remove(idx);
            if idx > 0 {
                children[idx - 1].size += removed.size;
            } else if !children.is_empty() {
                children[0].size += removed.size;
            }
            if children.len() == 1 {
                let only = children.remove(0);
                *self = only.node;
            }
            return true;
        }
        for child in children.iter_mut() {
            if child.node.remove_group(group_id) {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_sibling_same_direction_halves_reference_size() {
        // root: Split(Row)[ Leaf(a, size=300), Leaf(b, size=300) ]
        let mut root = PaneNode::Split {
            id: "root".into(),
            direction: SplitDirection::Row,
            children: vec![
                PaneChild { node: PaneNode::Leaf { id: "la".into(), group_id: "a".into() }, size: 300.0, auto: false },
                PaneChild { node: PaneNode::Leaf { id: "lb".into(), group_id: "b".into() }, size: 300.0, auto: false },
            ],
        };
        assert!(root.insert_sibling("a", "c", SplitDirection::Row));
        let PaneNode::Split { children, .. } = &root else { panic!("root must stay Split") };
        assert_eq!(children.len(), 3);
        assert_eq!(children[0].size, 150.0); // 分割元(a)は半分になる
        assert_eq!(children[1].size, 150.0); // 新規(c)がもう半分
        assert_eq!(children[2].size, 300.0); // 無関係な兄弟(b)は変化しない
        let PaneNode::Leaf { group_id, .. } = &children[1].node else { panic!("expected leaf") };
        assert_eq!(group_id, "c");
    }

    #[test]
    fn insert_sibling_different_direction_wraps_reference() {
        // root: Leaf(a) のみ(まだ分割されていない単一グループ)
        let mut root = PaneNode::new_leaf("a");
        assert!(root.insert_sibling("a", "c", SplitDirection::Column));
        let PaneNode::Split { direction, children, .. } = &root else { panic!("root must become Split") };
        assert_eq!(*direction, SplitDirection::Column);
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].size, 1.0);
        assert_eq!(children[1].size, 1.0);
        let PaneNode::Leaf { group_id, .. } = &children[0].node else { panic!("expected leaf") };
        assert_eq!(group_id, "a");
        let PaneNode::Leaf { group_id, .. } = &children[1].node else { panic!("expected leaf") };
        assert_eq!(group_id, "c");
    }

    #[test]
    fn insert_sibling_wraps_inside_existing_row_when_direction_differs() {
        // root: Split(Row)[ Leaf(a, size=300), Leaf(b, size=300) ]
        // a を下に分割(Column)すると、a の位置が Split(Column)[a, c] にラップされ、
        // root の子リスト自体は2つのまま(bはそのまま)、a のスロットの size(300)は維持される。
        let mut root = PaneNode::Split {
            id: "root".into(),
            direction: SplitDirection::Row,
            children: vec![
                PaneChild { node: PaneNode::Leaf { id: "la".into(), group_id: "a".into() }, size: 300.0, auto: false },
                PaneChild { node: PaneNode::Leaf { id: "lb".into(), group_id: "b".into() }, size: 300.0, auto: false },
            ],
        };
        assert!(root.insert_sibling("a", "c", SplitDirection::Column));
        let PaneNode::Split { direction: root_dir, children, .. } = &root else { panic!("root must stay Split") };
        assert_eq!(*root_dir, SplitDirection::Row);
        assert_eq!(children.len(), 2); // aの位置がラップされただけで、root直下の子数は変わらない
        assert_eq!(children[0].size, 300.0); // ラップ後もaスロット自体のsizeは維持
        assert_eq!(children[1].size, 300.0); // bは無関係、変化なし
        let PaneNode::Split { direction: inner_dir, children: inner, .. } = &children[0].node else {
            panic!("a's slot must now be a Column split")
        };
        assert_eq!(*inner_dir, SplitDirection::Column);
        assert_eq!(inner.len(), 2);
        assert_eq!(inner[0].size, 1.0);
        assert_eq!(inner[1].size, 1.0);
    }

    #[test]
    fn insert_sibling_returns_false_when_reference_not_found() {
        let mut root = PaneNode::new_leaf("a");
        assert!(!root.insert_sibling("nope", "c", SplitDirection::Column));
    }

    #[test]
    fn remove_group_gives_freed_size_to_previous_sibling() {
        // root: Split(Row)[a(150), c(150), b(300)] から c を消す -> a が300に戻る、bは無関係
        let mut root = PaneNode::Split {
            id: "root".into(),
            direction: SplitDirection::Row,
            children: vec![
                PaneChild { node: PaneNode::Leaf { id: "la".into(), group_id: "a".into() }, size: 150.0, auto: false },
                PaneChild { node: PaneNode::Leaf { id: "lc".into(), group_id: "c".into() }, size: 150.0, auto: false },
                PaneChild { node: PaneNode::Leaf { id: "lb".into(), group_id: "b".into() }, size: 300.0, auto: false },
            ],
        };
        assert!(root.remove_group("c"));
        let PaneNode::Split { children, .. } = &root else { panic!("root must stay Split (3->2 children)") };
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].size, 300.0); // aがcの分を引き継ぐ
        assert_eq!(children[1].size, 300.0); // bは無関係
    }

    #[test]
    fn remove_group_gives_freed_size_to_next_sibling_when_removing_first() {
        let mut root = PaneNode::Split {
            id: "root".into(),
            direction: SplitDirection::Row,
            children: vec![
                PaneChild { node: PaneNode::Leaf { id: "la".into(), group_id: "a".into() }, size: 150.0, auto: false },
                PaneChild { node: PaneNode::Leaf { id: "lb".into(), group_id: "b".into() }, size: 300.0, auto: false },
                PaneChild { node: PaneNode::Leaf { id: "lc".into(), group_id: "c".into() }, size: 300.0, auto: false },
            ],
        };
        assert!(root.remove_group("a"));
        let PaneNode::Split { children, .. } = &root else { panic!("root must stay Split") };
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].size, 450.0); // 先頭(a)を消したので直後のbが引き継ぐ
    }

    #[test]
    fn remove_group_collapses_split_with_one_remaining_child() {
        // root: Split(Row)[ Split(Column)[a(1.0), c(1.0)] (size=300 in root), b(300) ]
        let mut root = PaneNode::Split {
            id: "root".into(),
            direction: SplitDirection::Row,
            children: vec![
                PaneChild {
                    node: PaneNode::Split {
                        id: "inner".into(),
                        direction: SplitDirection::Column,
                        children: vec![
                            PaneChild { node: PaneNode::Leaf { id: "la".into(), group_id: "a".into() }, size: 1.0, auto: false },
                            PaneChild { node: PaneNode::Leaf { id: "lc".into(), group_id: "c".into() }, size: 1.0, auto: false },
                        ],
                    },
                    size: 300.0,
                    auto: false,
                },
                PaneChild { node: PaneNode::Leaf { id: "lb".into(), group_id: "b".into() }, size: 300.0, auto: false },
            ],
        };
        assert!(root.remove_group("c"));
        let PaneNode::Split { children, .. } = &root else { panic!("root must stay Split") };
        assert_eq!(children.len(), 2); // rootの子数は変わらない(inner splitがLeafに畳まれただけ)
        // inner splitが畳まれ、aのLeafがrootの直接の子(size=300を引き継ぐ)になっている
        let PaneNode::Leaf { group_id, .. } = &children[0].node else { panic!("inner split must collapse into a leaf") };
        assert_eq!(group_id, "a");
        assert_eq!(children[0].size, 300.0); // 畳まれたSplit自身が親から見て持っていたsizeを引き継ぐ
    }

    #[test]
    fn remove_group_returns_false_when_group_not_found() {
        let mut root = PaneNode::new_leaf("a");
        assert!(!root.remove_group("nope"));
    }
}
