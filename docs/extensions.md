# Extensions GEDCOM de GedRoots

GedRoots stocke ses propres données **dans le fichier `.ged`**, jamais dans un fichier
annexe. Toutes ses balises commencent par `_GR_` et sont déclarées dans l'en-tête, comme
l'exige GEDCOM 7 :

```
0 HEAD
1 SCHMA
2 TAG _GR_IGNORE https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#_gr_ignore
2 TAG _GR_ROOT https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#_gr_root
```

Les autres logiciels ignorent ces balises sans perte de données.

## `_GR_ROOT`

Individu racine de l'arbre : celui qu'ouvre GedRoots au chargement du fichier et vers
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

Sans cette balise (ou si elle pointe vers un individu supprimé), GedRoots prend le Sosa 1
déclaré par Ancestris (`_SOSADABOVILLE 1`), sinon le premier individu du fichier.
Supprimer l'individu racine retire aussi la balise.

## `_GR_IGNORE`

Masque une anomalie du contrôle de cohérence (fenêtre **Analyses → Cohérence**) que
l'utilisateur a vérifiée et jugée correcte, par exemple un mariage réellement célébré
très jeune.

- **Sous** : n'importe quel enregistrement (`INDI`, `FAM`…), au niveau 1.
- **Valeur** : le code de l'anomalie (voir ci-dessous). Une ligne par anomalie masquée.
- **Sous-structures** : aucune.

```
0 @I263@ INDI
1 NAME Antoine /GIRAUDON/
1 _GR_IGNORE marriage_too_young
```

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
| `unsourced` | Événement sans source (signalé seulement sur demande) |

Les dates imprécises (`ABT`, `BEF`, `BET … AND …`, année seule) sont traitées comme des
intervalles : une anomalie n'est signalée que si elle est certaine pour toutes les dates
possibles. Les dates « vers » ont une marge de 5 ans.
