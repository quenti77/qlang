# Sécurité : ce qui est vérifié

Ce document garde le résultat des revues de sécurité (voir aussi `SECURITY.md` pour signaler une
faille). Il dit ce qui a été regardé, ce qui a été corrigé et ce qui reste à la charge de celui qui
écrit le programme.

## Primitives cryptographiques (`Crypto`, JWT)

Revue de `crates/qlang-cli/src/signature.rs`, `Crypto` du prélude et `jwt.q` de `qlang-std`.

- **Rien n'est réécrit à la main** : les signatures (RSA PKCS#1 et PSS, ECDSA P-256 et P-384,
  Ed25519) et le HMAC viennent de `ring`, qui compare les signatures HMAC en temps constant. Les
  clés RSA de moins de 2048 bits sont refusées par `ring`.
- **Le code qui lit les clés** (PEM, JWK, DER) ne fait que de la lecture bornée : une longueur
  incohérente donne une erreur, pas un accès hors limites (testé, et couvert par les balayages de
  `mutation_fuzz`). Un mauvais format de clé est une erreur claire, une signature fausse est
  `false`.
- **`Crypto.equal` et `Crypto.equal_bytes`** regardent tous les octets quel que soit le premier
  écart (seule la longueur peut se déduire du temps).
- **Corrigé** : `Crypto.verify` avec un secret HMAC **vide** rendait `true` pour une signature
  faite avec un secret vide, alors que `sign` le refusait. Un secret lu dans une variable
  d'environnement absente (`""`) aurait laissé n'importe qui fabriquer un jeton valide.
  `verify` rend maintenant une erreur, comme `sign`.
- **JWT (`qlang-std`)** : l'algorithme est celui des clés que vous donnez, jamais celui que le
  jeton réclame (`none` est refusé, une clé publique n'est jamais prise pour un secret HMAC : c'est
  l'attaque classique de confusion d'algorithme) ; `exp` est exigé par défaut ; un secret HMAC doit
  faire au moins la taille de son hachage.
- **Reste à votre charge** : choisir un secret long et aléatoire (`Crypto.token`), le garder hors du
  code, faire tourner les clés (`kid`).

## HTTPS (TLS)

`qlang serve` parle HTTP en clair, c'est un choix assumé : le certificat, son renouvellement et les
réglages TLS sont le métier d'un proxy inverse (Caddy, nginx, Traefik, l'équilibreur de l'hébergeur), qui
parle à qlang sur la machine. Démarrer qlang sur l'adresse locale (`--bind 127.0.0.1`) pour que seul le proxy
l'atteigne, et faire remplacer `X-Forwarded-For` / `X-Forwarded-Proto` par le proxy lui-même (Caddy
`reverse_proxy` le fait ; nginx : `proxy_set_header X-Forwarded-For $remote_addr;`
`proxy_set_header X-Forwarded-Proto $scheme;`). Côté programme, `qlang-std` a `web/proxy.q`
(`is_https`, `client_ip`, `redirect_to_https()`) et les cookies de session prennent `Secure`.
Un TLS intégré à `qlang serve` n'est pas fait.

## Permissions

Un programme qlang ne peut que ce que son hôte lui laisse :

| Capacité | Qui l'accorde | Vérifié |
| --- | --- | --- |
| Lire et écrire des fichiers (`fs`), `import` | `qlang run` et `serve` : tout le disque lisible ; `--root DIR` : ce dossier | test `fs_stays_inside_the_root` : `..`, chemin absolu, lien symbolique vers l'extérieur sont refusés |
| Lancer un programme (`Process.run`) | `--allow-process` seulement (`qlang run`, `test`, `serve`) ; jamais le protocole JSON, le navigateur ni `qlang.toml` | erreur de compilation `T250` sans la permission |
| Commandes `extern` d'un `qlang.toml` | demandées une fois au terminal et mémorisées par fichier, nom et commande (une commande changée redemande) ; sans terminal, `--trust-externs` | `--extern CMD` sur la ligne de commande est un choix de l'utilisateur |
| Durée et mémoire | `--max-steps`, `--max-memory` | arrêt `R904` |
| Réseau, base de données, variables d'environnement | accordés au programme dès qu'il tourne avec la commande `qlang` ; absents du protocole JSON et du navigateur | |

À savoir :

- `--root` ne borne **pas** un programme lancé avec `--allow-process` : une commande lancée voit
  tout le disque et n'a pas de limite de pas ni de mémoire (une minute au plus). Accorder
  `--allow-process`, c'est accorder l'accès à la machine.
- `--root` ne limite pas les paquets (`qlang.toml`) : c'est le projet qui les choisit.
- Le serveur n'a pas de TLS ni d'authentification propres : voir `docs/language.md`, « Le serveur ».
