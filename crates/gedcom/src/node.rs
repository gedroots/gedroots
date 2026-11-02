//! Arbre GEDCOM générique et sans perte.
//!
//! Chaque ligne GEDCOM devient un [`Node`] ; le niveau est implicite (profondeur
//! dans l'arbre). Rien n'est interprété ni normalisé à ce stade : les balises
//! inconnues ou propriétaires (`_SOSADABOVILLE`, `_TODO`…) sont conservées
//! telles quelles, ce qui permet de réécrire le fichier à l'identique.

/// Une structure GEDCOM : une ligne et ses sous-structures.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Node {
    /// Identifiant croisé sans les `@` (ex. `I307`), seulement sur les enregistrements.
    pub xref: Option<String>,
    pub tag: String,
    /// Valeur brute de la ligne, telle qu'écrite après la balise.
    pub value: Option<String>,
    pub children: Vec<Node>,
}

impl Node {
    pub fn new(tag: impl Into<String>) -> Self {
        Self { tag: tag.into(), ..Default::default() }
    }

    pub fn with_value(tag: impl Into<String>, value: impl Into<String>) -> Self {
        Self { tag: tag.into(), value: Some(value.into()), ..Default::default() }
    }

    pub fn with_xref(mut self, xref: impl Into<String>) -> Self {
        self.xref = Some(xref.into());
        self
    }

    pub fn push(mut self, child: Node) -> Self {
        self.children.push(child);
        self
    }

    /// Valeur de la ligne, chaîne vide si absente.
    pub fn val(&self) -> &str {
        self.value.as_deref().unwrap_or("")
    }

    /// Si la valeur est un pointeur `@X@`, renvoie `X`.
    pub fn pointer(&self) -> Option<&str> {
        let v = self.value.as_deref()?;
        if v.len() > 2 && v.starts_with('@') && v.ends_with('@') && !v.starts_with("@@") {
            Some(&v[1..v.len() - 1])
        } else {
            None
        }
    }

    pub fn child(&self, tag: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.tag == tag)
    }

    pub fn child_mut(&mut self, tag: &str) -> Option<&mut Node> {
        self.children.iter_mut().find(|c| c.tag == tag)
    }

    pub fn children_with<'a>(&'a self, tag: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |c| c.tag == tag)
    }

    /// Valeur d'une sous-structure par chemin, ex. `child_val(&["BIRT", "DATE"])`.
    pub fn path_val(&self, path: &[&str]) -> Option<&str> {
        let mut n = self;
        for tag in path {
            n = n.child(tag)?;
        }
        n.value.as_deref()
    }

    pub fn child_val(&self, tag: &str) -> Option<&str> {
        self.child(tag).and_then(|c| c.value.as_deref())
    }

    /// Texte complet d'une valeur multi-ligne (valeur + `CONT`, et `CONC` des
    /// fichiers 5.5.1 importés).
    pub fn text(&self) -> String {
        let mut s = self.val().to_string();
        for c in &self.children {
            match c.tag.as_str() {
                "CONT" => {
                    s.push('\n');
                    s.push_str(c.val());
                }
                "CONC" => s.push_str(c.val()),
                _ => {}
            }
        }
        s
    }

    /// Remplace la valeur par un texte multi-ligne (découpé en `CONT`).
    pub fn set_text(&mut self, text: &str) {
        self.children.retain(|c| c.tag != "CONT" && c.tag != "CONC");
        let mut lines = text.split('\n');
        let first = lines.next().unwrap_or("");
        self.value = if first.is_empty() && !text.contains('\n') { None } else { Some(first.to_string()) };
        let conts: Vec<Node> = lines
            .map(|l| Node { tag: "CONT".into(), value: (!l.is_empty()).then(|| l.to_string()), ..Default::default() })
            .collect();
        // Les CONT doivent précéder les autres sous-structures.
        for (i, c) in conts.into_iter().enumerate() {
            self.children.insert(i, c);
        }
    }

    /// Définit (ou crée, ou supprime si vide) la valeur d'une sous-structure.
    pub fn set_child_val(&mut self, tag: &str, value: &str) {
        let value = value.trim();
        if value.is_empty() {
            self.children.retain(|c| c.tag != tag);
            return;
        }
        match self.child_mut(tag) {
            Some(c) => c.value = Some(value.to_string()),
            None => self.insert_child(Node::with_value(tag, value)),
        }
    }

    /// Ajoute une sous-structure avant les métadonnées de fin (`CREA`, `CHAN`).
    pub fn insert_child(&mut self, child: Node) {
        let at = self.children.iter().position(|c| c.tag == "CREA" || c.tag == "CHAN").unwrap_or(self.children.len());
        self.children.insert(at, child);
    }

    /// Comme [`Node::set_child_val`] mais pour une valeur multi-ligne.
    pub fn set_child_text(&mut self, tag: &str, text: &str) {
        if text.trim().is_empty() {
            self.children.retain(|c| c.tag != tag);
            return;
        }
        if self.child(tag).is_none() {
            self.insert_child(Node::new(tag));
        }
        self.child_mut(tag).unwrap().set_text(text);
    }

    /// Renvoie la sous-structure `tag`, en la créant si besoin.
    pub fn ensure_child(&mut self, tag: &str) -> &mut Node {
        let i = match self.children.iter().position(|c| c.tag == tag) {
            Some(i) => i,
            None => {
                self.insert_child(Node::new(tag));
                self.children.iter().position(|c| c.tag == tag).unwrap()
            }
        };
        &mut self.children[i]
    }

    /// Parcourt l'arbre en profondeur (ce nœud compris).
    pub fn walk<'a>(&'a self, f: &mut impl FnMut(&'a Node)) {
        f(self);
        for c in &self.children {
            c.walk(f);
        }
    }

    pub fn walk_mut(&mut self, f: &mut impl FnMut(&mut Node)) {
        f(self);
        for c in &mut self.children {
            c.walk_mut(f);
        }
    }

    /// Vrai si ce nœud ou l'un de ses descendants vérifie `f`.
    pub fn any(&self, f: &impl Fn(&Node) -> bool) -> bool {
        f(self) || self.children.iter().any(|c| c.any(f))
    }

    /// Tous les pointeurs contenus dans ce nœud et ses descendants.
    pub fn pointers(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.walk(&mut |n| {
            if let Some(p) = n.pointer() {
                out.push(p.to_string());
            }
        });
        out
    }
}

/// Date et heure courantes (UTC) au format GEDCOM : (`1 NOV 2026`, `19:02:41`).
pub fn now_gedcom() -> (String, String) {
    let secs =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Algorithme « civil_from_days » (H. Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    const M: [&str; 12] = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];
    (
        format!("{day} {} {year}", M[(month - 1) as usize]),
        format!("{:02}:{:02}:{:02}", rem / 3600, rem / 60 % 60, rem % 60),
    )
}

impl Node {
    /// Met à jour la date de dernière modification (`CHAN`), comme le fait Ancestris.
    pub fn touch(&mut self) {
        let (date, time) = now_gedcom();
        let chan = self.ensure_child("CHAN");
        let d = chan.ensure_child("DATE");
        d.value = Some(date);
        d.set_child_val("TIME", &time);
    }
}
