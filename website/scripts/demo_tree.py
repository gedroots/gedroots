#!/usr/bin/env python3
"""Build the fictional family tree used for the documentation screenshots.

Every person, place, record and archive reference here is invented. Usage:

    python3 website/scripts/demo_tree.py OUT_DIR

writes OUT_DIR/demo.ged and the record images it points to (OUT_DIR/Records/...).
"""
import os
import random
import sys

random.seed(7)

PLACES = {
    # name: (postcode, county, lat, long) — long negative = west
    "Saint-Lô": ("50000", "Manche", 49.1157, -1.0906),
    "Agneaux": ("50180", "Manche", 49.1181, -1.1070),
    "Coutances": ("50200", "Manche", 49.0477, -1.4452),
    "Carentan": ("50500", "Manche", 49.3036, -1.2480),
    "Villedieu-les-Poêles": ("50800", "Manche", 48.8378, -1.2192),
    "Bayeux": ("14400", "Calvados", 49.2764, -0.7024),
    "Caen": ("14000", "Calvados", 49.1829, -0.3707),
    "Vire": ("14500", "Calvados", 48.8386, -0.8892),
    "Granville": ("50400", "Manche", None, None),  # left without coordinates on purpose
}


def plac(town, level):
    code, county, lat, lon = PLACES[town]
    out = [f"{level} PLAC , {town}, {code}, {county}, Normandie, France"]
    if lat is not None:
        out += [f"{level + 1} MAP", f"{level + 2} LATI N{lat:.4f}",
                f"{level + 2} LONG {'W' if lon < 0 else 'E'}{abs(lon):.4f}"]
    return out


MONTHS = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"]


def date(year, exact=True):
    if not exact:
        return f"ABT {year}"
    return f"{random.randint(1, 28)} {random.choice(MONTHS)} {year}"


people = {}   # id -> dict
families = []  # dicts
sources = {}  # key -> (id, title, caln)
media = []    # (id, file, title)


def person(pid, given, surname, sex, birth, birth_place, death=None, death_place=None,
           occupation=None, exact=True, sosa=None):
    people[pid] = dict(given=given, surname=surname, sex=sex, birth=birth, birth_place=birth_place,
                       death=death, death_place=death_place, occupation=occupation, exact=exact,
                       sosa=sosa, famc=None, fams=[], cites={}, objes=[])
    return pid


def family(fid, husb, wife, marr_year=None, marr_place=None, children=()):
    families.append(dict(id=fid, husb=husb, wife=wife, marr=marr_year, place=marr_place,
                         children=list(children), cites=None))
    for p in (husb, wife):
        if p:
            people[p]["fams"].append(fid)
    for c in children:
        people[c]["famc"] = fid


def source(key, title, caln):
    sid = f"S{len(sources) + 1}"
    sources[key] = (sid, title, caln)
    return sid


