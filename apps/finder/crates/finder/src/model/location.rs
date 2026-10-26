//! Typed browser destinations; virtual locations are never filesystem paths.
use crate::model::tag::Tag;
use std::path::{Path, PathBuf};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Location {
    AirDrop,
    Applications,
    Folder(PathBuf),
    Trash,
    Search {
        root: PathBuf,
        query: String,
        folder_scope: bool,
    },
    Tag {
        root: PathBuf,
        tag: Tag,
    },
}
impl From<PathBuf> for Location {
    fn from(path: PathBuf) -> Self {
        Self::Folder(path)
    }
}
impl Location {
    pub fn directory(&self) -> Option<&Path> {
        match self {
            Self::Folder(path) => Some(path),
            Self::Search { root, .. } | Self::Tag { root, .. } => Some(root),
            Self::Trash | Self::AirDrop | Self::Applications => None,
        }
    }
    pub fn sidebar_matches(&self, target: &Self) -> bool {
        match (self, target) {
            (Self::Applications, Self::Applications) => true,
            (Self::Applications, _) | (_, Self::Applications) => false,
            (Self::AirDrop, Self::AirDrop) => true,
            (Self::AirDrop, _) | (_, Self::AirDrop) => false,
            (Self::Trash, Self::Trash) => true,
            (Self::Tag { .. }, _) | (Self::Trash, _) | (_, Self::Trash) => false,
            _ => self.directory() == target.directory(),
        }
    }
    pub fn is_trash(&self) -> bool {
        matches!(self, Self::Trash)
    }
    pub fn capabilities(&self) -> Capabilities {
        Capabilities {
            create: matches!(self, Self::Folder(_)),
            paste: matches!(self, Self::Folder(_)),
            rename: !self.is_trash() && !matches!(self, Self::AirDrop | Self::Applications),
            duplicate: !self.is_trash() && !matches!(self, Self::AirDrop | Self::Applications),
            copy: !self.is_trash() && !matches!(self, Self::AirDrop | Self::Applications),
            trash: !self.is_trash() && !matches!(self, Self::AirDrop | Self::Applications),
            open: !self.is_trash() && !matches!(self, Self::AirDrop),
            restore: self.is_trash(),
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Capabilities {
    pub create: bool,
    pub paste: bool,
    pub rename: bool,
    pub duplicate: bool,
    pub copy: bool,
    pub trash: bool,
    pub open: bool,
    pub restore: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn applications_are_read_only_launchable_and_distinct() {
        let app = Location::Applications;
        assert_eq!(app.directory(), None);
        assert!(app.sidebar_matches(&Location::Applications));
        assert!(!app.sidebar_matches(&Location::Trash));
        assert!(!app.sidebar_matches(&Location::Folder("/usr/share/applications".into())));
        let c = app.capabilities();
        assert!(c.open);
        assert!(
            !c.create && !c.paste && !c.rename && !c.duplicate && !c.copy && !c.trash && !c.restore
        );
    }
    #[test]
    fn virtual_locations_never_masquerade_as_filesystem_paths() {
        assert_eq!(Location::Trash.directory(), None);
        let caps = Location::Trash.capabilities();
        assert!(caps.restore);
        assert!(
            !caps.create
                && !caps.paste
                && !caps.rename
                && !caps.duplicate
                && !caps.copy
                && !caps.trash
                && !caps.open
        );
    }
    #[test]
    fn search_and_tags_operate_on_results_without_accepting_folder_writes() {
        for location in [
            Location::Search {
                root: "/home/u".into(),
                query: "report".into(),
                folder_scope: true,
            },
            Location::Tag {
                root: "/home/u".into(),
                tag: Tag::Blue,
            },
        ] {
            assert_eq!(location.directory(), Some(Path::new("/home/u")));
            let caps = location.capabilities();
            assert!(caps.copy && caps.open && caps.trash && caps.rename && caps.duplicate);
            assert!(!caps.create && !caps.paste && !caps.restore);
        }
    }
    #[test]
    fn sidebar_distinguishes_trash_and_tags_from_their_previous_folder() {
        let home = Location::Folder("/home/u".into());
        assert!(!Location::Trash.sidebar_matches(&home));
        assert!(Location::Trash.sidebar_matches(&Location::Trash));
        assert!(
            !Location::Tag {
                root: "/home/u".into(),
                tag: Tag::Red
            }
            .sidebar_matches(&home)
        );
        assert!(
            Location::Search {
                root: "/home/u".into(),
                query: "report".into(),
                folder_scope: false
            }
            .sidebar_matches(&home)
        );
    }
}
