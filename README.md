# GedRoots

Logiciel de généalogie **libre et local**, écrit en Rust avec [GPUI](https://www.gpui.rs/)
(via [GPUI Kit](https://gpui-kit.com)). Les données restent dans un fichier **GEDCOM 7**
sur votre disque : pas de base de données, pas de compte, pas de cloud.

L'interface reprend les fonctions principales d'[Ancestris](https://www.ancestris.org)
et suit la maquette « Lignée » (thème sombre, barre d'activité, liste, arbre, fiche).

![Arbre d'ascendance](docs/capture-arbre.png)

## Fonctionnalités

- **Tout est dans la barre de menus** (Fichier, Édition, Individu, Famille, Source, Média,
  Affichage, Analyses, Aide) : pas de barre d'outils, l'écran reste aux données. Menus natifs
  sous macOS ; sous Linux et Windows, les mêmes menus dans le haut de la fenêtre. Les commandes
  qui ne s'appliquent pas à la sélection sont grisées (macOS) ou sans effet.
- **Palette de commandes** (`⌘⇧P` / `Ctrl+Maj+P`) : toutes les commandes applicables, recherche
  sans accents. Raccourcis : `⌘1`…`⌘7` listes, `⌥⌘1`…`⌥⌘3` arbre/fiche/GEDCOM, `⌘F`
  rechercher, `⌘⇧N` nouvel élément, `⌘⇧H` individu racine, `⌘=` / `⌘-` générations,
  `⌘⇧S` enregistrer sous.
- **Clic droit partout** : lignes des listes (et le fond, pour créer), cases des arbres et de
  l'éventail, cases vides (nouvelle personne ou personne existante), événements, proches,
  membres d'une famille, liens et vignettes du panneau de détail, repères et fond de la carte
  (« Placer ce lieu ici »). Le menu agit sur l'élément cliqué sans le sélectionner, avec les
  mêmes commandes que la barre de menus.
- **Événements modifiables** : un clic sur un événement (individu ou famille) le rouvre dans
  la même fenêtre que l'ajout (type, précision, date, lieu suggéré, source) ; on peut aussi le
  supprimer. Notes, âge, médias et autres citations de l'événement sont conservés.
- **Individus** : liste avec recherche sans accents ni casse et critères
  (`né:1800-1850`, `mort:<1900`, `vécu:1870`, `lieu:troyes`, `prof:meunier`, `sexe:f`,
  `sans:source`, `sans:parents`, `avec:média`, `sosa:1-63`…), filtres rapides (Affichage › Filtrer la liste) ;
  fiche avec portrait, galerie d'images, événements (dates en français, calendrier républicain compris), sources citées,
  famille (parents, conjoints, enfants) cliquable.
- **Arbres** : ascendance (4 générations, portraits, lignées paternelle/maternelle, numéros Sosa),
  descendance (2 à 8 générations) et **éventail** (2 à 7 générations), cliquables pour recentrer.
- **Ajouter un proche** en un clic depuis une personne (père, mère, conjoint, enfant, frère ou
  sœur), ou **lier une personne existante** via un sélecteur qui propose d'abord les dates
  plausibles ; détacher une personne d'une famille. Les liens `FAMS`/`FAMC` ↔ `HUSB`/`WIFE`/`CHIL`
  restent cohérents, et une famille vidée est supprimée.
- **Photos et actes** : glisser-déposer ou choisir des fichiers sur une personne, une famille ou
  une source ; suggestions des fichiers du dossier (`Actes/`, `Portraits/`…) dont le nom contient
  celui de la personne ; liste des fichiers du dossier sans média, importables en un clic.
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

- **Médias / photos** : portraits des individus, images des sources (actes numérisés)
  dans la fiche et le panneau de détail, chemins relatifs au fichier `.ged`.
- **Recadrage et identification des personnes** : sur la fiche d'une photo, Média › « Identifier une
  personne sur la photo » puis tracer un cadre autour du visage et choisir la personne. C'est le `CROP`
  standard de GEDCOM 7 sur le lien `INDI → OBJE` (pas d'extension, lisible par les autres
  logiciels) : la photo de groupe affiche les noms, le portrait et les vignettes de la personne
  sont recadrés sur son visage. Chaque lien vers une image (source, famille…) peut aussi être
  délimité, par exemple l'extrait utile d'un registre.
- **Liens web** (page d'un acte aux archives en ligne, fiche Geneanet…) : section « Liens web »
  du panneau de détail, cliquable (ouverture dans le navigateur), avec « + », modifier et
  retirer ; aussi Édition › « Ajouter un lien web… ». Stockés en GEDCOM 7 standard : lien
  multimédia vers un `OBJE` dont le `FILE` est l'URL (`FORM text/html`), et `WWW` pour les
  dépôts.
- **Assistant d'import** (Fichier › « Importer un GEDCOM… ») : ouvre un fichier venu d'un autre
  logiciel, liste ce qu'il contient de non standard et propose de le convertir en GEDCOM 7
  (annulable). Premier contrôle : les balises `_URL` deviennent des liens multimédia (`WWW`
  pour les dépôts). Les contrôles sont dans `crates/gedcom/src/import.rs`.
- **Médias à plusieurs pages** : un acte sur deux feuillets est un seul média (`OBJE` avec un
  `FILE` par page, standard GEDCOM 7). Visionneuse page par page (miniatures, ⇞ / ⇟), pages
  ajoutées par glisser-déposer ou Média › Pages, réordonnées en glissant les miniatures, retirées
  ou séparées en un nouveau média. Pour fusionner des médias existants : ⌘ / Ctrl + clic ou Maj + clic
  dans la liste des médias, puis « Fusionner » (ou clic droit, ou Média › Fusionner avec un autre
  média…). Tous les liens sont réécrits ; une personne située sur la page 2 garde son cadre
  (extension [`_GR_FILE`](docs/extensions.md#_gr_file)).

![Personnes identifiées sur une photo](docs/capture-photo.png)
- **Individu racine** : enregistré dans le fichier lui-même (extension `_GR_ROOT`, voir
  [docs/extensions.md](docs/extensions.md)), jamais dans un fichier annexe.
- Les identifiants GEDCOM n'apparaissent pas dans l'interface : liens choisis par nom
  (époux, dépôt…), identifiant accessible via **… → Copier l'ID** et l'onglet GEDCOM.
- **Éditeur visuel** pour les enregistrements courants (individu, famille, source, dépôt,
  média, note) : seuls les champs affichés sont modifiés, tout le reste est conservé.
- **Éditeur GEDCOM brut** pour chaque enregistrement, avec validation ; changer un
  identifiant met à jour tous les pointeurs. **Autocomplétion** selon le schéma GEDCOM 7
  officiel : balises permises à cet endroit (recherche aussi par libellé français,
  « nais » → `BIRT`), individus/familles/sources pour les pointeurs, valeurs énumérées,
  mots-clés et mois des dates (calendrier républicain compris), lieux déjà saisis.
  `Ctrl+Espace` affiche les suggestions à la demande.
- **Saisie d'acte guidée** (`⌘E` / `Ctrl+E`), à la manière d'*Ancestral Sources* pour
  Family Historian : on choisit un modèle (naissance, baptême, mariage, décès, inhumation,
  recensement, registre matricule), la source et son image, puis on saisit les personnes
  citées. Pour chacune, GedRoots propose les individus existants qui correspondent (nom,
  âge, et liens déjà connus : « parent de… », « conjoint de… ») ; on peut lier, créer, ou
  mentionner sans créer. Un seul enregistrement crée les individus, l'événement, les liens
  familiaux, les témoins (`ASSO`), les professions et domiciles datés, une naissance estimée
  d'après l'âge, et toutes les citations avec leur rôle (`EVEN`/`ROLE`). Aperçu du GEDCOM
  avant d'écrire, et « Enregistrer et acte suivant » pour dépouiller un registre.
  Les modèles sont des extensions GEDCOM (voir [docs/extensions.md](docs/extensions.md)).
- Créer / supprimer un enregistrement (avec confirmation ; un lieu peut être retiré de tous
  les événements), annuler (50 niveaux), date de modification `CHAN`
  mise à jour comme dans Ancestris.
- **Analyses** (`⌘⇧A` / `Ctrl+Shift+A`, ou menu Analyses) :
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

![Éventail](docs/capture-eventail.png)

![Lier une personne existante](docs/capture-lier.png)

![Éditeur visuel](docs/capture-fiche.png)

![Saisie d'acte](docs/capture-saisie.png)

## Lancer

```sh
cargo run --release -- chemin/vers/arbre.ged
```

Sans argument, l'app propose d'ouvrir un fichier ou d'en créer un nouveau.
Raccourcis : `⌘O` / `Ctrl+O` ouvrir, `⌘S` / `Ctrl+S` enregistrer, `⌘N` nouveau,
`⌘E` saisir un acte, `⌘⇧Z` annuler la dernière modification du document.

Variables d'environnement facultatives pour la carte : `GEDROOTS_OFFLINE=1` (démarrer hors ligne),
`GEDROOTS_TILE_URL` (autre serveur de tuiles, `{z}/{x}/{y}`), `GEDROOTS_TILE_CACHE` (dossier du
cache, par défaut `~/Library/Caches/GedRoots/tiles` ou `~/.cache/gedroots/tiles`),
`GEDROOTS_GEOCODER` (autre instance Nominatim).

Sous Linux, installer d'abord les dépendances de GPUI :
`libxkbcommon-dev libxkbcommon-x11-dev libfontconfig-dev libwayland-dev libvulkan-dev`.

## Organisation du code

| Crate | Rôle |
| --- | --- |
| [`crates/gedcom`](crates/gedcom) | Lecture/écriture GEDCOM 7 sans perte, vues typées (individus, familles, sources, lieux, médias), recadrage `CROP` (`crop.rs`), médias multipages (`multipage.rs`), liens web (`weblink.rs`) et assistant d'import (`import.rs`), formulaires d'édition, dates, coordonnées (`geo.rs`), outils d'analyse (`analysis/`), recherche (`search.rs`), ajout/liaison de proches et médias (`relations.rs`), descendance (`pedigree.rs`), modèles d'actes (`template.rs`) et saisie guidée (`entry.rs`). Sans dépendance. |
| [`crates/gedroots`](crates/gedroots) | Application GPUI : `commands.rs` (toutes les commandes, raccourcis et menus), `workspace.rs` (fenêtre et vues) et `workspace/` (`commands.rs` commandes et palette, `tree.rs` arbres, `relatives.rs` proches/sélecteur/médias, `add.rs` fenêtres d'ajout et de modification d'événement, `menus.rs` menus du clic droit, `photo.rs` recadrage et personnes sur les photos, `pages.rs` médias multipages et fusion, `weblinks.rs` liens web, `import.rs` assistant d'import, `filters.rs` filtres rapides), `editor.rs` (éditeurs visuel et brut), `entry_form.rs` (saisie d'acte), `map.rs` (carte des lieux), `theme.rs` (palette de la maquette), `analyses.rs` (fenêtre Analyses). |

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
