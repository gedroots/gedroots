# Design system GEDRoots

Tiré du kit de marque GEDRoots v1.0 (« graphe organique, minimaliste, violet »). Le code
de référence est [`crates/gedroots/src/theme.rs`](crates/gedroots/src/theme.rs) : les vues
n'écrivent jamais de couleur, de rayon ni de police en dur, elles passent par ses jetons.

| Clair | Sombre |
|---|---|
| ![Thème clair](docs/capture-theme-clair.png) | ![Thème sombre](docs/capture-theme-sombre.png) |

Ce document réunit le design system (jetons, typographie, icônes) et les règles d'interface
tirées des guides [Apple HIG](https://developer.apple.com/design/human-interface-guidelines),
[GNOME HIG](https://developer.gnome.org/hig/) et [Windows / Fluent](https://learn.microsoft.com/windows/apps/design/),
adaptées à GEDRoots. En cas de conflit : les règles du projet ci-dessous l'emportent, puis la
convention de la plateforme où l'app tourne.

## Avant toute maquette

À faire **à chaque fois**, sans attendre qu'on le demande, avant de proposer un écran, une
fenêtre ou un parcours :

1. **Logiciels de généalogie.** Regarder comment au moins trois d'entre eux traitent le sujet,
   en commençant par les plus pertinents :
   - MacFamilyTree : app Mac soignée, arbres et graphes, édition directe dans la fiche ;
   - Family Historian : sources et citations, témoins (rôles), diagrammes, requêtes ;
   - Heredis : référence française, saisie d'actes, lieux et cartes ;
   - RootsMagic : citations, « Problems », gestion des doublons, listes de tâches ;
   - Gramps : modèle de données (événements partagés, rôles), Linux, greffons ;
   - Ancestris : GEDCOM natif, vocabulaire français ;
   - les services web (FamilySearch, Geneanet, Ancestry, Filae) pour la saisie et la
     présentation des enregistrements.
2. **Meilleures apps en général.** Chercher le même motif (liste + détail, recherche, import,
   réglages, édition en place, carte, chronologie…) dans au moins deux apps réputées pour leur
   design : Finder, Photos, Mail, Notes, Plans, Réglages (macOS) ; Things, Linear, Figma, Zed,
   Obsidian, Raycast, Arc ; Fichiers, Fragments, Paramètres (GNOME) ; Explorateur, Photos,
   Paramètres (Windows 11).
3. **Guides de plateforme.** Vérifier le motif dans les HIG Apple, GNOME et Windows, puis
   contre ce document (sections Principes, Interaction et Plateformes).
4. **Rendre la recherche avec la maquette** : un court tableau « app · ce qu'elle fait bien ·
   ce qu'on reprend · ce qu'on évite », avec liens ou captures. Pas de longue dissertation.
5. **Maquetter avec les jetons** de ce document, en clair **et** en sombre, avec les vrais
   libellés français et des données réalistes (dates partielles, noms longs, lieux
   hiérarchisés, champs vides).

Une proposition sans cette recherche n'est pas prête à montrer.

## Principes

- **Fonds neutres, une pointe de violet** : les fonds restent blanc et ocre en clair, gris et
  noir en sombre. Le violet primaire n'apparaît que par touches : action principale, lien,
  sélection, symbole de marque. Jamais de fond ni de grand aplat violet, pas de dégradé.
- **Les données d'abord** : l'écran sert la généalogie, pas l'interface. Pas de chrome
  décoratif, pas de cartes dans des cartes, pas d'ombre sauf pour ce qui flotte (menus,
  popovers, modales).
- **Commandes dans les menus** : pas de barre d'outils. Chaque commande existe une seule fois
  dans la barre de menus (modèle de `commands.rs`), donc aussi dans la palette `⌘⇧P` et, quand
  elle porte sur un élément, dans son menu contextuel avec le même libellé. Les seuls boutons
  visibles sont les petits « + » des en-têtes de section (Événements, Famille, Membres…) et les
  emplacements pointillés de l'arbre.
- **Pas de texte explicatif** : un titre par élément ; les exemples de format vont dans le
  placeholder des champs ; les sections vides sont masquées.
- **Dévoilement progressif** : les champs techniques sont déduits, pas affichés (ex. le `FORM`
  d'un média) ; les actions rares ou destructrices sont repliées ou dans les menus ; les
  identifiants GEDCOM n'apparaissent que via Édition › Copier l'ID et la recherche avancée.
- **Édition en place** : on modifie là où on lit (fiche, événement, case de l'arbre), dans la
  même fenêtre pour créer et modifier. Pas de mode « édition » séparé.
- **Annuler plutôt que confirmer** : toute modification passe par un point d'annulation ; une
  confirmation n'est demandée que pour ce qui ne s'annule pas (fichier écrasé sur le disque…).
- **Une seule barre en haut** : sous macOS, la barre de titre est fondue dans celle de l'app
  (feux tricolores à gauche, déplacement de la fenêtre). Le nom du fichier n'y figure qu'une
  fois, centré, suivi de « • » tant qu'il n'est pas enregistré. Pas de barre d'état : les
  messages s'affichent à droite de la barre du haut.
- **Infobulles** sur tout bouton sans libellé, avec le raccourci quand il existe.
- **Jamais la couleur seule** : lignées, états et sélections ont aussi une forme (point,
  contour, icône, texte). Les couleurs restent lisibles par les daltoniens.

## Couleurs

Le mode suit l'apparence du système et change à chaud. `GEDROOTS_APPEARANCE=light|dark` le
force (captures, tests).

| Jeton (`theme::`) | Rôle | Clair | Sombre |
|---|---|---|---|
| `bg()` | Canevas principal | `#FFFFFF` | `#141414` |
| `panel()` | Barres, panneaux latéraux | `#FAF7F1` | `#1A1A1A` |
| `surface()` | Cartes, champs | `#F4EFE6` | `#222222` |
| `hover()` | Survol, sélection discrète | `#EEE7DA` | `#2B2B2B` |
| `border()` | Séparateurs | `#E6DED0` | `#2E2E2E` |
| `border_strong()` | Contours de champs, cases | `#D5CAB7` | `#3D3D3D` |
| `text()` | Texte principal | `#1F1B16` | `#ECECEC` |
| `muted()` | Texte secondaire | `#6B6257` | `#A3A3A3` |
| `accent()` | Violet : actions, liens, sélection | `#6D3DF0` | `#B997FF` |
| — survol / pressé | Boutons primaires | `#5B2BDD` / `#4C21B8` | `#C8AAFF` / `#A77CF6` |
| `on_accent()` | Texte sur le violet | `#FFFFFF` | `#1A1A1A` |
| `accent_soft()` | Ligne sélectionnée | violet 10 % | violet 16 % |
| `paternal()` | Lignée paternelle | `#3F5BD8` | `#8FA2FF` |
| `maternal()` | Lignée maternelle | `#9C36B5` | `#E39BF0` |
| `ok()` | Succès, « sourcé » | `#1A7346` | `#6FD3A0` |
| `warn()` | À vérifier | `#9A5B00` | `#F0B35A` |
| `danger()` | Erreur, suppression | `#C42B48` | `#F2828E` |
| `scrim()` | Voile sous les modales | noir 35 % | noir 55 % |
| `viewer()` | Fond des visionneuses d'actes | `#0E0E0E` | `#0E0E0E` |
| `image_*()` | Cadres et étiquettes posés sur une photo | couleurs sombres, fond noir 72 % | idem |

Les neutres clairs tirent vers l'ocre, les sombres sont des gris purs.

Le de cujus (Sosa 1) prend le violet ; les deux lignées restent dans la gamme violette
(indigo côté père, orchidée côté mère) au lieu d'introduire d'autres couleurs fortes.

Contrastes (WCAG) : tout texte courant dépasse 4,5:1 sur `bg`, `panel` et `surface` dans les
deux modes (violet sur blanc 5,9:1 ; muted 5,2:1 sur surface ; texte sombre 13,5:1 sur surface). Les couleurs d'état
et de lignée dépassent aussi 4,5:1 sur `bg`.

Le thème des composants gpui-component (boutons, listes, menus, tableaux, champs) est rempli
à partir de ces jetons dans `theme::apply`.

## Typographie

- **Inter**, embarquée dans l'app (Regular, Medium, SemiBold, Bold ; licence SIL OFL dans
  `crates/gedroots/assets/fonts/OFL.txt`). Le rendu est le même sur macOS, Linux et Windows.
- **Chiffres tabulaires** activés sur toute la fenêtre (`theme::tabular_nums()`), pour aligner
  dates et effectifs.
- Graisses : 400 texte, 500 libellés, 600 titres et noms, 700 réservé.
- Échelle (px) : 12 `text_xs` (métadonnées, listes denses) · 14 `text_sm` (texte courant) ·
  16 base · 20 `text_xl` (titres de fiche) · 24 `text_2xl`. Pas de 18 px.
- Libellés en casse de phrase, courts.

## Espacements

Pas de 4 px : 4, 8, 12, 16, 24, 32, 48, 64 — soit `gap_1`, `gap_2`, `gap_3`, `gap_4`, `gap_6`,
`gap_8`… (idem `p_*`, `px_*`, `py_*`). `0p5` (2 px) est toléré pour un écart vertical entre deux
lignes de texte ; pas de `1p5` ni de `2p5`.

## Rayons

| Jeton | Valeur | Usage |
|---|---|---|
| `RADIUS_SM` | 8 px | Champs, puces, vignettes, cases de l'arbre, cartes de section (aussi `Theme.radius`) |
| `RADIUS_MD` | 12 px | Boutons, menus, popovers (`Theme.radius_lg`) |
| `RADIUS_LG` | 20 px | Fenêtres modales (palette de commandes, ajout d'un proche…) |

Le kit prévoit aussi 28 px pour de grands panneaux superposés : à introduire si l'app en a un.
Bordures 1 px `border()` ; focus : anneau `accent()`.

L'anneau de focus (3 px) est peint hors de la bordure du champ : une zone qui défile le coupe si le
champ touche son bord. Une zone qui défile et contient des champs s'écrit donc
`theme::scroll_y(id)` (retrait `FOCUS_ROOM` compensé par une marge négative, l'alignement ne
bouge pas) ou garde un `p_*` intérieur d'au moins 4 px ; jamais un `overflow_y_scroll()` nu
autour d'un champ.

## Icônes

- **Interface** : icônes Lucide (jeu de gpui-kit + `ExtraIcons` dans `main.rs`), 16 px dans les
  listes et en-têtes, 14 px dans le texte, couleur `muted()` ou `accent()` quand actif.
- **Marque** : sources SVG du kit dans `crates/gedroots/assets/brand/`, chargées par
  `img("brand/…")`.
  - `icon-micro.svg` : symbole simplifié pour 16–32 px (barre du haut) ;
  - `icon-primary.svg` : symbole détaillé dès 48 px ;
  - `logo-horizontal-light.svg` / `-dark.svg` : écran d'accueil, selon le mode.
- Ne jamais redessiner, recolorer ni entourer le symbole d'une forme ajoutée.
- L'icône d'application (`.icns`, `.ico`, PNG) relève de `packaging/`.

## Graphes (arbre, éventail, carte)

- Lien de parenté proche : trait plein ; ascendants colorés par lignée, descendants et
  collatéraux neutres (`border_strong()`).
- Sélection : contour `accent()` + fond `accent_soft()`, jamais une simple teinte.
- Liens : courbes ou coudes discrets, épaisseur homogène.

## Interaction

Règles communes aux trois guides, à appliquer partout.

### Navigation

- Structure fixe : barre latérale (zones) → liste → fiche → panneau de détail. La barre
  latérale est à gauche sur les trois plateformes et se masque.
- **Historique** précédent/suivant sur toute la navigation (`workspace/history.rs`) : clavier,
  boutons latéraux de la souris et, sur trackpad, balayage. Chaque liste rouvre le dernier
  élément consulté ; Individus retombe sur l'individu racine.
- Tout lien vers un enregistrement (proche, source citée, lieu, média) est cliquable et ouvre
  sa fiche ; jamais d'impasse.

### Menus et commandes

- Libellés : verbe à l'infinitif pour une commande (« Ajouter un événement »), nom ou adjectif
  pour un réglage coché (« Événements des proches »). Casse de phrase.
- **Points de suspension « … »** quand la commande demande encore quelque chose avant d'agir
  (fenêtre, sélecteur) ; jamais sinon.
- Une commande inapplicable est grisée, pas masquée (seuls les menus contextuels n'affichent
  que ce qui s'applique). Un sous-menu vide est masqué (ex. Ouvrir récent).
- Menus : 3 à 12 entrées, groupées par séparateurs, sous-menus à un seul niveau.
- Menu contextuel : clic droit (et `⇧F10` / touche Menu hors macOS), mêmes libellés que la barre
  de menus, il agit sur l'élément cliqué sans changer la sélection.
- Raccourcis : seulement pour les commandes fréquentes ; reprendre ceux de la plateforme avant
  d'en inventer (voir Plateformes). Le raccourci apparaît dans le menu, la palette et
  l'infobulle.

### Fenêtres et dialogues

- Préférer l'édition en place et les popovers aux modales. Une modale n'est justifiée que pour
  une tâche courte et bloquante (ajouter un proche, choisir un fichier, saisir un acte).
- Boutons de dialogue libellés par leur **verbe** (« Supprimer », « Fusionner »), jamais « OK »
  ou « Oui ». `Entrée` valide le bouton par défaut, `Échap` annule. Le bouton destructif prend
  `danger()` et n'est jamais le bouton par défaut.
- Ordre des boutons selon la plateforme (voir Plateformes).
- Réglages : appliqués immédiatement, sans bouton « Appliquer » ni « OK ».
- Fichiers : dialogues natifs du système (ouvrir, enregistrer, choisir un dossier).

### Retours et états

- Message bref à droite de la barre du haut après une action (« 3 médias rattachés »), en
  `danger()` pour une erreur. Une erreur dit ce qui s'est passé et quoi faire, sans jargon.
- Au-delà d'environ une seconde : indicateur de progression ; au-delà de quelques secondes :
  progression chiffrée et annulable. Ne jamais figer la fenêtre.
- État vide d'une vue entière : icône, titre court et au plus une action, sans paragraphe.
  Une section vide dans une fiche est simplement masquée.
- Glisser-déposer : la cible s'éclaire (`accent_soft()` + contour `accent()`) pendant le survol.

### Clavier et accessibilité

- Tout est faisable au clavier : ordre de tabulation logique, focus toujours visible (anneau
  `accent()`), `Échap` ferme le popover ou la modale ouverte.
- Cibles cliquables d'au moins 24 × 24 px (WCAG 2.2), 28 px conseillés pour les icônes seules.
- Contraste 4,5:1 pour le texte, 3:1 pour les contours de contrôles et les icônes porteuses
  de sens. Jamais la couleur seule (voir Principes).
- Animations courtes (≤ 200 ms) et utiles ; respecter « réduire les animations » du système.
- Tout bouton sans libellé a une infobulle et un nom accessible.

### Textes

- Français soigné : guillemets « », espace insécable avant `: ; ! ?` et dans « », caractère
  `…` (pas trois points), apostrophe typographique `’` dans les libellés.
- Vocabulaire du généalogiste, pas de GEDCOM : « Naissance », « Union », « Source citée »,
  jamais `BIRT`, `FAM`, `SOUR` dans l'interface (sauf vue GEDCOM brute).
- Dates à la française (`12 mars 1854`, `vers 1850`, `entre 1850 et 1855`), calendrier
  républicain compris.
- Libellés courts ; pas de « Veuillez », pas de point final dans les libellés et infobulles.

## Plateformes

Le même modèle de commandes et la même mise en page partout ; seules changent les conventions
ci-dessous. Toute nouvelle fonction se vérifie sur les trois colonnes.

| Sujet | macOS (Apple HIG) | Windows (Fluent) | Linux (GNOME HIG) |
|---|---|---|---|
| Barre de menus | Native, en haut de l'écran, menu « GEDRoots » en premier | Dans la fenêtre, barre du haut | Dans la fenêtre, barre du haut (écart assumé : GNOME préfère un menu principal ☰) |
| Boutons de fenêtre | Feux tricolores à gauche, fondus dans la barre du haut | Réduire / agrandir / fermer à droite | Côté et boutons selon le réglage du système (fermer à droite par défaut) |
| Retour / suivant | Chevrons ‹ › dans la barre du haut, après le bouton de barre latérale | Flèche retour seule, tout à gauche de la barre de titre | Flèche retour au début de la barre d'en-tête |
| Raccourcis retour / suivant | `⌘[` / `⌘]` | `Alt+←` / `Alt+→` | `Alt+←` / `Alt+→` |
| Touche de commande | `⌘` (`⌥` pour les variantes) | `Ctrl` (`Alt` pour les variantes) | `Ctrl` (`Alt` pour les variantes) |
| Rétablir | `⌘⇧Z` | `Ctrl+Y` | `Ctrl+Maj+Z` |
| Réglages | GEDRoots › Réglages… `⌘,` | Fichier › Paramètres… / Options… `Ctrl+,` | Fichier › Préférences `Ctrl+,` |
| À propos, Quitter | Menu GEDRoots ; `⌘Q` | Aide › À propos ; Fichier › Quitter, `Alt+F4` | Aide › À propos ; Fichier › Quitter, `Ctrl+Q` |
| Aide | Menu Aide avec champ de recherche | `F1` | `F1` |
| Plein écran | `⌃⌘F` | `F11` | `F11` |
| Lettres soulignées des menus | Non | Oui (`Alt` + lettre) | Oui (`Alt` + lettre) |
| Ordre des boutons de dialogue | Annuler à gauche, action à droite | Action à gauche, Annuler à droite | Annuler à gauche, action à droite |
| Menu contextuel au clavier | — | `Maj+F10`, touche Menu | `Maj+F10`, touche Menu |
| Barres de défilement | Superposées, selon le réglage système | Fines, s'élargissent au survol | Superposées |

La police (Inter) et les couleurs restent les mêmes partout : l'identité de GEDRoots prime sur
l'apparence de chaque système, mais le comportement suit la plateforme.

### Écarts connus à corriger

État au 2 novembre 2026, à résorber au fil des PR :

- Pas de boutons précédent / suivant visibles dans la barre du haut (seulement clavier, menu
  et boutons de la souris).
- Hors macOS, les menus n'ont pas de lettres soulignées et Réglages ne porte pas le nom de la
  plateforme (Paramètres, Préférences).

## Avant de livrer une interface

- [ ] Recherche « Avant toute maquette » faite et résumée dans la PR.
- [ ] Commandes ajoutées au modèle de menus (donc palette), libellés identiques au clic droit.
- [ ] Aucune couleur, police ni rayon en dur : jetons de `theme.rs` uniquement.
- [ ] Captures en clair et en sombre, avec un vrai fichier (copie) et des données longues ou vides.
- [ ] Parcours au clavier seul ; focus visible ; infobulles sur les icônes.
- [ ] Comportement vérifié pour macOS, Windows et Linux (tableau Plateformes).
- [ ] Pas de texte explicatif, sections vides masquées, identifiants GEDCOM invisibles.

## Ajouter un jeton

Ajouter le rôle dans `Palette` (valeurs claire **et** sombre), une fonction d'accès, puis le
documenter ici. Vérifier le contraste à 4,5:1 sur `bg`, `panel` et `surface`.
