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
 - [ ] Select
 - [ ] Fetch
 - [ ] Close
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
 - [ ] Nettoyer tout ça (mettre dans des fichiers séparés parce que là c'est n'importe quoi)
 - [ ] Faire un client de l'API digne de ce nom
 - [ ] Async !
 - [ ] Utiliser un truc plus robuste pour les messages (Framed de tokio_util)
