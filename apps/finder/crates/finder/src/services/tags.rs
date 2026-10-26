//! Persistent file tags stored under the Linux XDG data directory.
//! Renames and moves performed by Finder update the stored paths.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::model::tag::Tag;

fn key(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

#[derive(Default)]
pub struct Tags {
    /// By case-preserving path: the path as it was tagged, and its tags in order.
    items: HashMap<String, (PathBuf, Vec<Tag>)>,
    file: Option<PathBuf>,
}

impl Tags {
    pub fn load() -> Tags {
        let file = Some(crate::services::xdg::data_dir().join("tags.txt"));
        let mut tags = Tags {
            items: HashMap::new(),
            file: file.clone(),
        };
        if let Some(text) = file.and_then(|f| std::fs::read_to_string(f).ok()) {
            for line in text.lines() {
                let Some((path, list)) = line.split_once('\t') else {
                    continue;
                };
                let list: Vec<Tag> = list.split(',').filter_map(Tag::from_name).collect();
                if !list.is_empty() {
                    let path = PathBuf::from(path);
                    tags.items.insert(key(&path), (path, list));
                }
            }
        }
        tags
    }

    fn save(&self) {
        let Some(file) = &self.file else { return };
        let mut lines: Vec<String> = self
            .items
            .values()
            .map(|(p, list)| {
                format!(
                    "{}\t{}",
                    p.display(),
                    list.iter().map(|t| t.name()).collect::<Vec<_>>().join(",")
                )
            })
            .collect();
        lines.sort();
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(file, lines.join("\n") + "\n");
    }

    pub fn of(&self, p: &Path) -> &[Tag] {
        self.items
            .get(&key(p))
            .map_or(&[], |(_, list)| list.as_slice())
    }

    /// Toggles `tag` on `paths`: off all of them if they all have it, else
    /// on each one that doesn't.
    pub fn toggle(&mut self, paths: &[PathBuf], tag: Tag) {
        let all = !paths.is_empty() && paths.iter().all(|p| self.of(p).contains(&tag));
        for p in paths {
            let entry = self
                .items
                .entry(key(p))
                .or_insert_with(|| (p.clone(), Vec::new()));
            if all {
                entry.1.retain(|t| *t != tag);
            } else if !entry.1.contains(&tag) {
                entry.1.push(tag);
            }
        }
        self.items.retain(|_, (_, list)| !list.is_empty());
        self.save();
    }

    /// `from` (and, if a folder, everything in it) is now at `to`.
    pub fn moved(&mut self, from: &Path, to: &Path) {
        let (old, new) = (key(from), to.to_path_buf());
        let prefix = format!("{old}{}", std::path::MAIN_SEPARATOR);
        let moving: Vec<String> = self
            .items
            .keys()
            .filter(|k| **k == old || k.starts_with(&prefix))
            .cloned()
            .collect();
        if moving.is_empty() {
            return;
        }
        for k in moving {
            let (path, list) = self.items.remove(&k).expect("listed");
            let full = path.to_string_lossy().into_owned();
            let rest = full
                .get(from.to_string_lossy().len()..)
                .unwrap_or_default()
                .to_string();
            let path = PathBuf::from(format!("{}{rest}", new.display()));
            self.items.insert(key(&path), (path, list));
        }
        self.save();
    }

    /// Everything tagged `tag`, that still exists.
    pub fn tagged(&self, tag: Tag) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = self
            .items
            .values()
            .filter(|(p, list)| list.contains(&tag) && p.exists())
            .map(|(p, _)| p.clone())
            .collect();
        v.sort();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Tags {
        Tags {
            items: HashMap::new(),
            file: None,
        }
    }

    #[test]
    fn toggle_adds_then_removes() {
        let mut t = store();
        let a = PathBuf::from("/tmp/x/a.txt");
        let b = PathBuf::from("/tmp/x/b.txt");
        t.toggle(&[a.clone()], Tag::Red);
        assert_eq!(t.of(&a), &[Tag::Red]);
        // Not all of them have it: tag the rest.
        t.toggle(&[a.clone(), b.clone()], Tag::Red);
        assert_eq!(t.of(&b), &[Tag::Red]);
        // All of them do: untag all.
        t.toggle(&[a.clone(), b.clone()], Tag::Red);
        assert!(t.of(&a).is_empty() && t.of(&b).is_empty());
    }

    #[test]

    fn linux_tags_preserve_case_and_follow_directory_moves() {
        let mut t = store();
        t.toggle(
            &[
                PathBuf::from("/tmp/Dir/Inner/f.txt"),
                PathBuf::from("/tmp/Directory"),
            ],
            Tag::Blue,
        );
        assert!(t.of(Path::new("/tmp/dir/Inner/f.txt")).is_empty());
        t.moved(Path::new("/tmp/Dir"), Path::new("/tmp/Moved"));
        assert_eq!(t.of(Path::new("/tmp/Moved/Inner/f.txt")), &[Tag::Blue]);
        assert_eq!(t.of(Path::new("/tmp/Directory")), &[Tag::Blue]);
        assert!(t.of(Path::new("/tmp/Dir/Inner/f.txt")).is_empty());
    }
}
