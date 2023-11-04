pub mod api;
pub mod auth;
pub mod mailbox;

use imap_codec::imap_types::{core::NonEmptyVec, response::Capability};

pub fn capabilities() -> NonEmptyVec<Capability<'static>> {
    use imap_codec::imap_types::{auth::AuthMechanism::*, response::Capability::*};
    NonEmptyVec::try_from(vec![Imap4Rev1, Auth(Plain)]).unwrap()
}
