use imap_codec::{
    decode::{AuthenticateDataDecodeError, CommandDecodeError, Decoder},
    encode::Encoder,
    imap_types::{
        self,
        auth::AuthMechanism,
        command::Command,
        core::Text,
        flag::{Flag, FlagPerm},
        mailbox::{ListMailbox, Mailbox},
        response::{
            Code, CommandContinuationRequest, Data, Greeting, GreetingKind, Response, Status,
        },
        secret::Secret,
        state::State,
    },
    AuthenticateDataCodec, CommandCodec, GreetingCodec, ResponseCodec,
};
use std::borrow::Cow;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::ops::Range;
use std::str;
use std::thread;

use ecoledirecte_imap::api;
use ecoledirecte_imap::auth;
use ecoledirecte_imap::capabilities;

struct Connection<'a> {
    state: State<'a>,
    user: Option<auth::User>,
}

impl<'a> Default for Connection<'a> {
    fn default() -> Connection<'a> {
        Connection {
            state: State::Greeting,
            user: None,
        }
    }
}

fn main() {
    let listener = TcpListener::bind("localhost:1993").unwrap();
    let client = reqwest::blocking::Client::new();

    thread::scope(|s| {
        for stream in listener.incoming() {
            let stream = stream.unwrap();

            s.spawn(|| responder(stream, Connection::default(), &client));
        }
    });
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

fn responder(
    mut stream: TcpStream,
    mut connection: Connection<'_>,
    client: &reqwest::blocking::Client,
) {
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
                print!(
                    "C: {}",
                    str::from_utf8(&CommandCodec::default().encode(&command).dump()).unwrap()
                );
                for response in process(command, &mut connection, &mut stream, client) {
                    print!(
                        "S: {}",
                        str::from_utf8(&ResponseCodec::default().encode(&response).dump()).unwrap()
                    );
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

fn process<'a>(
    command: Command<'a>,
    connection: &mut Connection<'_>,
    stream: &mut TcpStream,
    client: &reqwest::blocking::Client,
) -> Vec<Response<'a>> {
    use imap_types::{
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

                // TODO: Malgré cette tentative de faire marcher les choses (voir commentaire suivant),
                // ça va pas :
                // Si le client envoie un paquet AAABBB avec AAA une commande AUTHENTICATE PLAIN
                // normalement on devrait lire BBB pour avoir les données d'authentification
                // mais ici on n'y a pas accès (elles sont dans le buffer de la boucle principale)
                // donc on se retrouvera à lire CCC d'un autre paquet
                // Donc la gestion totale pour AAABBB, CCC... serait AAA, CCC, BBB, ...
                // ce qui ne va clairement pas (même si en pratique si le client est bien discipliné
                // il devrait attendre de recevoir la confirmation du serveur pour envoyer les données
                // d'authentification. il y a quand même de quoi améliorer les choses + aussi les
                // littéraux non-synchronisants poseraient problème (mais je sais pas s'ils peuvent
                // être utilisés pendant l'authentification))
                let mut buffer = [0u8; 1024];
                let mut consumed = 0;
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
                            stream.read(&mut buffer[consumed..range.start]).unwrap();
                            break line;
                        }
                        Err(AuthenticateDataDecodeError::Incomplete) => {
                            if peeked >= buffer.len() {
                                todo!("OUT OF MEMORY");
                            }
                            // unwrap: ok puisque déjà peeked
                            stream.read(&mut buffer[consumed..peeked]).unwrap();
                            consumed = peeked;
                            let received = stream.peek(&mut buffer[consumed..]).unwrap();
                            if received == 0 {
                                return vec![];
                            }
                            peeked += received;
                        }
                        Err(AuthenticateDataDecodeError::Failed) => {
                            stream.read(&mut buffer[consumed..peeked]).unwrap();
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
                let (username, password) = match auth::parse_plain_message(
                    Secret::new(&line.0.declassify()),
                    command.tag.clone(),
                ) {
                    Ok(tup) => tup,
                    Err(response) => {
                        return response;
                    }
                };

                let (state, user, response) =
                    auth::translate(api::login(client, username, password), command.tag);

                connection.state = state;
                connection.user = user;
                return response;
            }
            Login { username, password } => {
                let (state, user, response) = auth::translate(
                    api::login(
                        client,
                        str::from_utf8(username.as_ref()).unwrap(),
                        str::from_utf8(password.declassify().as_ref()).unwrap(),
                    ),
                    command.tag,
                );

                connection.state = state;
                connection.user = user;
                return response;
            }
            _ => (),
        }
    }

    if let Authenticated | Selected(_) = connection.state {
        match command.body {
            Select { mailbox } => {
                return vec![
                    Response::Data(Data::Flags(vec![Flag::Seen, Flag::Answered, Flag::Draft])),
                    Response::Data(Data::Exists(0)),
                    Response::Data(Data::Recent(0)),
                    Response::Status(
                        Status::ok(
                            None,
                            Some(Code::PermanentFlags(vec![
                                FlagPerm::Flag(Flag::Seen),
                                FlagPerm::Flag(Flag::Draft),
                            ])),
                            "Flags",
                        )
                        .unwrap(),
                    ),
                    Response::Status(
                        Status::ok(Some(command.tag), Some(Code::ReadWrite), "SELECT completed")
                            .unwrap(),
                    ),
                ];
            }
            Examine { mailbox } => todo!("EXAMINE {:?}", mailbox),
            Create { mailbox } => todo!("CREATE {:?}", mailbox),
            Delete { mailbox } => todo!("DELETE {:?}", mailbox),
            Rename { from, to } => todo!("RENAME {:?} {:?}", from, to),
            List {
                reference,
                mailbox_wildcard,
            } => {
                use imap_types::flag::FlagNameAttribute::Noselect;
                let name = match mailbox_wildcard {
                    ListMailbox::String(ref name) => name.as_ref(),
                    ListMailbox::Token(ref name) => name.as_ref(),
                };

                if name.len() == 0 {
                    return vec![
                        Response::Data(Data::List {
                            items: vec![Noselect],
                            delimiter: None,
                            mailbox: Mailbox::try_from("").unwrap(),
                        }),
                        Response::Status(
                            Status::ok(Some(command.tag), None, "LIST completed").unwrap(),
                        ),
                    ];
                }

                // unwrap: on est en authenticated ou selected
                let user = connection.user.clone().unwrap();
                let mut response: Vec<_> = list(
                    api::get_folders(client, user.id, &user.token),
                    reference,
                    name,
                )
                .into_iter()
                .map(|classeur: String| {
                    Response::Data(Data::List {
                        items: vec![],
                        delimiter: None,
                        mailbox: Mailbox::try_from(classeur).unwrap(),
                    })
                })
                .collect();

                response.push(Response::Data(Data::List {
                    items: vec![],
                    delimiter: None,
                    mailbox: Mailbox::Inbox,
                }));
                response.push(Response::Status(
                    Status::ok(Some(command.tag), None, "LIST completed").unwrap(),
                ));
                return response;
            }
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

fn list(folders: Vec<String>, reference: Mailbox<'_>, mailbox_wildcard: &[u8]) -> Vec<String> {
    folders // TODO!!!
}
