# Extensions GEDCOM de GedRoots

GedRoots stocke ses propres données **dans le fichier `.ged`**, jamais dans un fichier
annexe. Toutes ses balises commencent par `_GR_` et sont déclarées dans l'en-tête, comme
l'exige GEDCOM 7 :

```
0 HEAD
1 SCHMA
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
