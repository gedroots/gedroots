# Extensions GEDCOM de GEDRoots

GEDRoots stocke ses propres données **dans le fichier `.ged`**, jamais dans un fichier
annexe. Toutes ses balises commencent par `_GR_` et sont déclarées dans l'en-tête, comme
l'exige GEDCOM 7 :

```
0 HEAD
1 SCHMA
2 TAG _GR_IGNORE https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#_gr_ignore
2 TAG _GR_ROOT https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#_gr_root
2 TAG _GR_TPL https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#_gr_tpl
2 TAG _GR_FLD https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#_gr_fld
2 TAG _GR_FILE https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#_gr_file
2 TAG _GR_EVEN https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#_gr_even
2 TAG _GR_MEDIA https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#_gr_media
```

Les autres logiciels ignorent ces balises sans perte de données.

## `_GR_ROOT`

Individu racine de l'arbre : celui qu'ouvre GEDRoots au chargement du fichier et vers
lequel ramène le bouton **Racine**. Se définit depuis la fiche d'un individu,
menu **… → Définir comme racine**.

- **Sous** : `HEAD`, au niveau 1. Une seule occurrence.
- **Valeur** : pointeur vers un `INDI` (`@I1@`).
- **Sous-structures** : aucune.

```
0 HEAD
1 SCHMA
2 TAG _GR_ROOT https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#_gr_root
1 _GR_ROOT @I00001@
```

Sans cette balise (ou si elle pointe vers un individu supprimé), GEDRoots prend le Sosa 1
déclaré par Ancestris (`_SOSADABOVILLE 1`), sinon le premier individu du fichier.
Supprimer l'individu racine retire aussi la balise.

## `_GR_EVEN`

Type d'événement personnalisé déclaré avant d'être utilisé (**Fichier › Réglages du
fichier… › Événements**). Un type personnalisé est un `EVEN` standard avec un `TYPE` :
ceux qui figurent déjà dans le fichier sont déduits de leurs `TYPE` et n'ont pas besoin de
cette balise. Elle ne sert qu'à proposer un type pas encore utilisé dans **Ajouter un
événement…**.

- **Sous** : `HEAD`, au niveau 1. Une occurrence par type déclaré.
- **Valeur** : le libellé du type, tel qu'il sera écrit dans `TYPE`.
- **Sous-structures** : `_GR_REC FAM` pour un événement familial (sous `FAM`) ; absent,
  le type concerne les individus.

```
0 HEAD
1 SCHMA
2 TAG _GR_EVEN https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#_gr_even
2 TAG _GR_REC https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#_gr_even
1 _GR_EVEN Baptême protestant
1 _GR_EVEN Publication des bans
2 _GR_REC FAM
```

Renommer un type dans les réglages réécrit tous les `TYPE` correspondants (renommer vers un
type existant les fusionne) ; un type déclaré et inutilisé peut être retiré.

## `_GR_MEDIA`

Rangement des fichiers médias (**Fichier › Réglages du fichier… › Médias**) : un dossier
racine, puis des dossiers de rangement, chacun avec les types de médias qu'il reçoit et son
modèle de nom. Les chemins restent des `FILE` standard, relatifs au dossier du `.ged`.

- **Sous** : `HEAD`, au niveau 1, une seule occurrence.
- **Valeur** : le dossier racine des médias, relatif au `.ged` (peut être vide).
- **Sous-structures** : un `_GR_DIR <sous-dossier>` par dossier de rangement, dans l'ordre :
  - `_GR_TYPES <liste>` : ce que le dossier reçoit, séparé par des virgules — balises
    d'événements (`BIRT`, `MARR`…), types personnalisés (le `TYPE` d'un `EVEN`), `INDI`
    (média rattaché directement à un individu : portrait) ou `FAM`. Absent : tout le reste.
  - `_GR_NAME <modèle>` : nom des fichiers. Absent : le nom d'origine est gardé.

Un média va dans le premier dossier dont les types contiennent le sien ; sans dossier qui
le reçoive, il reste où il est.

```
0 HEAD
1 SCHMA
2 TAG _GR_MEDIA https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#_gr_media
2 TAG _GR_DIR https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#_gr_media
2 TAG _GR_TYPES https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#_gr_media
2 TAG _GR_NAME https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#_gr_media
1 _GR_MEDIA
2 _GR_DIR Matricules
3 _GR_TYPES military service, military discharge
3 _GR_NAME {sosa:6}_{fratrie}_{nom:simple}_{prenom}_{n}
2 _GR_DIR Actes
3 _GR_TYPES BIRT, BAPM, CHR, MARR, DEAT, BURI
3 _GR_NAME {sosa:6}_{fratrie}_{type:1}_{nom:simple}_{prenom}_{conjoint:simple}_{n}
2 _GR_DIR Portraits
3 _GR_TYPES INDI
3 _GR_NAME {sosa:6}_{nom:simple}_{prenom}
2 _GR_DIR Divers
```

