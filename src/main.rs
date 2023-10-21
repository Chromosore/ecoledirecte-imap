use imap_codec::{
    decode::{AuthenticateDataDecodeError, CommandDecodeError, Decoder},
    encode::Encoder,
    imap_types::{
        auth::AuthMechanism,
        command::Command,
        core::{NonEmptyVec, Text},
        response::{
            Capability, Code, CommandContinuationRequest, Data, Greeting, GreetingKind, Response,
            Status,
        },
        state::State,
    },
    AuthenticateDataCodec, CommandCodec, GreetingCodec, ResponseCodec,
};
use reqwest::header::USER_AGENT;
use serde::Deserialize;
use serde_json::{json, Value};
use std::borrow::Cow;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::ops::Range;
use std::str;
use std::thread;

struct Connection<'a> {
    state: State<'a>,
    token: Option<String>,
}

impl<'a> Default for Connection<'a> {
    fn default() -> Connection<'a> {
        Connection {
            state: State::Greeting,
            token: None,
        }
    }
}

#[derive(Deserialize, Debug)]
struct APIResponse {
    host: Option<String>,
    code: u32,
    token: String,
    message: Option<String>,
    data: Value,
}

fn capabilities() -> NonEmptyVec<Capability<'static>> {
    use imap_codec::imap_types::{auth::AuthMechanism::*, response::Capability::*};
    NonEmptyVec::try_from(vec![Imap4Rev1, Auth(Plain)]).unwrap()
}

fn login(username: &str, password: &str) -> Result<String, Option<String>> {
    let body = json!({
        "identifiant": username,
        "motdepasse": password,
    });

    let client = reqwest::blocking::Client::new();
    let response: APIResponse = client
        .post("https://api.ecoledirecte.com/v3/login.awp?v=4.43.0")
        .header(USER_AGENT, "ecoledirecte-imap")
        .body("data=".to_owned() + &body.to_string())
        .send()
        .unwrap()
        .json()
        .unwrap();

    if response.code == 200 {
        Ok(response.token)
    } else {
        Err(response.message)
    }
}

