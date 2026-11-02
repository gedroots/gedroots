//! Clé phonétique des noms de famille français, pour retrouver les variantes
//! d'orthographe d'un même nom (Guyot, Guyau, Guillot ; Martin, Martain).
//!
//! Inspirée de Soundex2 et de Phonex : on ramène les graphies d'un même son à une
//! seule lettre, on retire les lettres muettes de fin et les doubles. Deux noms sont
//! des variantes s'ils ont la même clé, ou des clés de 5 lettres et plus qui ne
//! diffèrent que d'une lettre.

use crate::model::sort_key;

/// Clé phonétique d'un nom (lettres minuscules, sans accents).
pub fn key(name: &str) -> String {
    let mut s: String = sort_key(name).chars().filter(|c| c.is_ascii_alphabetic()).collect();
    if s.is_empty() {
        return s;
    }
    // Graphies de plusieurs lettres, dans l'ordre (les plus longues d'abord).
    const RULES: &[(&str, &str)] = &[
        ("sch", "ch"),
        ("sh", "ch"),
        ("ph", "f"),
        ("bv", "v"),
        ("th", "t"),
        ("ck", "k"),
        ("qu", "k"),
        ("gue", "Ge"),
        ("gui", "Gi"),
        ("guy", "Gi"),
        ("eaux", "o"),
        ("eau", "o"),
        ("aux", "o"),
        ("ault", "o"),
        ("aut", "o"),
        ("au", "o"),
        ("ill", "i"),
        ("ail", "ai"),
        ("eil", "ei"),
        ("aim", "in"),
        ("ain", "in"),
        ("ein", "in"),
        ("ai", "e"),
        ("ei", "e"),
        ("er", "e"),
        ("y", "i"),
        ("w", "v"),
        ("q", "k"),
    ];
    for (from, to) in RULES {
        if *from == "er" {
            // Terminaisons seulement (Mercier → mercie, Pottier → potie).
            if s.ends_with(from) && s.len() > 3 {
                s.truncate(s.len() - from.len());
                s.push_str(to);
            }
            continue;
        }
        s = s.replace(from, to);
    }
    // C et G devant e, i : s et j ; sinon k et g. H muet sauf dans « ch ».
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(chars.len());
    for (i, &c) in chars.iter().enumerate() {
        let next = chars.get(i + 1).copied();
        let soft = matches!(next, Some('e' | 'i'));
        match c {
            'c' if next == Some('h') => out.push('c'),
            'c' if soft => out.push('s'),
            'c' => out.push('k'),
            'G' => out.push('g'),
            'g' if soft => out.push('j'),
            'h' if i > 0 && chars[i - 1] == 'c' => out.push('h'),
            'h' => {}
            'x' if i + 1 == chars.len() => {}
            'x' => out.push_str("ks"),
            _ => out.push(c),
        }
    }
    // Nasales (« am, an, em, en », « om, on », « im, in, um, un ») devant une consonne ou en fin :
    // une seule lettre (A, O, I), qui ne compte pas comme une voyelle.
    let vowel = |c: char| "aeiou".contains(c);
    let chars: Vec<char> = out.chars().collect();
    let mut nasal = String::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let n = chars.get(i + 1).copied();
        let after = chars.get(i + 2).copied();
        if matches!(n, Some('m' | 'n')) && after.is_none_or(|a| !vowel(a) && a != 'm' && a != 'n') {
            match c {
                'a' | 'e' => {
                    nasal.push('A');
                    i += 2;
                    continue;
                }
                'o' => {
                    nasal.push('O');
                    i += 2;
                    continue;
                }
                'i' | 'u' => {
                    nasal.push('I');
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        nasal.push(c);
        i += 1;
    }
    // Lettres doublées, puis lettres muettes de fin : e, puis s / t / d / x / p, puis e
    // (Guyotte, Guyots).
    let mut key = dedup(&nasal);
    for set in ["e", "stdxp", "e"] {
        if key.len() > 2 && key.ends_with(|c: char| set.contains(c)) {
            key.pop();
        }
    }
    // Le z final se prononce (Martinez) : remplacé seulement après.
    dedup(&key.replace('z', "s"))
}

fn dedup(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if !out.ends_with(c) {
            out.push(c);
        }
    }
    out
}

/// Les deux noms se prononcent (à peu près) pareil.
pub fn similar(a: &str, b: &str) -> bool {
    let (a, b) = (key(a), key(b));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    a == b || (a.len().min(b.len()) >= 5 && one_edit(&a, &b))
}

/// Les deux mots ne diffèrent que d'une lettre (ajoutée, retirée ou changée).
fn one_edit(a: &str, b: &str) -> bool {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let (short, long) = if a.len() <= b.len() { (&a, &b) } else { (&b, &a) };
    if long.len() - short.len() > 1 {
        return false;
    }
    // Seule une voyelle peut changer, apparaître ou disparaître (Garney / Garnier,
    // Guillaumot / Guillemot), jamais la première lettre ni une consonne.
    let vowel = |c: char| "aeiou".contains(c);
    let prefix = short.iter().zip(long.iter()).take_while(|(x, y)| x == y).count();
    if prefix == 0 {
        return false;
    }
    if short.len() == long.len() {
        vowel(short[prefix]) && vowel(long[prefix]) && short[prefix + 1..] == long[prefix + 1..]
    } else {
        vowel(long[prefix]) && short[prefix..] == long[prefix + 1..]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_of_the_same_name() {
        for (a, b) in [
            ("Guyot", "Guyau"),
            ("Guyot", "Guillot"),
            ("Guyot", "GUYOTTE"),
            ("Martin", "Martain"),
            ("Barricand", "Barriquand"),
            ("Guillaumot", "Guillemot"),
            ("Collet", "Collé"),
            ("Lacourd", "Lacourt"),
            ("Lefebvre", "Lefèvre"),
            ("Philippe", "Filipe"),
            ("Mercier", "Mercié"),
            ("Thibault", "Thibaut"),
            ("Thibault", "Tibaud"),
            ("Rousseau", "Rousseaux"),
            ("Chevalier", "Chevallier"),
            ("Dupont", "Dupond"),
            ("Bernard", "Bernart"),
        ] {
            assert!(similar(a, b), "{a} ~ {b} ({} / {})", key(a), key(b));
        }
    }

    #[test]
    fn different_names_stay_apart() {
        for (a, b) in [
            ("Guyot", "Gaillard"),
            ("Martin", "Morin"),
            ("Petit", "Pernet"),
            ("Roux", "Rey"),
            ("Dubois", "Dupont"),
            ("Martin", "Martinez"),
            ("Simon", "Simand"),
            ("Bourg", "Boury"),
            ("Cordier", "Jordy"),
            ("Chalon", "Schallhauser"),
            ("David", "Davidet"),
        ] {
            assert!(!similar(a, b), "{a} ≁ {b} ({} / {})", key(a), key(b));
        }
    }
}