# Generation 1 (root) — Sosa 1
person("I1", "Paul Émile", "MARCHAND", "M", 1912, "Saint-Lô", 1987, "Caen", "Schoolteacher", sosa=1)
# Spouse and children of the root (all deceased, born before 1925)
person("I30", "Madeleine", "LECOQ", "F", 1914, "Bayeux", 1995, "Caen")
person("I31", "Jacques", "MARCHAND", "M", 1937, "Caen", 2008, "Caen", "Engineer")
person("I32", "Odile", "MARCHAND", "F", 1940, "Caen", 2019, "Vire")
# Siblings of the root
person("I33", "Henri", "MARCHAND", "M", 1910, "Saint-Lô", 1944, "Saint-Lô", "Baker")
person("I34", "Suzanne", "MARCHAND", "F", 1915, "Saint-Lô", 1990, "Granville")
# Generation 2
person("I2", "Émile", "MARCHAND", "M", 1884, "Agneaux", 1944, "Saint-Lô", "Baker", sosa=2)
person("I3", "Jeanne", "ROUSSEL", "F", 1886, "Coutances", 1961, "Saint-Lô", "Seamstress", sosa=3)
# Generation 3
person("I4", "Louis", "MARCHAND", "M", 1851, "Agneaux", 1920, "Agneaux", "Miller", sosa=4)
person("I5", "Marie", "HUET", "F", 1856, "Carentan", 1931, "Agneaux", sosa=5)
person("I6", "Auguste", "ROUSSEL", "M", 1858, "Coutances", 1915, "Coutances", "Carpenter", sosa=6)
person("I7", "Louise", "DUVAL", "F", 1862, "Villedieu-les-Poêles", 1940, "Coutances", sosa=7)
# Generation 4
person("I8", "Pierre", "MARCHAND", "M", 1820, "Agneaux", 1878, "Agneaux", "Miller", sosa=8)
person("I9", "Rose", "LEFÈVRE", "F", 1824, "Saint-Lô", 1890, "Agneaux", sosa=9)
person("I10", "François", "HUET", "M", 1825, "Carentan", 1880, "Carentan", "Fisherman", exact=False, sosa=10)
person("I11", "Victoire", "LEMONNIER", "F", 1829, "Carentan", 1899, "Carentan", sosa=11)
person("I12", "Jean-Baptiste", "ROUSSEL", "M", 1830, "Coutances", 1890, "Coutances", "Carpenter", sosa=12)
person("I13", "Adèle", "GOSSELIN", "F", 1833, "Coutances", None, None, sosa=13)
person("I14", "Victor", "DUVAL", "M", 1835, "Villedieu-les-Poêles", 1901, "Villedieu-les-Poêles",
       "Coppersmith", sosa=14)
person("I15", "Eugénie", "LEBRUN", "F", 1838, "Vire", 1910, "Villedieu-les-Poêles", sosa=15)
# Generation 5 (partial: research still to do)
person("I16", "Jacques", "MARCHAND", "M", 1790, "Agneaux", 1851, "Agneaux", "Miller", exact=False, sosa=16)
person("I17", "Catherine", "PIGNOT", "F", 1795, "Agneaux", 1860, "Agneaux", exact=False, sosa=17)
# Siblings in generation 3
person("I35", "Augustine", "MARCHAND", "F", 1854, "Agneaux", 1930, "Saint-Lô")
person("I36", "Jules", "MARCHAND", "M", 1857, "Agneaux", 1916, "Saint-Lô", "Soldier")

family("F1", "I2", "I3", 1909, "Saint-Lô", ["I33", "I1", "I34"])
family("F2", "I4", "I5", 1880, "Carentan", ["I2"])
family("F3", "I6", "I7", 1883, "Coutances", ["I3"])
family("F4", "I8", "I9", 1848, "Saint-Lô", ["I4", "I35", "I36"])
family("F5", "I10", "I11", 1852, "Carentan", ["I5"])
family("F6", "I12", "I13", 1855, "Coutances", ["I6"])
family("F7", "I14", "I15", 1860, "Villedieu-les-Poêles", ["I7"])
family("F8", "I16", "I17", 1818, "Agneaux", ["I8"])
family("F9", "I1", "I30", 1936, "Bayeux", ["I31", "I32"])

# Sources: one register per town and record type, like most civil registers.
def register(town, kind, start):
    return source((town, kind, start), f"{town}, {kind} {start}–{start + 9}",
                  f"5 Mi {random.randint(100, 999)}")


cite_plan = [
    ("I1", "BIRT"), ("I2", "BIRT"), ("I2", "DEAT"), ("I3", "BIRT"), ("I4", "BIRT"), ("I4", "DEAT"),
    ("I5", "BIRT"), ("I6", "BIRT"), ("I8", "BIRT"), ("I9", "BIRT"), ("I12", "BIRT"), ("I14", "BIRT"),
    ("I33", "BIRT"), ("I34", "BIRT"), ("I35", "BIRT"), ("I36", "BIRT"),
]
KIND = {"BIRT": "births", "DEAT": "deaths", "MARR": "marriages"}
for pid, tag in cite_plan:
    p = people[pid]
    year = p["birth"] if tag == "BIRT" else p["death"]
    town = p["birth_place"] if tag == "BIRT" else p["death_place"]
    start = year - year % 10
    key = (town, KIND[tag].capitalize(), start)
    if key not in sources:
        register(town, KIND[tag].capitalize(), start)
    p["cites"][tag] = (sources[key][0], f"Record no. {random.randint(10, 180)}, image {random.randint(2, 60)}")