fn process<'a>(
    command: Command<'a>,
    connection: &mut Connection<'_>,
    stream: &mut TcpStream,
) -> Vec<Response<'a>> {
    use imap_codec::imap_types::{
        command::CommandBody::*,
        command::CommandBody::{Logout, Status as StatusCommand},
        response::Status,
        state::State::{Logout as LogoutState, *},
    };

    match command.body {
        Capability => {
            return vec![
                Response::Data(Data::Capability(capabilities())),
                Response::Status(
                    Status::ok(Some(command.tag), None, "CAPABILITY completed").unwrap(),
                ),
            ]
        }
        Noop => {
            return vec![Response::Status(
                Status::ok(Some(command.tag), None, "NOOP completed").unwrap(),
            )]
        }
        Logout => {
            connection.state = LogoutState;
            return vec![
                Response::Status(Status::bye(None, "Logging out!").unwrap()),
                Response::Status(Status::ok(Some(command.tag), None, "LOGOUT completed").unwrap()),
            ];
        }
        _ => (),
    }

    if connection.state == NotAuthenticated {
        match command.body {
            Authenticate {
                mechanism,
                initial_response,
            } => {
                if mechanism != AuthMechanism::Plain {
                    return vec![Response::Status(
                        Status::no(Some(command.tag), None, "Unsupported mechanism").unwrap(),
                    )];
                }
                if initial_response != None {
                    return vec![Response::Status(
                        Status::no(Some(command.tag), None, "Unexpected initial response").unwrap(),
                    )];
                }

                stream
                    .write(
                        &ResponseCodec::default()
                            .encode(&Response::CommandContinuationRequest(
                                CommandContinuationRequest::Base64(Cow::Borrowed(&[])),
                            ))
                            .dump(),
                    )
                    .unwrap();

                let mut buffer = [0u8; 1024];
                let mut consummed = 0;
                let mut peeked;

                peeked = stream.peek(&mut buffer).unwrap();

                // Le problème est de consommer juste la bonne quantité de données
                // pour que le reste soit géré par la boucle principale
                // On pourrait utiliser juste peek puis consommer ce qui a été con-
                // sommé par le codec mais le problème est que peek ne bloque pas
                // et donc on attendrait en boucle quand il manque des données.
                // La solution: peek pour obtenir des données (puisque quand il n'y
                // a pas de données disponibles peek bloque) puis si c'est pas suf-
                // fisant, on read() les données pour les consommer (puisqu'on sait
                // qu'on les utilise de toute manière) et on peek à nouveau.
                let line = loop {
                    match AuthenticateDataCodec::default().decode(&buffer[..peeked]) {
                        Ok((remaining, line)) => {
                            // unwrap: ok puisque remaining est une slice de buffer
                            let range = remaining.as_range_of(&buffer).unwrap();
                            // unwrap: ok puisque déjà peeked
                            stream.read(&mut buffer[consummed..range.start]).unwrap();
                            break line;
                        }
                        Err(AuthenticateDataDecodeError::Incomplete) => {
                            if peeked >= buffer.len() {
                                todo!("OUT OF MEMORY");
                            }
                            // unwrap: ok puisque déjà peeked
                            stream.read(&mut buffer[consummed..peeked]).unwrap();
                            consummed = peeked;
                            let received = stream.peek(&mut buffer[consummed..]).unwrap();
                            if received == 0 {
                                return vec![];
                            }
                            peeked += received;
                        }
                        Err(AuthenticateDataDecodeError::Failed) => {
                            stream.read(&mut buffer[consummed..peeked]).unwrap();
                            return vec![Response::Status(
                                Status::bad(Some(command.tag), None, "Invalid BASE64 literal")
                                    .unwrap(),
                            )];
                        }
                    }
                };

                /* AuthenticateDataCodec ne gère par "*" mais l'erreur failed le gère
                 * (pour la mauvaise raison :p)
                 */

                let parts: Vec<_> = line.0.declassify().split(|c| *c == 0).collect();
                if parts.len() != 3 {
                    return vec![Response::Status(
                        Status::no(Some(command.tag), None, "Invalid challenge").unwrap(),
                    )];
                }
                let identity = parts[0];
                let username = parts[1];
                let password = parts[2];

                if identity != "".as_bytes() && identity != username {
                    return vec![Response::Status(
                        Status::no(Some(command.tag), None, "Invalid identity").unwrap(),
                    )];
                }

                match login(
                    str::from_utf8(username).unwrap(),
                    str::from_utf8(password).unwrap(),
                ) {
                    Ok(token) => {
                        connection.state = State::Authenticated;
                        connection.token = Some(token);
                        return vec![Response::Status(
                            Status::ok(
                                Some(command.tag),
                                Some(Code::Capability(capabilities())),
                                "AUTHENTICATE completed",
                            )
                            .unwrap(),
                        )];
                    }
                    Err(message) => {
                        return vec![Response::Status(
                            Status::no(
                                Some(command.tag),
                                None,
                                match message {
                                    Some(message) => format!("AUTHENTICATE failed: {}", message),
                                    None => String::from("AUTHENTICATE failed"),
                                },
                            )
                            .unwrap(),
                        )];
                    }
                }
            }
            Login { username, password } => {
                match login(
                    str::from_utf8(username.as_ref()).unwrap(),
                    str::from_utf8(password.declassify().as_ref()).unwrap(),
                ) {
                    Ok(token) => {
                        connection.state = State::Authenticated;
                        connection.token = Some(token);
                        return vec![Response::Status(
                            Status::ok(
                                Some(command.tag),
                                Some(Code::Capability(capabilities())),
                                "LOGIN completed",
                            )
                            .unwrap(),
                        )];
                    }
                    Err(message) => {
                        return vec![Response::Status(
                            Status::no(
                                Some(command.tag),
                                None,
                                match message {
                                    Some(message) => format!("LOGIN failed: {}", message),
                                    None => String::from("LOGIN failed"),
                                },
                            )
                            .unwrap(),
                        )];
                    }
                }
            }
            _ => (),
        }
    }

    if let Authenticated | Selected(_) = connection.state {
        match command.body {
            Select { mailbox } => todo!("SELECT {:?}", mailbox),
            Examine { mailbox } => todo!("EXAMINE {:?}", mailbox),
            Create { mailbox } => todo!("CREATE {:?}", mailbox),
            Delete { mailbox } => todo!("DELETE {:?}", mailbox),
            Rename { from, to } => todo!("RENAME {:?} {:?}", from, to),
            List {
                reference,
                mailbox_wildcard,
            } => todo!("LIST {:?} {:?}", reference, mailbox_wildcard),
            StatusCommand {
                mailbox,
                item_names,
            } => todo!("STATUS {:?} {:?}", mailbox, item_names),
            _ => (),
        }
    }

    if let Selected(mailbox) = &connection.state {
        match command.body {
            Check => todo!("CHECK ({:?})", mailbox),
            Close => todo!("CLOSE ({:?})", mailbox),
            Search {
                charset,
                criteria,
                uid,
            } => todo!(
                "SEARCH {:?} {:?} {:?} ({:?})",
                charset,
                criteria,
                uid,
                mailbox
            ),
            Fetch {
                sequence_set,
                macro_or_item_names,
                uid,
            } => todo!(
                "FETCH {:?} {:?} {:?} ({:?})",
                sequence_set,
                macro_or_item_names,
                uid,
                mailbox
            ),
            _ => (),
        }
    }

    vec![Response::Status(
        Status::no(Some(command.tag), None, "Not supported!").unwrap(),
    )]
}

