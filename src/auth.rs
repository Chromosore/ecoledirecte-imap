use imap_codec::imap_types::{
    core::Tag,
    response::{Code, Response, Status},
    secret::Secret,
    state::State,
};
use std::str;

use crate::capabilities;

#[derive(Clone)]
pub struct User {
    pub id: u32,
    pub token: String,
}

pub fn parse_plain_message<'a, 'b>(
    secret: Secret<&'a [u8]>,
    tag: Tag<'b>,
) -> Result<(&'a str, &'a str), Vec<Response<'b>>> {
    let parts: Vec<_> = secret.declassify().split(|c| *c == 0).collect();
    if parts.len() != 3 {
        return Err(vec![Response::Status(
            Status::no(Some(tag), None, "Invalid challenge string").unwrap(),
        )]);
    }

    let identity = parts[0];
    let username = parts[1];
    let password = parts[2];

    if identity != "".as_bytes() && identity != username {
        return Err(vec![Response::Status(
            Status::no(Some(tag), None, "Invalid identity").unwrap(),
        )]);
    }

    match (str::from_utf8(username), str::from_utf8(password)) {
        (Ok(u), Ok(p)) => Ok((u, p)),
        _ => Err(vec![Response::Status(
            Status::no(Some(tag), None, "Challenge must be valid UTF-8").unwrap(),
        )]),
    }
}

// Pas sûr de comment il faut nommer cette fonction puisqu'elle ne fait que
// traduire le résultat de l'API en action concrètes dans le système.
pub fn translate(
    authentification_result: Result<(u32, String), Option<String>>,
    tag: Tag,
) -> (State<'static>, Option<User>, Vec<Response<'_>>) {
    match authentification_result {
        Ok((id, token)) => (
            State::Authenticated,
            Some(User { id, token }),
            vec![Response::Status(
                Status::ok(
                    Some(tag),
                    Some(Code::Capability(capabilities())),
                    "Authentication completed",
                )
                .unwrap(),
            )],
        ),
        Err(message) => (
            State::NotAuthenticated,
            None,
            vec![Response::Status(
                Status::no(
                    Some(tag),
                    None,
                    match message {
                        Some(message) => format!("Authentication failed: {}", message),
                        None => String::from("Authentication failed"),
                    },
                )
                .unwrap(),
            )],
        ),
    }
}
