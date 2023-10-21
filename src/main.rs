use imap_codec::{
    decode::{CommandDecodeError, Decoder},
    encode::Encoder,
    imap_types::{
        command::Command,
        core::{NonEmptyVec, Text},
        response::{Capability, Code, Data, Greeting, GreetingKind, Response, Status},
        state::State,
    },
    CommandCodec, GreetingCodec, ResponseCodec,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::ops::Range;
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

fn login(username: &[u8], password: &[u8]) -> Result<String, Option<String>> {
    let body = json!({
        "username": username,
        "password": password
    });

    let client = reqwest::blocking::Client::new();
    let response: APIResponse = client
        .post("https://api.ecoledirecte.com/v3/login.awp?v=4.43.0")
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

fn process<'a, 'b>(command: Command<'a>, connection: &mut Connection<'b>) -> Vec<Response<'a>> {
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
            } => todo!("AUTHENTICATE {:?} {:?}", mechanism, initial_response),
            Login { username, password } => {
                match login(username.as_ref(), password.declassify().as_ref()) {
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

fn responder<'a>(mut stream: TcpStream, mut connection: Connection<'a>) {
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
                for response in process(command, &mut connection) {
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
