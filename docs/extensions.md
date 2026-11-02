# Extensions GEDCOM de GEDRoots

GEDRoots stocke ses propres données **dans le fichier `.ged`**, jamais dans un fichier
annexe. Toutes ses balises commencent par `_GR_` et sont déclarées dans l'en-tête, comme
l'exige GEDCOM 7 :

```
0 HEAD
1 SCHMA
2 TAG _GR_IGNORE https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#_gr_ignore
2 TAG _GR_ROOT https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#_gr_root
2 TAG _GR_FILE https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#_gr_file
2 TAG _GR_EVEN https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#_gr_even
2 TAG _GR_MEDIA https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#_gr_media
```

Les autres logiciels ignorent ces balises sans perte de données.

## `_GR_ROOT`

Individu racine de l'arbre : celui qu'ouvre GEDRoots au chargement du fichier et vers
lequel ramène **Individu › Aller à l'individu racine**. Se définit dans
**Réglages du fichier › Général**.

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

Type d'événement personnalisé (**Fichier › Réglages du fichier… › Événements**). Un type
personnalisé est un `EVEN` standard avec un `TYPE` : ceux qui figurent déjà dans le fichier
sont déduits de leurs `TYPE` et n'ont pas besoin de cette balise. Elle ne sert qu'à proposer
un type pas encore utilisé dans **Ajouter un événement…**, ou à afficher un type sous un
autre libellé que son `TYPE`.

- **Sous** : `HEAD`, au niveau 1. Une occurrence par type déclaré.
- **Valeur** : le `TYPE` des événements de ce type, tel qu'il est écrit dans le fichier.
- **Sous-structures** :
  - `_GR_REC FAM` pour un événement familial (sous `FAM`) ; absent, le type concerne
    les individus.
  - `_GR_LABEL <libellé>` : libellé affiché par GEDRoots à la place du `TYPE`. Les
    événements gardent leur `TYPE`.

```
0 HEAD
1 SCHMA
2 TAG _GR_EVEN https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#_gr_even
2 TAG _GR_REC https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#_gr_even
2 TAG _GR_LABEL https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#_gr_even
1 _GR_EVEN Baptême protestant
1 _GR_EVEN Publication des bans
2 _GR_REC FAM
1 _GR_EVEN military service
2 _GR_LABEL Service militaire
0 @I1@ INDI
1 EVEN
2 TYPE military service
2 DATE FROM 1914 TO 1918
```

Renommer un `TYPE` dans les réglages réécrit tous les `TYPE` correspondants (renommer vers
un type existant les fusionne) et garde son libellé ; un type déclaré et inutilisé peut être
retiré. Une déclaration devenue inutile (type utilisé, sans libellé) est retirée.

À l'import (**Fichier › Importer un GEDCOM…**), les événements à balise d'extension d'autres
logiciels (`_MILT`, `_MILI`, `_DEG`, `_FUN`…, ou toute balise `_XXX` datée ou localisée
sous un individu ou une famille) deviennent des `EVEN` + `TYPE`.

## `_GR_MEDIA`

Rangement des fichiers médias (**Fichier › Réglages du fichier… › Médias**) : le dossier
des médias, puis des sous-dossiers, chacun avec les événements qu'il reçoit et son modèle
de nom. Les chemins restent des `FILE` standard, relatifs au dossier du `.ged`.

- **Sous** : `HEAD`, au niveau 1, une seule occurrence.
- **Valeur** : le dossier des médias, relatif au `.ged` (peut être vide).
- **Sous-structures** : un `_GR_DIR <sous-dossier>` par sous-dossier, dans l'ordre :
  - `_GR_TYPES <liste>` : ce que le dossier reçoit, séparé par des virgules — balises
    d'événements (`BIRT`, `MARR`…), types personnalisés (le `TYPE` d'un `EVEN`), `INDI`
    (média rattaché directement à un individu : portrait) ou `FAM`. Absent : tout le reste.
  - `_GR_NAME <modèle>` : nom des fichiers. Absent : le nom d'origine est gardé.

Un média va dans le premier sous-dossier dont les types contiennent le sien ; sans
sous-dossier qui le reçoive, il reste où il est.

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