for fam in families:
    if fam["marr"] and fam["id"] in ("F1", "F2", "F4"):
        start = fam["marr"] - fam["marr"] % 10
        key = (fam["place"], "Marriages", start)
        if key not in sources:
            register(fam["place"], "Marriages", start)
        fam["cites"] = (sources[key][0], f"Record no. {random.randint(5, 60)}")

# Record images for a few acts.
RECORD_IMAGES = [("I1", "BIRT", "Birth record of Paul MARCHAND"),
                 ("I2", "BIRT", "Birth record of Émile MARCHAND"),
                 ("I4", "DEAT", "Death record of Louis MARCHAND")]


def write_images(out_dir):
    try:
        from PIL import Image, ImageDraw, ImageFont
    except ImportError:
        return []
    os.makedirs(os.path.join(out_dir, "Records"), exist_ok=True)
    made = []
    font_path = "/usr/share/fonts/truetype/dejavu/DejaVuSerif-Italic.ttf"
    for n, (pid, tag, title) in enumerate(RECORD_IMAGES, 1):
        p = people[pid]
        img = Image.new("RGB", (1200, 1600), (232, 220, 192))
        d = ImageDraw.Draw(img)
        try:
            big = ImageFont.truetype(font_path, 44)
            small = ImageFont.truetype(font_path, 30)
        except OSError:
            big = small = ImageFont.load_default()
        d.text((90, 90), f"{'Naissance' if tag == 'BIRT' else 'Décès'} — {p['given']} {p['surname']}",
               fill=(60, 45, 30), font=big)
        for i in range(34):
            words = " ".join(random.choice(["l'an", "mil", "huit", "cent", "devant", "nous", "maire",
                                            "officier", "de", "l'état", "civil", "est", "comparu",
                                            "lequel", "nous", "a", "déclaré", "que", "témoins", "domicilié"])
                             for _ in range(9))
            d.text((90, 190 + i * 40), words, fill=(80, 62, 44), font=small)
        name = f"Records/{n:02d}_{tag.lower()}_{p['surname'].lower()}_{p['given'].split()[0].lower()}.jpg"
        img.save(os.path.join(out_dir, name), quality=82)
        mid = f"M{n}"
        media.append((mid, name, title))
        p["objes"].append((tag, mid))
        made.append(name)
    return made


