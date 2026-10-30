# Migrer d'une version à l'autre

Ce fichier garde, pour chaque changement cassant, ce qui change et comment s'adapter. La règle est dans
[stability.md](stability.md) : à partir de la 0.69, chaque version du CHANGELOG qui casse quelque chose a une
rubrique **Cassant**, et le changement figure ici.

Pour trouver ce qui pourrait casser **avant** de monter de version : lancer `qlang check --deny-warnings` (ou
`qlang test --deny-warnings`) sur le projet avec la version actuelle ; ce qui est déprécié apparaît en `W001`.

## 0.69 : `Log.warn` est déprécié (pas encore retiré)

- **Avant** : `Log.warn("message")`.
- **Après** : `Log.warning("message")`. `Log.warn` marche encore, avec l'avertissement `W001`. Retrait au plus tôt en 0.72.
- **S'adapter** : remplacer `Log.warn(` par `Log.warning(` (recherche et remplacement dans le projet).

## 0.61 : `--dev` est refusé hors `localhost`

- **Avant** : `qlang serve app.q --dev --bind 0.0.0.0` affichait les erreurs détaillées à tout le monde.
- **Après** : refusé, parce que la page d'erreur montre le code, la pile et les requêtes SQL.
- **S'adapter** : ne pas utiliser `--dev` en production ; si c'est voulu sur un réseau de confiance, ajouter `--dev-public`.

## 0.57 : `Log.warn` écrit le niveau `warning`

- **Avant** : le niveau écrit était `warn` (`"level":"warn"` en JSON).
- **Après** : `warning`, comme PSR-3.
- **S'adapter** : mettre à jour les filtres des outils de journaux qui cherchaient `warn`.

## 0.36 : base distante en clair refusée, limite de mémoire

- `Db.connect("postgres://hôte-distant/…")` sans `sslmode` dans l'URL est **refusé** (le mot de passe partait en clair).
  S'adapter : `?sslmode=require`, ou `?sslmode=disable` si c'est voulu.
- Avec `--root`, le fichier d'une base `sqlite://` doit être dans le dossier.
- La mémoire d'un programme est limitée à 1 Go (`R904`) : `--max-memory MB` pour la changer, `0` pour la retirer.
- Une réponse de plus de 64 Mo d'un fournisseur d'`extern` est refusée.

## Avant la 0.36

Les changements cassants n'étaient pas marqués à part : voir le [CHANGELOG](../CHANGELOG.md).
