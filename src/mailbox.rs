use chrono::{Datelike, Local};
use imap_codec::imap_types::{
    flag::{Flag, FlagPerm},
    mailbox::Mailbox,
    response::{Code, Data, Response, Status},
};
use serde_json::Value;
use std::collections::HashMap;

pub fn make_folders(folders: Vec<(String, u32)>) -> HashMap<String, u32> {
    let mut map: HashMap<_, _> = folders.into_iter().collect();
    map.insert("INBOX".into(), 0);
    map.insert("Sent".into(), 0);
    map.insert("Archived".into(), 0);
    map.insert("Drafts".into(), 0);
    map
}

pub fn filter<'a>(
    folders: &'a HashMap<String, u32>,
    reference: Mailbox<'_>,
    mailbox_wildcard: &[u8],
) -> Vec<Response<'a>> {
    folders // TODO!!!
        .keys()
        .map(|folder| {
            Response::Data(Data::List {
                items: vec![],
                delimiter: None,
                mailbox: <Mailbox as TryFrom<&str>>::try_from(folder).unwrap(),
            })
        })
        .collect()
}

pub fn mailbox_info<'a, 'b>(mailbox: &'a str, folder: Value) -> Vec<Response<'b>> {
    let existing_messages_count = match mailbox {
        "Sent" => &folder["pagination"]["messagesEnvoyesCount"],
        "Archived" => &folder["pagination"]["messagesArchivesCount"],
        "Drafts" => &folder["pagination"]["messagesDraftCount"],
        _ => &folder["pagination"]["messagesRecusCount"],
    }
    .as_u64()
    .unwrap() as u32;

    let unseen_messages_count = match mailbox {
        "Sent" => None,
        "Archived" => None,
        "Drafts" => None,
        _ => folder["pagination"]["messagesRecusNotReadCount"].as_u64(),
    };

    let date = Local::now().date_naive();
    // unwrap: Normalement on est après l'an 0
    let school_year: u32 = match date.month() {
        1..=8 => date.year() - 1,
        9..=12 => date.year(),
        _ => panic!("Month must be in the 1..=12 range"),
    }
    .try_into()
    .unwrap();

    let mut response = vec![
        Response::Data(Data::Flags(vec![Flag::Seen, Flag::Answered])),
        Response::Data(Data::Exists(existing_messages_count)),
        Response::Data(Data::Recent(0)),
        Response::Status(
            Status::ok(
                None,
                Some(Code::PermanentFlags(vec![FlagPerm::Flag(Flag::Seen)])),
                "Flags",
            )
            .unwrap(),
        ),
        Response::Status(
            Status::ok(
                None,
                Some(Code::UidValidity(school_year.try_into().unwrap())),
                format!("Valide en {}-{}", school_year, school_year + 1),
            )
            .unwrap(),
        ),
    ];

    if let Some(count) = unseen_messages_count {
        let count = count as u32;
        if let Ok(count) = count.try_into() {
            response.push(Response::Status(
                Status::ok(None, Some(Code::Unseen(count)), "Unseen").unwrap(),
            ));
        }
    }

    response
}
