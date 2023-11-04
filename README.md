# ecoledirecte-imap

Un serveur IMAP qui permet d'accéder à sa messagerie EcoleDirecte avec un client mail normal.

Pour l'instant c'est en pré-pré-pré-pré-alpha... Mais contribuez si vous voulez !

## Utilisation

```sh
cargo run
```

## Autres notes

Commands implémentées (± par ordre de priorité) :
 - [x] Login
 - [x] Authenticate PLAIN
 - [x] Capability
 - [x] Noop (facile à implémenter :p)
 - [x] Logout
 - [ ] List: Il reste la logique de tri à implémenter
 - [x] Select
 - [ ] Fetch
 - [x] Close
 - [ ] Examine
 - [ ] Create
 - [ ] Delete
 - [ ] Rename
 - [ ] Check
 - [ ] Search

Extensions potentielles :
 - [ ] Idle
 - [ ] Move (obligatoire puisqu'on implémente pas copy/store/expunge)
 - [ ] Unselect (même si ça ne change rien puisque pas d'expunge)

Il y a d'autres commandes dans la spécification IMAP mais la nature même de la messagerie EcoleDirecte ne permet pas de les faire fonctionner. En gros, tout ce qui concerne l'ajout ou la suppression de message.

Autres choses à faire (notes de dev) :
 - [ ] Async !
 - [ ] Utiliser un truc plus robuste pour les messages (Framed de tokio_util) avec un moyen de faire stream.read_message(ReponseCodec) ET stream.read_message(AuthenticateDataCodec) puisque ça résout direct la complexité d'Authenticate
