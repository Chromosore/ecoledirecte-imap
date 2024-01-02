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

pub fn translate_to_mailbox<'a>(mailbox: &'a str, folder: Value) {
    let messages = match mailbox {
        "Sent" => &folder["messages"]["sent"],
        "Archived" => &folder["messages"]["archived"],
        "Drafts" => &folder["messages"]["draft"],
        _ => &folder["messages"]["received"],
    }
}

pub fn to_folder<'a>(
    folders: &HashMap<String, u32>, mailbox: Mailbox<'_>) -> Option<(u32, &'static str)> {
    let name = match mailbox {
        Mailbox::Inbox => "INBOX",
        Mailbox::Other(mailbox) => str::from_utf8(mailbox.as_ref()).unwrap(),
    };
    match name {
        "INBOX" => Some((0, "received")),
        "Sent" => Some((0, "sent")),
        "Archived" => Some((0, "archived")),
        "Drafts" => Some((0, "draft")),
        _ => folders.get(name).map(|id| (id, "received")),
    }
}

pub fn paginate(min: u32, max: u32) -> (u32, u32) {
    // Le problème ici est de déterminer la plus petite taille
    // de page possible pour avoir une seule page qui contient
    // tous les messages de min à max (inclus).
    // Exemple : de 17 à 23, on trouve que la plus petite taille
    // qui satisfait les contraintes est 8 : la page 3 contient
    // les messages de 17 à 24.
    // J'ai essayé de résoudre le problème d'un point de vu
    // mathématique, sans trop de succès. Ce qui ressort,
    // c'est que dans tous les cas, utiliser un page de taille
    // max contiendra tous les messages de 1 à max et donc de
    // min à max, mais c'est un peu du gachis. Cela dit on est
    // obligé d'utiliser cette technique si max >= 2*min + 1
    // Sinon la taille minimum de la page doit être évidemment
    // max - min + 1 (le nombre de messages dans l'intervalle
    // min..max).
    // Une formalisation du problème si des gens sont éventuel-
    // lement tentés de le résoudre est :
    // Soient a et b deux entiers (min et max) avec 1 <= a <= b
    // On cherche la plus petite valeur de n (taille de la page)
    // telle qu'il existe un entier p (numéro de la page) tel que
    // n*(p-1) + 1 <= a <= b <= n*p
    // Pour l'instant, la solution consiste en vérifier si les entiers
    // de (b - a + 1) à b fonctionnent. Comme dit précédemment, b
    // fonctionne forcément.
    // Donc complexité O(min)
    for size in (max - min + 1)..max {
        // max <= n*p
        let page = ((max - 1) / size) + 1;
        if size * (page - 1) + 1 <= min && max <= size * page {
            return (page, size);
        }
    }
    (1, max)
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_pagination() {
        assert_eq!(paginate(5, 5), (5, 1));
        assert_eq!(paginate(1, 20), (1, 20));
        assert_eq!(paginate(11, 20), (2, 10));
        assert_eq!(paginate(5, 8), (2, 4));
        assert_eq!(paginate(17, 23), (3, 8));
        assert_eq!(paginate(5, 20), (1, 20));
    }
}
