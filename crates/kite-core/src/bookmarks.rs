//! Bookmarks, stored in Falkon's `bookmarks.json` format so existing Falkon
//! bookmark files can be imported verbatim.
//!
//! Every node gets a runtime-only numeric id so QML and the WebExtension
//! `bookmarks` API can address it.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Folder,
    Url,
    Separator,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    #[serde(skip)]
    pub id: u64,
    #[serde(rename = "type")]
    pub kind: Kind,
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub keyword: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub visit_count: i64,
    #[serde(default, rename = "dateAdded", skip_serializing_if = "is_zero")]
    pub date_added: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expanded: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expanded_sidebar: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Node>,
}

fn is_zero(v: &i64) -> bool {
    *v == 0
}

impl Node {
    pub fn folder(name: &str) -> Self {
        Node {
            id: 0,
            kind: Kind::Folder,
            name: name.into(),
            url: String::new(),
            keyword: String::new(),
            description: String::new(),
            visit_count: 0,
            date_added: crate::util::now_ms(),
            expanded: Some(true),
            expanded_sidebar: Some(true),
            children: vec![],
        }
    }
    pub fn url(name: &str, url: &str) -> Self {
        Node { kind: Kind::Url, url: url.into(), expanded: None, expanded_sidebar: None, ..Self::folder(name) }
    }
    pub fn separator() -> Self {
        Node { kind: Kind::Separator, expanded: None, expanded_sidebar: None, ..Self::folder("") }
    }
}

/// Root folder keys, in the order Falkon displays them.
pub const ROOTS: [(&str, &str); 3] = [
    ("bookmark_bar", "Bookmarks Toolbar"),
    ("bookmark_menu", "Bookmarks Menu"),
    ("other", "Unsorted Bookmarks"),
];

pub struct Bookmarks {
    path: PathBuf,
    /// Synthetic root whose children are the three Falkon roots.
    pub root: Node,
    next_id: u64,
}