trait AsRange {
    fn as_range_of(&self, other: &Self) -> Option<Range<usize>>;
}

impl<T> AsRange for [T] {
    fn as_range_of(&self, other: &[T]) -> Option<Range<usize>> {
        let self_ = self.as_ptr_range();
        let other = other.as_ptr_range();
        if other.start > self_.start || self_.end > other.end {
            None
        } else {
            let from = unsafe { self_.start.offset_from(other.start) };
            let to = unsafe { self_.end.offset_from(other.start) };
            Some((from as usize)..(to as usize))
        }
    }
}

fn responder(mut stream: TcpStream, mut connection: Connection<'_>) {
    let mut buffer = [0u8; 1024];
    let mut cursor = 0;

    stream
        .write(
            &GreetingCodec::default()
                .encode(&Greeting {
                    kind: GreetingKind::Ok,
                    code: Some(Code::Capability(capabilities())),
                    text: Text::try_from("ecoledirecte-imap ready").unwrap(),
                })
                .dump(),
        )
        .unwrap();

    connection.state = State::NotAuthenticated;

    loop {
        match CommandCodec::default().decode(&buffer[..cursor]) {
            Ok((remaining, command)) => {
                for response in process(command, &mut connection, &mut stream) {
                    stream
                        .write(&ResponseCodec::default().encode(&response).dump())
                        .unwrap();
                }

                if let State::Logout = connection.state {
                    break;
                }

                let range = remaining.as_range_of(&buffer).unwrap();
                cursor = range.len();
                buffer.copy_within(range, 0);
            }
            Err(CommandDecodeError::LiteralFound { tag, length, mode }) => {
                todo!("LITERAL {:?} {} {:?}", tag, length, mode)
            }
            Err(CommandDecodeError::Incomplete) => {
                if cursor >= buffer.len() {
                    todo!("OUT OF MEMORY!");
                }
                let received = stream.read(&mut buffer[cursor..]).unwrap();
                if received == 0 {
                    break;
                }
                cursor += received;
            }
            Err(CommandDecodeError::Failed) => {
                stream
                    .write(
                        &ResponseCodec::default()
                            .encode(&Response::Status(
                                Status::bad(None, None, "Parsing failed").unwrap(),
                            ))
                            .dump(),
                    )
                    .unwrap();
                cursor = 0;
            }
        }
    }
}

fn main() {
    let listener = TcpListener::bind("localhost:1993").unwrap();

    thread::scope(|s| {
        for stream in listener.incoming() {
            let stream = stream.unwrap();

            s.spawn(|| responder(stream, Connection::default()));
        }
    });
}