Variables du modèle (`/` crée un sous-dossier) :

| Variable | Valeur |
|---|---|
| `{type}` | type de l'événement (« Naissance », ou le `TYPE` d'un `EVEN`) |
| `{date}`, `{annee}`, `{lieu}` | date (AAAA-MM-JJ), année et commune de l'événement |
| `{nom}`, `{prenom}`, `{prenoms}` | nom, premier prénom, tous les prénoms ; pour un couple, le mari |
| `{conjoint}` | pour un couple, le nom de l'épouse |
| `{sosa}`, `{fratrie}` | numéro Sosa-d'Aboville calculé depuis l'individu racine : `58` et `2` pour le 2e enfant du Sosa 58, `B1` pour le 1er enfant de sa 2e union, `2.3` pour un petit-enfant |
| `{source}`, `{page}`, `{titre}` | titre de la source, cote (`PAGE`), titre du média |
| `{n}` | numéro de page d'un média à plusieurs fichiers (sinon « p2 », « p3 »… en fin de nom) |

Options : `{sosa:6}` complète un nombre par des zéros, `{type:1}` garde les premières
lettres (N, M, D…), `{nom:simple}` retire accents, espaces et apostrophes. Une variable
vide disparaît avec le séparateur qui la précède.

Les variables sont tirées de l'événement qui cite le média, ou une source qui le porte : de
préférence celui que la source décrit (`DATA.EVEN`) ou que son titre nomme (« Acte de
naissance… »), plutôt qu'un attribut (profession, résidence) ; à égalité, celui que le
premier dossier reçoit. À défaut, de la source qui porte le média, puis de la personne ou
de la famille liée. Un fichier rattaché depuis l'extérieur du dossier du `.ged` y est copié
sous ce nom ; **Média › Renommer selon le modèle…** déplace les fichiers d'un média existant
après confirmation.

## `_GR_FILE`

Page d'un média à plusieurs pages visée par un lien. Un média multipage est un
enregistrement `OBJE` standard avec plusieurs `FILE`, un par page, dans l'ordre :

```
0 @M1@ OBJE
1 FILE Actes/acte_1.jpg
2 FORM image/jpeg
2 TITL Acte de mariage de Michel OHL et Jeanne BOUST
1 FILE Actes/acte_2.jpg
2 FORM image/jpeg
```

