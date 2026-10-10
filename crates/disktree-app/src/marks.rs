//! The set of paths marked for removal.
//!
//! Marks are keyed by absolute path, not by tree position: the tree is
//! re-scanned after a removal, and a mark must survive that (or be reported as
//! gone) rather than silently pointing at a different node.

use std::path::{Path, PathBuf};

use disktree_core::removal::{Target, addressable, entry_snapshot};
use disktree_core::tree::Node;
use rustc_hash::FxHashSet;

/// Marked paths, in the order they were marked.
#[derive(Debug, Default)]
pub struct Marks {
    items: Vec<Target>,
    index: FxHashSet<PathBuf>,
}

impl Marks {
    pub fn contains(&self, path: &Path) -> bool {
        self.index.contains(path)
    }

    pub fn items(&self) -> &[Target] {
        &self.items
    }

    pub const fn len(&self) -> usize {
        self.items.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Mark `target`, or unmark it when it is already marked.
    ///
    /// Returns `true` when the path ended up marked.
    pub fn toggle(&mut self, target: Target) -> bool {
        if self.index.remove(&target.path) {
            self.items.retain(|item| item.path != target.path);
            false
        } else {
            self.index.insert(target.path.clone());
            self.items.push(target);
            true
        }
    }

    pub fn remove(&mut self, path: &Path) {
        if self.index.remove(path) {
            self.items.retain(|item| item.path != path);
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.index.clear();
    }

    /// Re-read sizes from a freshly scanned tree and drop marks whose entry or
    /// directory scope changed, preserving what the original mark authorized.
    pub fn refresh(&mut self, root_path: &Path, root: &Node) {
        self.items.retain_mut(|item| {
            if let Some(marked) = item.identity {
                let matches = entry_snapshot(&item.path).is_some_and(|entry| {
                    entry.identity == marked
                        && entry.kind.is_dir() == item.is_dir
                });
                if !matches {
                    return false;
                }
            }
            let Some(node) = find(root_path, root, &item.path) else {
                // A scan says nothing about entries outside its root.
                // They stay marked but the plan keeps them back.
                return item.path.strip_prefix(root_path).is_err();
            };
            if node.is_dir() != item.is_dir {
                return false;
            }
            item.bytes = node.bytes;
            item.hidden = is_hidden(&item.path);
            true
        });
        self.index = self.items.iter().map(|item| item.path.clone()).collect();
    }
}

/// Walk the tree to the node at an absolute path. Path components are compared
/// one at a time, so a name containing a path separator cannot confuse it.
pub fn find<'a>(
    root_path: &Path,
    root: &'a Node,
    path: &Path,
) -> Option<&'a Node> {
    if !addressable(path) {
        return None;
    }
    let relative = path.strip_prefix(root_path).ok()?;
    let mut node = root;
    for component in relative.components() {
        let name = component.as_os_str().to_string_lossy();
        node = node
            .children
            .iter()
            .find(|child| child.name.as_ref() == name)?;
    }
    Some(node)
}

/// A dotfile or dot-directory by name.
pub fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with('.'))
}

