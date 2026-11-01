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
- **Carte des lieux** : tous les lieux ayant des coordonnées sur une carte OpenStreetMap
  (glisser pour déplacer, molette pour zoomer, clic sur un repère pour ouvrir le lieu).
  Les lieux sans coordonnées se localisent d'un clic (un par un ou tous, via Nominatim) ou se
  placent à la main ; les coordonnées sont écrites dans le GEDCOM (`PLAC.MAP.LATI/LONG`,
  format `N49.11652` / `W1.11437`) et s'annulent comme toute modification. Les tuiles sont
  gardées en cache sur le disque : en mode « Hors ligne », rien ne part sur Internet.

![Carte des lieux](docs/capture-carte.png)

- **Médias / photos** : aperçu des images, chemins relatifs au fichier `.ged`.
- **Éditeur visuel** pour les enregistrements courants (individu, famille, source, dépôt,
  média, note) : seuls les champs affichés sont modifiés, tout le reste est conservé.
- **Éditeur GEDCOM brut** pour chaque enregistrement, avec validation ; changer un
  identifiant met à jour tous les pointeurs.
- Créer / supprimer un enregistrement, annuler (50 niveaux), date de modification `CHAN`
  mise à jour comme dans Ancestris.
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

Variables d'environnement facultatives pour la carte : `GEDROOTS_OFFLINE=1` (démarrer hors ligne),
`GEDROOTS_TILE_URL` (autre serveur de tuiles, `{z}/{x}/{y}`), `GEDROOTS_TILE_CACHE` (dossier du
cache, par défaut `~/Library/Caches/GedRoots/tiles` ou `~/.cache/gedroots/tiles`),
`GEDROOTS_GEOCODER` (autre instance Nominatim).

Sous Linux, installer d'abord les dépendances de GPUI :
`libxkbcommon-dev libxkbcommon-x11-dev libfontconfig-dev libwayland-dev libvulkan-dev`.

## Organisation du code

| Crate | Rôle |
| --- | --- |
| [`crates/gedcom`](crates/gedcom) | Lecture/écriture GEDCOM 7 sans perte, vues typées (individus, familles, sources, lieux, médias), formulaires d'édition, dates, coordonnées (`geo.rs`). Sans dépendance. |
| [`crates/gedroots`](crates/gedroots) | Application GPUI : `workspace.rs` (fenêtre et vues), `editor.rs` (éditeurs visuel et brut), `map.rs` (carte des lieux), `theme.rs` (palette de la maquette). |

## Tests

```sh
cargo test -p gedcom
# Vérifier qu'un vrai fichier est relu et réécrit à l'identique :
GEDROOTS_TEST_GED=~/Genealogie/arbre.ged cargo test -p gedcom real_file -- --nocapture
```

Le fichier d'exemple `crates/gedcom/tests/fixtures/exemple.ged` est fictif. Les fichiers
`.ged` personnels sont ignorés par git.
