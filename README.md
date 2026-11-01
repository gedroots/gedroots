# GedRoots

Logiciel de généalogie **libre et local**, écrit en Rust avec [GPUI](https://www.gpui.rs/)
(via [GPUI Kit](https://gpui-kit.com)). Les données restent dans un fichier **GEDCOM 7**
sur votre disque : pas de base de données, pas de compte, pas de cloud.

L'interface reprend les fonctions principales d'[Ancestris](https://www.ancestris.org)
et suit la maquette « Lignée » (thème sombre, barre d'activité, liste, arbre, fiche).

![Arbre d'ascendance](docs/capture-arbre.png)

## Fonctionnalités

- **Individus** : liste filtrable (nom, prénom, lieu, `@I…@`), filtre « sans source »,
  arbre d'ascendance sur 4 générations avec lignées paternelle/maternelle et numéros Sosa,
  fiche avec événements (dates en français, calendrier républicain compris), sources citées,
  famille (parents, conjoints, enfants) cliquable.
- **Familles, sources, dépôts, médias, notes partagées** : liste, éditeur visuel,
  et panneau « Utilisé par / Liens » pour naviguer entre les enregistrements.
- **Lieux** : index de tous les `PLAC` du fichier, renommage global et coordonnées (`MAP`).
- **Médias / photos** : aperçu des images, chemins relatifs au fichier `.ged`.
- **Éditeur visuel** pour les enregistrements courants (individu, famille, source, dépôt,
  média, note) : seuls les champs affichés sont modifiés, tout le reste est conservé.
- **Éditeur GEDCOM brut** pour chaque enregistrement, avec validation ; changer un
  identifiant met à jour tous les pointeurs.
- Créer / supprimer un enregistrement, annuler (50 niveaux), date de modification `CHAN`
  mise à jour comme dans Ancestris.
- **Analyses** (`⌘⇧A` / `Ctrl+Shift+A`, ou bouton « Analyses ») :
  - *Cohérence* : dates impossibles, âges invraisemblables, liens cassés ou à sens unique,
    boucles d'ascendance… Chaque anomalie ouvre la fiche concernée ; « Ignorer » la masque
    avec l'extension [`_GR_IGNORE`](docs/extensions.md) (annulable).
  - *Lien de parenté* : « cousine issue de germain », « oncle à la mode de Bretagne »,
    demi-frère, beau-père… avec le chemin par l'ancêtre commun et le degré civil.
  - *Chronologie* : âge à chaque événement, avec les naissances, mariages et décès des proches.
  - *Statistiques* : patronymes et prénoms fréquents, longévité par siècle, âge au mariage,
    complétude de l'ascendance par génération et implexe.
- Enregistrement **sans perte** : un fichier relu puis réécrit est identique à l'octet près
  (balises propriétaires `_SOSADABOVILLE`, `_TODO`… comprises). Une copie `.ged.bak` est
  gardée à chaque enregistrement.

![Éditeur visuel](docs/capture-fiche.png)

## Lancer

```sh
cargo run --release -- chemin/vers/arbre.ged
```

Sans argument, l'app propose d'ouvrir un fichier ou d'en créer un nouveau.
Raccourcis : `⌘O` / `Ctrl+O` ouvrir, `⌘S` / `Ctrl+S` enregistrer, `⌘N` nouveau,
`⌘⇧Z` annuler la dernière modification du document.

Sous Linux, installer d'abord les dépendances de GPUI :
`libxkbcommon-dev libxkbcommon-x11-dev libfontconfig-dev libwayland-dev libvulkan-dev`.

## Organisation du code

| Crate | Rôle |
| --- | --- |
| [`crates/gedcom`](crates/gedcom) | Lecture/écriture GEDCOM 7 sans perte, vues typées (individus, familles, sources, lieux, médias), formulaires d'édition, dates, outils d'analyse (`analysis/`). Sans dépendance. |
| [`crates/gedroots`](crates/gedroots) | Application GPUI : `workspace.rs` (fenêtre et vues), `editor.rs` (éditeurs visuel et brut), `theme.rs` (palette de la maquette), `analyses.rs` (fenêtre Analyses). |

## Tests

```sh
cargo test -p gedcom
# Contrôle de cohérence et statistiques d'un fichier, en ligne de commande :
cargo run -p gedcom --example analyse -- ~/Genealogie/arbre.ged
# Vérifier qu'un vrai fichier est relu et réécrit à l'identique :
GEDROOTS_TEST_GED=~/Genealogie/arbre.ged cargo test -p gedcom real_file -- --nocapture
```

Le fichier d'exemple `crates/gedcom/tests/fixtures/exemple.ged` est fictif. Les fichiers
`.ged` personnels sont ignorés par git.