impl Bookmarks {
    pub fn load(path: PathBuf) -> Self {
        let mut roots: Vec<Node> = Vec::new();
        let parsed: Option<Value> = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok());
        let fresh = parsed.is_none();
        for (key, title) in ROOTS {
            let node = parsed
                .as_ref()
                .and_then(|v| v.get("roots")?.get(key).cloned())
                .and_then(|v| serde_json::from_value::<Node>(v).ok())
                .unwrap_or_else(|| Node::folder(title));
            roots.push(Node { name: title.into(), kind: Kind::Folder, ..node });
        }
        if fresh {
            roots[1].children = vec![
                Node::url("KDE Community", "https://kde.org/"),
                Node::url("KDE Apps", "https://apps.kde.org/"),
                Node::url("Firefox Add-ons", "https://addons.mozilla.org/firefox/"),
            ];
        }
        let mut root = Node::folder("root");
        root.children = roots;
        let mut b = Bookmarks { path, root, next_id: 1 };
        b.assign_ids();
        b
    }

    fn assign_ids(&mut self) {
        fn walk(n: &mut Node, next: &mut u64) {
            n.id = *next;
            *next += 1;
            for c in &mut n.children {
                walk(c, next);
            }
        }
        let mut next = 0;
        walk(&mut self.root, &mut next);
        self.next_id = next;
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let mut roots = serde_json::Map::new();
        for (i, (key, _)) in ROOTS.iter().enumerate() {
            let mut v = serde_json::to_value(&self.root.children[i])?;
            // Falkon always writes "children" for folders, even when empty.
            v["children"] = serde_json::to_value(&self.root.children[i].children)?;
            roots.insert((*key).into(), v);
        }
        let doc = json!({ "version": 1, "roots": roots });
        crate::util::write_atomic(&self.path, serde_json::to_string_pretty(&doc)?.as_bytes())?;
        Ok(())
    }

    pub fn toolbar_id(&self) -> u64 {
        self.root.children[0].id
    }
    pub fn menu_id(&self) -> u64 {
        self.root.children[1].id
    }
    pub fn unsorted_id(&self) -> u64 {
        self.root.children[2].id
    }

    pub fn find(&self, id: u64) -> Option<&Node> {
        fn f(n: &Node, id: u64) -> Option<&Node> {
            if n.id == id {
                return Some(n);
            }
            n.children.iter().find_map(|c| f(c, id))
        }
        f(&self.root, id)
    }

    fn find_mut(&mut self, id: u64) -> Option<&mut Node> {
        fn f(n: &mut Node, id: u64) -> Option<&mut Node> {
            if n.id == id {
                return Some(n);
            }
            n.children.iter_mut().find_map(|c| f(c, id))
        }
        f(&mut self.root, id)
    }

    /// (parent id, index) of a node.
    pub fn parent_of(&self, id: u64) -> Option<(u64, usize)> {
        fn f(n: &Node, id: u64) -> Option<(u64, usize)> {
            for (i, c) in n.children.iter().enumerate() {
                if c.id == id {
                    return Some((n.id, i));
                }
                if let Some(r) = f(c, id) {
                    return Some(r);
                }
            }
            None
        }
        f(&self.root, id)
    }

    pub fn insert(&mut self, parent: u64, index: Option<usize>, mut node: Node) -> anyhow::Result<u64> {
        fn renumber(n: &mut Node, next: &mut u64) {
            n.id = *next;
            *next += 1;
            for c in &mut n.children {
                renumber(c, next);
            }
        }
        let mut next = self.next_id;
        renumber(&mut node, &mut next);
        self.next_id = next;
        let id = node.id;
        let p = self.find_mut(parent).filter(|p| p.kind == Kind::Folder).ok_or_else(|| anyhow::anyhow!("no such folder"))?;
        let idx = index.unwrap_or(p.children.len()).min(p.children.len());
        p.children.insert(idx, node);
        self.save()?;
        Ok(id)
    }

    pub fn remove(&mut self, id: u64) -> anyhow::Result<Node> {
        if self.root.children.iter().any(|r| r.id == id) || id == self.root.id {
            anyhow::bail!("cannot remove a root folder");
        }
        let (parent, idx) = self.parent_of(id).ok_or_else(|| anyhow::anyhow!("no such bookmark"))?;
        let n = self.find_mut(parent).unwrap().children.remove(idx);
        self.save()?;
        Ok(n)
    }

    pub fn update(&mut self, id: u64, name: Option<&str>, url: Option<&str>, keyword: Option<&str>, description: Option<&str>) -> anyhow::Result<()> {
        let n = self.find_mut(id).ok_or_else(|| anyhow::anyhow!("no such bookmark"))?;
        if let Some(v) = name {
            n.name = v.into();
        }
        if let Some(v) = url {
            n.url = v.into();
        }
        if let Some(v) = keyword {
            n.keyword = v.into();
        }
        if let Some(v) = description {
            n.description = v.into();
        }
        self.save()
    }

    pub fn move_to(&mut self, id: u64, parent: u64, index: Option<usize>) -> anyhow::Result<()> {
        // refuse moving a folder into itself
        if let Some(n) = self.find(id) {
            if self.subtree_contains(n, parent) {
                anyhow::bail!("cannot move a folder into itself");
            }
        }
        let (old_parent, old_idx) = self.parent_of(id).ok_or_else(|| anyhow::anyhow!("no such bookmark"))?;
        let node = self.find_mut(old_parent).unwrap().children.remove(old_idx);
        let p = self.find_mut(parent).ok_or_else(|| anyhow::anyhow!("no such folder"))?;
        let mut idx = index.unwrap_or(p.children.len());
        if old_parent == parent && old_idx < idx {
            idx -= 1;
        }
        let idx = idx.min(p.children.len());
        p.children.insert(idx, node);
        self.save()
    }

    fn subtree_contains(&self, n: &Node, id: u64) -> bool {
        n.id == id || n.children.iter().any(|c| self.subtree_contains(c, id))
    }

    pub fn search(&self, text: &str) -> Vec<&Node> {
        let needle = text.to_lowercase();
        let mut out = vec![];
        fn walk<'a>(n: &'a Node, needle: &str, out: &mut Vec<&'a Node>) {
            if n.kind == Kind::Url
                && (n.name.to_lowercase().contains(needle) || n.url.to_lowercase().contains(needle) || n.keyword.to_lowercase() == needle)
            {
                out.push(n);
            }
            for c in &n.children {
                walk(c, needle, out);
            }
        }
        walk(&self.root, &needle, &mut out);
        out
    }

    pub fn find_by_url(&self, url: &str) -> Vec<&Node> {
        let mut out = vec![];
        fn walk<'a>(n: &'a Node, url: &str, out: &mut Vec<&'a Node>) {
            if n.kind == Kind::Url && n.url == url {
                out.push(n);
            }
            for c in &n.children {
                walk(c, url, out);
            }
        }
        walk(&self.root, url, &mut out);
        out
    }

    pub fn find_by_keyword(&self, kw: &str) -> Option<&Node> {
        fn walk<'a>(n: &'a Node, kw: &str) -> Option<&'a Node> {
            if n.kind == Kind::Url && !n.keyword.is_empty() && n.keyword == kw {
                return Some(n);
            }
            n.children.iter().find_map(|c| walk(c, kw))
        }
        walk(&self.root, kw)
    }

    /// JSON for QML / the WebExtension API (`bookmarks.BookmarkTreeNode`-like).
    pub fn to_json(&self, n: &Node, parent: Option<u64>, index: usize, recursive: bool) -> Value {
        let mut v = json!({
            "id": n.id.to_string(),
            "title": n.name,
            "type": match n.kind { Kind::Folder => "folder", Kind::Url => "bookmark", Kind::Separator => "separator" },
            "index": index,
            "dateAdded": n.date_added,
        });
        if let Some(p) = parent {
            v["parentId"] = p.to_string().into();
        }
        if n.kind == Kind::Url {
            v["url"] = n.url.clone().into();
            v["keyword"] = n.keyword.clone().into();
            v["description"] = n.description.clone().into();
        }
        if n.kind == Kind::Folder {
            v["children"] = if recursive {
                Value::Array(n.children.iter().enumerate().map(|(i, c)| self.to_json(c, Some(n.id), i, true)).collect())
            } else {
                Value::Array(vec![])
            };
        }
        v
    }

    pub fn node_json(&self, id: u64, recursive: bool) -> Option<Value> {
        let n = self.find(id)?;
        let (parent, index) = self.parent_of(id).map(|(p, i)| (Some(p), i)).unwrap_or((None, 0));
        Some(self.to_json(n, parent, index, recursive))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn falkon_roundtrip_and_ops() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("bookmarks.json");
        std::fs::write(&p, r#"{"roots":{"bookmark_bar":{"children":[{"type":"url","name":"KDE","url":"https://kde.org/","keyword":"kde","description":"","visit_count":3}],"type":"folder","name":"x","expanded":true},"bookmark_menu":{"children":[],"type":"folder"},"other":{"children":[],"type":"folder"}},"version":1}"#).unwrap();
        let mut b = Bookmarks::load(p.clone());
        let tb = b.toolbar_id();
        assert_eq!(b.find(tb).unwrap().children[0].url, "https://kde.org/");
        assert_eq!(b.find_by_keyword("kde").unwrap().name, "KDE");
        let f = b.insert(tb, None, Node::folder("Dev")).unwrap();
        let rust = b.insert(f, None, Node::url("Rust", "https://rust-lang.org/")).unwrap();
        assert!(b.move_to(f, f, None).is_err());
        b.move_to(rust, b.unsorted_id(), Some(0)).unwrap();
        assert_eq!(b.parent_of(rust).unwrap().0, b.unsorted_id());
        assert!(b.remove(tb).is_err());
        b.remove(f).unwrap();
        let b2 = Bookmarks::load(p);
        assert_eq!(b2.search("rust").len(), 1);
        assert_eq!(b2.find(b2.toolbar_id()).unwrap().children.len(), 1);
    }
}