Les variables sont tirées de l'événement qui porte le média (`2 OBJE`), le cite, ou cite une
source qui le porte : de
préférence celui que la source décrit (`DATA.EVEN`) ou que son titre nomme (« Acte de
naissance… »), plutôt qu'un attribut (profession, résidence) ; à égalité, celui que le
premier sous-dossier reçoit. À défaut, de la source qui porte le média, puis de la personne ou
de la famille liée. Un fichier importé par **Édition › Médias…** (⌘⇧M) est déplacé (ou copié,
au choix) sous ce nom, l'aperçu montrant le nom avant l'import ; un nom déjà pris reçoit
« (2) », « (3) »… **Média › Renommer selon le modèle…** déplace les fichiers d'un média
existant après confirmation.

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
| `missing_file` | Fichier d'un média introuvable sur le disque |
| `moved_file` | Fichier introuvable à son chemin, mais un seul fichier du même nom existe dans le dossier du `.ged` |
| `duplicate_media` | Deux médias désignent exactement les mêmes fichiers |
| `missing_form` | `FILE` sans `FORM` (remplace `missing_required` pour ce cas) |
| `unattached_file` | Fichier du dossier du `.ged` qu'aucun média ne désigne (ne peut pas être masqué : il n'a pas d'enregistrement) |
| `missing_coords` | Lieu sans coordonnées (`PLAC.MAP`) |
| `bad_coords` | Coordonnées illisibles ou incomplètes |
| `place_variant` | Même lieu écrit autrement (casse, accents, « St »/« Saint », codes), ou forme abrégée d'un seul autre lieu |
| `place_hierarchy` | Une même division rattachée à deux divisions supérieures (« Loire » en « Rhône-Alpes » et en « Auvergne-Rhône-Alpes ») ; « Harmoniser » reprend la plus fréquente |

Les anomalies de fichiers et de lieux ne peuvent pas être masquées : elles ne portent sur
aucun enregistrement où écrire `_GR_IGNORE`.

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

## Actes des sources (sans extension)

Un acte se saisit dans la fiche de sa source (« + » à côté de **Actes**, ou **Saisir un acte**,
`⌘E` / `Ctrl+E`). Il n'y a pas de modèle : on choisit l'événement, puis les personnes avec leur
rôle GEDCOM 7. L'enregistrement produit uniquement des structures standard :

| Donnée saisie | GEDCOM écrit |
| --- | --- |
| Événement de l'acte | n'importe quel événement : `BIRT`, `BAPM`, `DEAT`, `CENS`… sur chaque personne principale, `MARR`, `DIV`… sur la famille des époux, `EVEN` + `TYPE` |
| Référence (n° d'acte, vue…) | `SOUR @S@` / `PAGE` |
| Date de l'acte, transcription | `SOUR` / `DATA` / `DATE`, `TEXT` |
| Rôle de chaque personne | `SOUR` / `EVEN <tag>` / `ROLE CHIL, FATH, MOTH, HUSB, WIFE, SPOU, WITN, GODP, OFFICIATOR, CLERGY, NGHBR, FRIEND, OTHER` (+ `PHRASE` : « Parrain », « Père de l'épouse », lien déclaré) |
| Témoins, parrains, déclarants… | `ASSO @I@` / `ROLE` sur l'événement principal |
| Âge déclaré | `AGE` (`32y`), `HUSB`/`WIFE` / `AGE` pour un mariage ; naissance estimée `BIRT` / `DATE EST 1880` |
| Profession, domicile, lieu de naissance déclaré | `OCCU`, `RESI` datés et `BIRT` / `PLAC`, chacun sourcé |
| Qualité | `QUAY` 0 à 3 |
| Image de l'acte | `OBJE` lié à la source et à chaque citation |
| Personne simplement mentionnée | `NOTE` de la citation principale (`Témoin : Jean DUPONT, 32 ans`) |

Les liens familiaux découlent des rôles : `FATH` / `MOTH` d'une personne principale la
rattachent à leur famille, `SPOU` forme un couple, et dans un ménage (plusieurs personnes
principales, ex. un recensement) le lien déclaré (« épouse », « fils »…) relie chacun à la
première.

**Liste des actes d'une source.** GEDRoots la reconstitue à partir des citations des individus
et des familles qui désignent la source, regroupées par `PAGE` et `DATA.DATE` : la personne
principale est celle dont l'événement porte la citation, les autres sont lues dans leur
`ROLE`. Les citations écrites par d'autres logiciels apparaissent donc aussi. Rouvrir un acte
remplace ses citations ; le supprimer retire ses citations et ses `ASSO`, en laissant les
événements et les individus.

### Anciennes balises `_GR_TPL`, `_GR_FLD`…

Les versions précédentes de GEDRoots décrivaient des modèles d'actes (`0 @T1@ _GR_TPL`,
`1 _GR_TPL` sur une source) et recopiaient les champs d'un acte dans ses citations
(`_GR_FLD`), avec un rôle non standard `PRIN`. Elles ne sont plus écrites. L'assistant d'import
(**Fichier › Importer un GEDCOM…**) les propose au nettoyage : les modèles et ces balises sont
retirés, ainsi que leur déclaration dans `HEAD.SCHMA` ; les données de l'acte restent dans
`PAGE`, `EVEN` et `DATA`.
