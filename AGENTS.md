# Instructions pour les agents

GEDRoots est un logiciel de généalogie libre et local (Rust, GPUI via gpui-kit) dont les données
restent dans un fichier **GEDCOM 7**. Ces règles valent pour tout agent qui travaille dans ce
dépôt (Claude Code, Codex, Copilot…). Antonin écrit en français : l'interface, les messages de
commit, les PR et les réponses sont en français.

## Design : à lire avant toute interface

[`design.md`](design.md) est la référence unique : démarche de recherche, principes, jetons
(`crates/gedroots/src/theme.rs`), règles d'interaction et différences entre macOS, Windows et
Linux. Le lire **en entier** avant de proposer une maquette ou de toucher à une vue.

Avant toute proposition de design ou maquette, sans attendre qu'on le demande :

1. regarder comment les meilleurs logiciels de généalogie traitent le sujet ;
2. regarder comment les meilleures apps en général traitent le même motif ;
3. confronter le résultat aux HIG Apple, GNOME et Windows et à `design.md` ;
4. présenter cette recherche (courte, avec liens ou captures) avec la maquette.

Le détail (quelles apps, quoi rendre) est dans [design.md › Avant toute maquette](design.md#avant-toute-maquette).

## Règles du projet

- **GEDCOM 7 standard d'abord.** Ce que l'app doit retenir (individu racine, réglages du
  fichier…) va dans le `.ged` sous forme de balises `_GR_*` déclarées dans `HEAD.SCHMA` et
  documentées dans [`docs/extensions.md`](docs/extensions.md), jamais dans un fichier annexe.
  Pas de GEDCOM X.
- **Fonctions génériques.** Rien n'est taillé pour un fichier particulier : les corrections de
  données passent par l'assistant d'import (`crates/gedcom/src/import.rs`, contrôles
  enfichables).
- **Multiplateforme** (macOS, Windows, Linux) : préférer les solutions qui marchent partout.
- **Commandes** : chaque commande vit dans le modèle de menus (`crates/gedroots/src/commands.rs`)
  et donc dans la palette `⌘⇧P` ; pas de barre d'outils (voir `design.md`).
- **Identifiants GEDCOM cachés** dans l'interface, sauf Édition › Copier l'ID et la recherche
  avancée.
- **Tester sur une copie** d'un vrai fichier, jamais sur l'original. Ne jamais committer de `.ged`
  contenant des personnes vivantes.
- **Une PR à la fois**, propre, fusionnée seulement quand Antonin l'approuve.
- Outils et CI : `mise` (`mise.toml`). Avant de pousser : `mise run ci` (formatage, tests,
  clippy) ; `mise run app fichier.ged` lance l'app.
