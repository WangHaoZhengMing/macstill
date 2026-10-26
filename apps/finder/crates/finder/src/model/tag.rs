//! File tag identities, independent of UI rendering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tag {
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
    Gray,
}

impl Tag {
    pub const ALL: [Tag; 7] = [
        Tag::Red,
        Tag::Orange,
        Tag::Yellow,
        Tag::Green,
        Tag::Blue,
        Tag::Purple,
        Tag::Gray,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Tag::Red => "Red",
            Tag::Orange => "Orange",
            Tag::Yellow => "Yellow",
            Tag::Green => "Green",
            Tag::Blue => "Blue",
            Tag::Purple => "Purple",
            Tag::Gray => "Gray",
        }
    }

    pub(crate) fn from_name(s: &str) -> Option<Tag> {
        Tag::ALL
            .into_iter()
            .find(|t| t.name().eq_ignore_ascii_case(s))
    }
}