def ged():
    L = ["0 HEAD", "1 GEDC", "2 VERS 7.0.13", "1 SOUR GEDROOTS", "2 NAME GEDRoots", "1 LANG en",
         "1 PLAC", "2 FORM Locality, Town, Postcode, County, Region, Country",
         "1 NOTE Fictional family used in the GEDRoots documentation"]
    # File settings and a saved search, stored as GEDRoots extensions (docs/extensions.md).
    ext = "https://github.com/gedroots/gedroots/blob/main/docs/extensions.md"
    head = ["1 SCHMA"] + [f"2 TAG {t} {ext}#{a}" for t, a in (
        ("_GR_MEDIA", "_gr_media"), ("_GR_DIR", "_gr_media"), ("_GR_TYPES", "_gr_media"),
        ("_GR_NAME", "_gr_media"), ("_GR_FIND", "_gr_find"), ("_GR_SCOPE", "_gr_find"),
        ("_GR_QUERY", "_gr_find"))]
    head += ["1 _GR_MEDIA",
             "2 _GR_DIR Records", "3 _GR_TYPES BIRT, BAPM, MARR, DEAT, BURI",
             "3 _GR_NAME {sosa:3}_{type:1}_{surname:simple}_{given}",
             "2 _GR_DIR Portraits", "3 _GR_TYPES INDI", "3 _GR_NAME {surname:simple}_{given}",
             "2 _GR_DIR Miscellaneous",
             "1 _GR_FIND Ancestors without sources", "2 _GR_SCOPE INDI",
             "2 _GR_QUERY ancestors:@I1@ without:source"]
    L[3:3] = head
    for pid, p in people.items():
        L += [f"0 @{pid}@ INDI", f"1 NAME {p['given']} /{p['surname']}/", f"2 GIVN {p['given']}",
              f"2 SURN {p['surname']}", f"1 SEX {p['sex']}"]
        for tag, year, town in (("BIRT", p["birth"], p["birth_place"]), ("DEAT", p["death"], p["death_place"])):
            if not year:
                continue
            L += [f"1 {tag}", f"2 DATE {date(year, p['exact'])}"] + plac(town, 2)
            if tag in p["cites"]:
                sid, page = p["cites"][tag]
                L += [f"2 SOUR @{sid}@", f"3 PAGE {page}"]
                for t, mid in p["objes"]:
                    if t == tag:
                        L += [f"3 OBJE @{mid}@"]
            for t, mid in p["objes"]:
                if t == tag:
                    L += [f"2 OBJE @{mid}@"]
        if pid == "I36":
            L += ["1 BURI", "2 DATE 2 MAR 1915", "2 PLAC , St-Lô, 50000, Manche, Normandie, France"]
        if p["occupation"]:
            L += [f"1 OCCU {p['occupation']}"]
        if p["famc"]:
            L += [f"1 FAMC @{p['famc']}@"]
        for f in p["fams"]:
            L += [f"1 FAMS @{f}@"]
    for fam in families:
        L += [f"0 @{fam['id']}@ FAM"]
        if fam["husb"]:
            L += [f"1 HUSB @{fam['husb']}@"]
        if fam["wife"]:
            L += [f"1 WIFE @{fam['wife']}@"]
        for c in fam["children"]:
            L += [f"1 CHIL @{c}@"]
        if fam["marr"]:
            L += ["1 MARR", f"2 DATE {date(fam['marr'])}"] + plac(fam["place"], 2)
            if fam["cites"]:
                L += [f"2 SOUR @{fam['cites'][0]}@", f"3 PAGE {fam['cites'][1]}"]
    for sid, title, caln in sources.values():
        L += [f"0 @{sid}@ SOUR", f"1 TITL {title}", "1 REPO @R1@", f"2 CALN {caln}"]
    # A few deliberate issues, so the Data Quality page has something to show.
    L += ["0 @S99@ SOUR", "1 TITL Agneaux, Parish register 1780–1792", "1 REPO @R1@", "2 CALN 5 Mi 12"]
    L += ["0 @R1@ REPO", "1 NAME Departmental Archives of Normandy (fictional)"]
    L += ["0 @M9@ OBJE", "1 FILE Records/census_1906_saint-lo.jpg", "2 FORM image/jpeg",
          "2 TITL Census 1906, Saint-Lô"]
    for mid, path, title in media:
        L += [f"0 @{mid}@ OBJE", f"1 FILE {path}", "2 FORM image/jpeg", f"3 MEDI MANUSCRIPT",
              f"2 TITL {title}"]
    L += ["0 TRLR"]
    return "\n".join(L) + "\n"


def main():
    out_dir = sys.argv[1] if len(sys.argv) > 1 else "."
    os.makedirs(out_dir, exist_ok=True)
    write_images(out_dir)
    with open(os.path.join(out_dir, "demo.ged"), "w", encoding="utf-8") as f:
        f.write(ged())
    print(f"{len(people)} people, {len(families)} families, {len(sources)} sources, {len(media)} media")


if __name__ == "__main__":
    main()
