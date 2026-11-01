# Extensions GEDCOM de GedRoots

GedRoots n'écrit **rien à côté** du fichier `.ged` : ce qui ne rentre pas dans GEDCOM 7
standard est stocké sous forme de balises d'extension (préfixe `_GR`), déclarées dans
`HEAD.SCHMA` comme l'exige la norme :

```
0 HEAD
1 GEDC
2 VERS 7.0
1 SCHMA
2 TAG _GRTPL https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#grtpl
2 TAG _GRFLD https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#grfld
…
```

Un logiciel qui ne les connaît pas peut les ignorer : toute l'information généalogique
(individus, événements, liens familiaux, citations) est écrite en GEDCOM 7 standard.

## Saisie guidée des actes

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

### `_GRTPL`

**Sur une source** (`0 @S@ SOUR` / `1 _GRTPL <id>`) : modèle d'acte utilisé pour la saisie,
repris automatiquement quand on saisit un nouvel acte depuis cette source (comme Family
Historian mémorise le modèle d'une source). La valeur est l'identifiant d'un modèle intégré
(`NAISSANCE`, `BAPTEME`, `MARIAGE`, `DECES`, `INHUMATION`, `RECENSEMENT`, `MATRICULE`) ou un
pointeur vers un modèle du fichier (`@T1@`).

**Enregistrement de niveau 0** (`0 @T1@ _GRTPL`) : modèle d'acte personnalisé.

```
0 @T1@ _GRTPL
1 NAME Décès (table décennale)
1 _GREVT DEAT
1 _GRPAGE Table {annee}, vue {vue}
1 _GRFLD annee
2 NAME Années
1 _GRFLD vue
2 NAME Vue
1 _GRROLE defunt
2 NAME Défunt(e)
2 ROLE PRIN
3 PHRASE Défunt
2 _GRREL PRIN
2 _GRPF SURN GIVN SEX AGE RESI
2 _GRMULT DEFAULT
1 _GRROLE conjoint
2 NAME Conjoint
2 ROLE SPOU
2 _GRREL SPOU defunt
2 _GRPF SURN GIVN
```

### `_GREVT`

Balise de l'événement créé par le modèle (`BIRT`, `MARR`, `CENS`, `EVEN`…), avec un `TYPE`
facultatif pour `EVEN` (ex. « Registre matricule »).

### `_GRPAGE`

Formule de la référence (`PAGE`) des citations. `{clé}` est remplacé par le champ du même
nom ; un segment (entre virgules) dont un champ est vide est omis :
`Acte n° {numero}, vue {vue}` → `Acte n° 112` si la vue n'est pas renseignée.

### `_GRFLD`

- Dans un modèle : un champ de la citation (`1 _GRFLD numero` / `2 NAME N° d'acte`).
- Dans une citation : la valeur saisie, `clé valeur` (`3 _GRFLD numero 112`), pour retrouver
  les champs structurés de l'acte (la valeur lisible est dans `PAGE`).

### `_GRROLE`

Rôle d'une personne citée dans l'acte. Valeur : clé du rôle dans le modèle. Sous-structures :
`NAME` (libellé), `ROLE` (+ `PHRASE`) écrit dans les citations, `SEX` imposé, et les
balises suivantes.

### `_GRREL`

Lien du rôle avec les autres rôles, qui détermine les liens familiaux créés :

| Valeur | Effet |
| --- | --- |
| `PRIN` | Porte l'événement (ou, pour un mariage, forme le couple) |
| `FATH <rôle>` / `MOTH <rôle>` | Père / mère de la personne du rôle indiqué (famille créée ou complétée) |
| `SPOU <rôle>` | Conjoint de la personne du rôle indiqué |
| `HOUS <rôle>` | Membre du ménage : conjoint ou enfant d'après le « lien déclaré » (recensements) |
| `ASSO` | Participant associé à l'événement (`ASSO` + `ROLE`) |

### `_GRPF`

Champs saisis pour ce rôle, séparés par des espaces : `SURN` nom, `GIVN` prénoms, `SEX`,
`AGE` âge déclaré, `OCCU` profession, `RESI` domicile, `BIRP` lieu de naissance déclaré,
`RELA` lien déclaré, `NOTE`.

### `_GRMULT`

Options du rôle : `MULTIPLE` (peut apparaître plusieurs fois, ex. témoins) et `DEFAULT`
(présent dans un nouvel acte).
