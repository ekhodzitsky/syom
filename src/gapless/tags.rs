//! A recognised tag may be unusable, but must still count toward ambiguity.
use super::Delay;

pub(crate) struct Tag {
    pub(crate) delay: Option<Delay>,
}

#[derive(Default)]
pub(crate) enum Tags {
    #[default]
    Missing,
    Single(Tag),
    Ambiguous,
}

impl Tags {
    pub(crate) fn add(&mut self, tag: Tag) {
        *self = match self {
            Self::Missing => Self::Single(tag),
            _ => Self::Ambiguous,
        };
    }

    pub(crate) fn delay(self) -> Option<Delay> {
        match self {
            Self::Single(tag) => tag.delay,
            _ => None,
        }
    }
}