/// Shorten a path for display: `~` for the home directory, and the path with
/// the home prefix replaced when it is below it. The separator after `~` is
/// the platform's, so Windows shows `~\AppData\Local`, not `~/AppData\Local`.
pub fn display_path(path: &Path, home: Option<&Path>) -> String {
    match home
        .and_then(|home| path.strip_prefix(home).ok().map(|rest| (home, rest)))
    {
        Some((_, rest)) if rest.as_os_str().is_empty() => "~".to_string(),
        Some((_, rest)) => {
            format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display())
        }
        None => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use disktree_core::removal::entry_identity;
    use disktree_core::tree::{Metric, NodeKind, aggregate};

    fn file(name: &str, bytes: u64) -> Node {
        Node::entry(name, NodeKind::File, bytes)
    }

    fn tree() -> Node {
        let mut root = Node::directory("home");
        let mut cache = Node::directory(".cache");
        cache.children.push(file("blob.bin", 900));
        root.children.push(cache);
        root.children.push(file("notes.bin", 100));
        aggregate(&mut root, Metric::Bytes);
        root
    }

    fn target(path: &str, bytes: u64) -> Target {
        Target {
            path: PathBuf::from(path),
            bytes,
            is_dir: false,
            hidden: false,
            identity: None,
        }
    }

    #[test]
    fn toggling_marks_and_unmarks() {
        let mut marks = Marks::default();
        assert!(marks.toggle(target("/home/tobi/a", 1)));
        assert!(marks.contains(Path::new("/home/tobi/a")));
        assert_eq!(marks.len(), 1);
        assert!(!marks.toggle(target("/home/tobi/a", 1)));
        assert!(marks.is_empty());
    }

    #[test]
    fn marking_the_same_path_twice_keeps_one_entry() {
        let mut marks = Marks::default();
        marks.toggle(target("/home/tobi/a", 1));
        marks.toggle(target("/home/tobi/a", 1));
        marks.toggle(target("/home/tobi/a", 1));
        assert_eq!(marks.len(), 1);
    }

    #[test]
    fn refresh_re_reads_sizes_from_a_new_tree() {
        let root_path = Path::new("/home/tobi");
        let root = tree();
        let mut marks = Marks::default();
        let mut cache = target("/home/tobi/.cache", 0);
        cache.is_dir = true;
        marks.toggle(cache);
        marks.toggle(target("/home/tobi/gone", 500));

        marks.refresh(root_path, &root);
        assert_eq!(marks.items()[0].bytes, 900);
        assert!(marks.items()[0].is_dir);
        assert!(marks.items()[0].hidden, "the .cache mark is hidden");
        assert_eq!(marks.len(), 1, "a missing path loses its mark");
        assert!(!marks.contains(&root_path.join("gone")));
    }

    #[test]
    fn a_disappeared_mark_does_not_authorize_a_recreated_path() {
        let root_path = Path::new("/home/tobi");
        let mut marks = Marks::default();
        let marked = root_path.join("notes.bin");
        marks.toggle(target("/home/tobi/notes.bin", 100));
        marks.refresh(root_path, &Node::directory("home"));
        assert!(!marks.contains(&marked));
        marks.refresh(root_path, &tree());
        assert!(marks.is_empty(), "a new entry is never marked implicitly");
        assert!(marks.toggle(target("/home/tobi/notes.bin", 100)));
    }

    #[test]
    fn a_replaced_entry_loses_its_mark_even_when_the_name_stays() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("notes.bin");
        std::fs::write(&path, b"original").expect("write");
        let mut marked = target(path.to_str().expect("text path"), 8);
        marked.identity = Some(entry_identity(&path).expect("identity"));
        let mut marks = Marks::default();
        marks.toggle(marked);
        // Keep the original alive so the filesystem cannot reuse its id.
        std::fs::rename(&path, temp.path().join("original.bin")).expect("move");
        std::fs::write(&path, b"replacement").expect("write");
        let mut root = Node::directory("root");
        root.children.push(file("notes.bin", 11));

        marks.refresh(temp.path(), &root);
        assert!(marks.is_empty());
        assert!(!marks.contains(&path));
    }

    #[test]
    fn refresh_drops_a_mark_whose_directory_scope_changed() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("entry");
        std::fs::create_dir(&path).expect("mkdir");
        let keep = path.join("keep.bin");
        std::fs::write(&keep, b"keep").expect("write");
        let identity = entry_identity(&path).expect("identity");
        let mut marked = target(path.to_str().expect("text path"), 0);
        marked.identity = Some(identity);
        let mut marks = Marks::default();
        marks.toggle(marked);
        let mut root = Node::directory("root");
        let mut entry = Node::directory("entry");
        entry.children.push(file("keep.bin", 4));
        root.children.push(entry);
        aggregate(&mut root, Metric::Bytes);

        marks.refresh(temp.path(), &root);

        assert_eq!(entry_identity(&path), Some(identity));
        assert!(marks.is_empty());
        assert!(!marks.contains(&path));
        assert_eq!(std::fs::read(keep).expect("read"), b"keep");
    }

    #[test]
    fn refresh_never_changes_a_marks_directory_scope_from_scan_data() {
        let root_path = Path::new("/home/tobi");
        let marked = root_path.join(".cache");
        let mut marks = Marks::default();
        marks.toggle(target("/home/tobi/.cache", 0));

        marks.refresh(root_path, &tree());

        assert!(marks.is_empty());
        assert!(!marks.contains(&marked));
    }

    #[test]
    fn refresh_keeps_verified_marks_outside_the_new_scan() {
        let temp = tempfile::tempdir().expect("tempdir");
        let old = temp.path().join("old");
        let new = temp.path().join("new");
        std::fs::create_dir(&old).expect("mkdir");
        std::fs::create_dir(&new).expect("mkdir");
        let path = old.join("notes.bin");
        std::fs::write(&path, b"original").expect("write");
        let mut item = target(path.to_str().expect("text path"), 8);
        item.identity = Some(entry_identity(&path).expect("identity"));
        let mut marks = Marks::default();
        marks.toggle(item);

        marks.refresh(&new, &Node::directory("root"));
        assert_eq!(marks.len(), 1);
        assert!(marks.contains(&path));
        assert_eq!(marks.items()[0].bytes, 8);
        let planned = disktree_core::removal::plan(marks.items(), &new);
        assert!(planned.is_empty());
        assert!(planned.blocked[0].reason.contains("outside"));
        std::fs::rename(&path, old.join("original.bin")).expect("move");
        std::fs::write(&path, b"replacement").expect("write");
        marks.refresh(&new, &Node::directory("root"));
        assert!(marks.is_empty(), "a replacement still loses its mark");
        assert!(!marks.contains(&path));
    }

    #[test]
    fn find_matches_whole_components_only() {
        let root = tree();
        assert!(
            find(
                Path::new("/home/tobi"),
                &root,
                Path::new("/home/tobi/.cache")
            )
            .is_some()
        );
        assert!(
            find(
                Path::new("/home/tobi"),
                &root,
                Path::new("/home/tobi/.cache/blob.bin")
            )
            .is_some()
        );
        assert!(
            find(
                Path::new("/home/tobi"),
                &root,
                Path::new("/home/tobi/cache")
            )
            .is_none()
        );
        assert!(
            find(
                Path::new("/elsewhere"),
                &root,
                Path::new("/home/tobi/notes.bin")
            )
            .is_none()
        );
    }

    #[test]
    fn display_path_shortens_the_home_prefix() {
        let home = Path::new("/home/tobi");
        let separator = std::path::MAIN_SEPARATOR;
        assert_eq!(
            display_path(&home.join(".cache").join("npm"), Some(home)),
            format!("~{separator}.cache{separator}npm")
        );
        assert_eq!(display_path(home, Some(home)), "~");
        assert_eq!(display_path(Path::new("/var/log"), Some(home)), "/var/log");
        assert_eq!(display_path(Path::new("/var/log"), None), "/var/log");
    }
}