GEDCOM 7 applique le `CROP` d'un lien au premier fichier seulement. Pour situer une
personne sur une autre page (cadre tracé sur la page 2 d'un acte), GEDRoots ajoute
`_GR_FILE` au lien :

- **Sous** : un lien vers un média (`n OBJE @M@`), à côté de `CROP`.
- **Valeur** : numéro de la page (du `FILE`), à partir de 1. Jamais écrit pour la page 1.
- **Sous-structures** : aucune.

```
0 @I1@ INDI
1 OBJE @M1@
2 CROP
3 TOP 120
3 LEFT 340
3 HEIGHT 260
3 WIDTH 200
2 _GR_FILE 2
```

Un lien sans `CROP` ni `_GR_FILE` concerne le document entier. Un logiciel qui ignore
l'extension applique le cadre à la première page.

Fusionner des médias (menu **Média › Fusionner…**, ou ⌘ / Maj + clic dans la liste des
médias puis clic droit), ajouter, déplacer, retirer ou séparer une page (**Média › Pages**)
met à jour `_GR_FILE` et tous les pointeurs du fichier, citations comprises.

## `_GR_IGNORE`

Masque une anomalie de la page **Qualité des données** que l'utilisateur a vérifiée et
jugée correcte, par exemple un mariage réellement célébré très jeune (clic droit sur
l'anomalie › **Masquer** ; **Affichage › Filtrer la liste › Anomalies masquées** les
réaffiche, et **Ne plus masquer** retire la balise).

- **Sous** : n'importe quel enregistrement (`INDI`, `FAM`…), au niveau 1.
- **Valeur** : le code de l'anomalie (voir ci-dessous). Une ligne par anomalie masquée.
- **Sous-structures** : aucune.

```
0 @I263@ INDI
1 NAME Antoine /GIRAUDON/
1 _GR_IGNORE marriage_too_young
```

Les anomalies masquées se retrouvent dans **Fichier › Réglages du fichier…**, d'où on peut les
réafficher (la balise est retirée, et sa déclaration aussi quand il n'en reste plus).

L'anomalie est masquée lorsque son enregistrement principal (celui qu'ouvre un clic dans
la liste) porte la balise avec le bon code.

| Code | Anomalie |
| --- | --- |
| `death_before_birth` | Décès avant la naissance |
| `burial_before_death` | Inhumation avant le décès |
| `event_before_birth` | Événement avant la naissance |
| `event_after_death` | Événement après le décès |
| `too_old` | Durée de vie supérieure à 110 ans |
| `parent_too_young` | Parent de moins de 14 ans à la naissance de l'enfant |
| `father_too_old` | Père de plus de 80 ans à la naissance |
| `mother_too_old` | Mère de plus de 55 ans à la naissance |
| `birth_after_mother_death` | Naissance après le décès de la mère |
| `birth_after_father_death` | Naissance plus de 10 mois après le décès du père |
| `child_before_parent` | Enfant né avant son parent |
| `marriage_too_young` | Mariage avant 14 ans |
| `marriage_after_death` | Mariage après le décès d'un époux |
| `marriage_age_gap` | Plus de 30 ans d'écart entre époux |
| `siblings_too_close` | Deux naissances (hors jumeaux) à moins de 8 mois |
| `husband_not_male` / `wife_not_female` | Sexe incohérent avec le rôle dans la famille |
| `own_ancestor` | Personne présente dans sa propre ascendance |
| `broken_link` | Pointeur vers un enregistrement absent |
| `missing_backlink` | `FAMC`/`FAMS` sans le `CHIL`/`HUSB`/`WIFE` correspondant |
| `unsourced` | Événement sans source |
| `tag_not_allowed` | Balise standard non permise à cet endroit par le schéma GEDCOM 7 |
| `missing_required` | Sous-structure obligatoire absente (ex. `NAME` d'un dépôt) |
| `too_many` | Sous-structure présente plusieurs fois alors qu'une seule est permise |
| `wrong_pointer` | Pointeur vers un enregistrement du mauvais type |
| `invalid_value` | Valeur invalide : date illisible ou inexistante, valeur énumérée inconnue… |
| `empty_fact` | Événement sans aucune donnée (ni valeur ni sous-structure) |
| `empty_note` | Note vide (`NOTE` sans texte, ou note partagée `SNOTE` vide) |
| `empty_family` | Famille sans aucun membre |
| `unused_record` | Média, source, dépôt ou note partagée cité nulle part |
| `disconnected` | Individu (ou groupe d'individus) sans aucun lien familial ni `ASSO` avec l'individu racine |

| `role_sex_mismatch` | Rôle `FATH`/`HUSB` d'une femme ou `MOTH`/`WIFE` d'un homme dans une citation |
| `outside_source_period` | Acte daté hors de la période que sa source déclare pour ce type d'acte |

Le contrôle du schéma GEDCOM 7 ne s'applique qu'aux fichiers en version 7 ; les balises
d'extension (`_XXX`) ne sont jamais signalées.

Les dates imprécises (`ABT`, `BEF`, `BET … AND …`, année seule) sont traitées comme des
intervalles : une anomalie n'est signalée que si elle est certaine pour toutes les dates
possibles. Les dates « vers » ont une marge de 5 ans.

## Types des sources et des médias (sans extension)

GEDRoots type les sources et les médias avec les structures standard de GEDCOM 7,
que les autres logiciels lisent :

| Donnée | GEDCOM écrit |
| --- | --- |
| Contenu d'une source : types d'actes, période (fiche source, champs **Actes** et **Période**) | `SOUR` / `DATA` / `EVEN BIRT, DEAT` + `DATE FROM 1793 TO 1802` (+ `PLAC`) |
| Support d'un média (fiche média, liste **Support**) | `OBJE` / `FILE` / `FORM` / `MEDI PHOTO`, `MANUSCRIPT`, `TOMBSTONE`… |
| Portrait d'un individu (clic droit sur une image › **Définir comme portrait**) | son lien `OBJE` passe en tête de ses médias ; le média devient `MEDI PHOTO` s'il n'avait pas de support |

La saisie d'un acte ajoute son type au contenu de la source et élargit la période à
l'année de l'acte ; l'image d'un acte saisi est un `MANUSCRIPT`.

Portrait affiché : la première photo (`MEDI PHOTO`) de l'individu, sinon une image sans
support rangée dans un dossier « Portraits » ou titrée « Portrait », sinon une image sans
support qui n'est pas le scan d'une source, sinon sa première image.

L'assistant d'import convertit `_PRIM Y` (photo principale de Family Tree Maker, Legacy,
MyHeritage…) : le lien marqué passe en tête des médias et la balise est retirée.

## Saisie guidée des actes (`_GR_TPL`, `_GR_FLD`…)

La saisie d'un acte (menu **Saisir un acte**, `⌘E` / `Ctrl+E`) produit uniquement des
structures standard :

| Donnée saisie | GEDCOM écrit |
| --- | --- |
| Événement de l'acte | `BIRT`, `BAPM`, `MARR` (sur la famille), `DEAT`, `BURI`, `CENS`, `EVEN` + `TYPE` |
| Référence (n° d'acte, vue…) | `SOUR @S@` / `PAGE` |
| Date de l'acte, transcription | `SOUR` / `DATA` / `DATE`, `TEXT` |
| Rôle de chaque personne | `SOUR` / `EVEN <tag>` / `ROLE CHIL, FATH, MOTH, HUSB, WIFE, WITN, GODP…` (+ `PHRASE`) |
| Témoins, parrains, déclarants | `ASSO @I@` / `ROLE` sur l'événement principal |
| Âge déclaré | `AGE` (`32y`), `HUSB`/`WIFE` / `AGE` pour un mariage ; naissance estimée `BIRT` / `DATE EST 1880` |
| Profession, domicile | `OCCU` et `RESI` datés et sourcés |
| Qualité | `QUAY` 0 à 3 |
| Image de l'acte | `OBJE` lié à la source et à chaque citation |
| Personne simplement mentionnée | `NOTE` de la citation principale |

### `_GR_TPL`

**Sur une source** (`0 @S@ SOUR` / `1 _GR_TPL <id>`) : modèle d'acte utilisé pour la saisie,
repris automatiquement quand on saisit un nouvel acte depuis cette source (comme Family
Historian mémorise le modèle d'une source). La valeur est l'identifiant d'un modèle intégré
(`NAISSANCE`, `BAPTEME`, `MARIAGE`, `DECES`, `INHUMATION`, `RECENSEMENT`, `MATRICULE`) ou un
pointeur vers un modèle du fichier (`@T1@`).

**Enregistrement de niveau 0** (`0 @T1@ _GR_TPL`) : modèle d'acte personnalisé.

```
0 @T1@ _GR_TPL
1 NAME Décès (table décennale)
1 _GR_EVT DEAT
1 _GR_PAGE Table {annee}, vue {vue}
1 _GR_FLD annee
2 NAME Années
1 _GR_FLD vue
2 NAME Vue
1 _GR_ROLE defunt
2 NAME Défunt(e)
2 ROLE PRIN
3 PHRASE Défunt
2 _GR_REL PRIN
2 _GR_PF SURN GIVN SEX AGE RESI
2 _GR_MULT DEFAULT
1 _GR_ROLE conjoint
2 NAME Conjoint
2 ROLE SPOU
2 _GR_REL SPOU defunt
2 _GR_PF SURN GIVN
```

### `_GR_EVT`

Balise de l'événement créé par le modèle (`BIRT`, `MARR`, `CENS`, `EVEN`…), avec un `TYPE`
facultatif pour `EVEN` (ex. « Registre matricule »).

### `_GR_PAGE`

Formule de la référence (`PAGE`) des citations. `{clé}` est remplacé par le champ du même
nom ; un segment (entre virgules) dont un champ est vide est omis :
`Acte n° {numero}, vue {vue}` → `Acte n° 112` si la vue n'est pas renseignée.

### `_GR_FLD`

- Dans un modèle : un champ de la citation (`1 _GR_FLD numero` / `2 NAME N° d'acte`).
- Dans une citation : la valeur saisie, `clé valeur` (`3 _GR_FLD numero 112`), pour retrouver
  les champs structurés de l'acte (la valeur lisible est dans `PAGE`).

### `_GR_ROLE`

Rôle d'une personne citée dans l'acte. Valeur : clé du rôle dans le modèle. Sous-structures :
`NAME` (libellé), `ROLE` (+ `PHRASE`) écrit dans les citations, `SEX` imposé, et les
balises suivantes.

### `_GR_REL`

Lien du rôle avec les autres rôles, qui détermine les liens familiaux créés :

| Valeur | Effet |
| --- | --- |
| `PRIN` | Porte l'événement (ou, pour un mariage, forme le couple) |
| `FATH <rôle>` / `MOTH <rôle>` | Père / mère de la personne du rôle indiqué (famille créée ou complétée) |
| `SPOU <rôle>` | Conjoint de la personne du rôle indiqué |
| `HOUS <rôle>` | Membre du ménage : conjoint ou enfant d'après le « lien déclaré » (recensements) |
| `ASSO` | Participant associé à l'événement (`ASSO` + `ROLE`) |

### `_GR_PF`

Champs saisis pour ce rôle, séparés par des espaces : `SURN` nom, `GIVN` prénoms, `SEX`,
`AGE` âge déclaré, `OCCU` profession, `RESI` domicile, `BIRP` lieu de naissance déclaré,
`RELA` lien déclaré, `NOTE`.

### `_GR_MULT`

Options du rôle : `MULTIPLE` (peut apparaître plusieurs fois, ex. témoins) et `DEFAULT`
(présent dans un nouvel acte).
