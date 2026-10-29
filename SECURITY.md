# Politique de sécurité

## Signaler une faille

**N'ouvrez pas d'issue publique** pour une faille : utilisez le signalement privé de GitHub,
[Security → Report a vulnerability](https://github.com/quenti77/qlang/security/advisories/new).
Décrivez ce que vous avez trouvé, la version (`qlang --version`), et si possible un programme ou
une requête qui le reproduit. Une faille dans `qlang-std`
([dépôt](https://github.com/quenti77/qlang-std)) se signale de la même manière dans ce dépôt-là.

Ce projet est entretenu par une personne, sur son temps : voici ce qu'on peut tenir.

| Étape | Délai visé |
| --- | --- |
| Accusé de réception | 7 jours |
| Première analyse (faille confirmée ou non, gravité) | 14 jours |
| Correctif publié pour une faille confirmée | 30 jours pour une faille grave, au mieux ensuite |

Vous êtes informé de l'avancement, et cité dans le journal des changements si vous le souhaitez.
Merci de laisser le temps de publier un correctif avant de parler de la faille publiquement.

## Versions suivies

qlang n'a pas encore de version 1.0. Seule la **dernière version** reçoit des correctifs de
sécurité : mettez à jour avant de signaler un problème déjà corrigé.

## Ce qui est une faille

- Un programme qui sort des permissions que l'hôte lui a laissées (`--root`, `--allow-process`,
  `--extern` et `--trust-externs`, protocole JSON et navigateur, qui n'accordent ni processus ni
  extern).
- Un plantage, un blocage ou une consommation sans borne du serveur (`qlang serve`) provoqué par une
  requête, ou du compilateur par un texte de programme (hors limites `--max-steps` et
  `--max-memory`, qui bornent l'exécution).
- Une erreur dans une primitive de sécurité : `Crypto` (signatures, JWT, hachage de mot de passe),
  sessions et jetons de `qlang-std`.
- Une chaîne d'approvisionnement : installation d'un paquet, fichier `qlang.lock`, archive de
  release dont la somme ou l'attestation ne correspond pas.

## Ce qui n'en est pas une

- Un programme qui fait ce qu'on lui a permis de faire : un programme lancé avec `--allow-process`
  ou avec des `extern fun` de confiance exécute des commandes, c'est le but.
- Le serveur sans TLS : `qlang serve` parle HTTP et doit se trouver derrière un proxy inverse qui
  fait le TLS (voir `docs/language.md`).
- Un programme qui tourne longtemps ou utilise beaucoup de mémoire sans limite demandée.

## Ce qui est déjà en place

- Les archives de release ont une somme (`SHA256SUMS`) et une attestation de provenance
  (`gh attestation verify`).
- Les dépendances Rust sont vérifiées chaque semaine avec `cargo audit` (workflow `Audit`) et mises à
  jour par Dependabot.
- Des tests de mutation (`crates/qlang-core/tests/mutation_fuzz.rs`, balayage long chaque dimanche
  avec une nouvelle graine : workflow `Fuzz`) et de requêtes HTTP malformées
  (`damaged_requests_do_not_stop_the_server`) vérifient que ni le compilateur, ni le formateur, ni
  le serveur ne plantent sur une entrée abîmée.
