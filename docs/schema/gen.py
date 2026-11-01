import json, sys
src, out = sys.argv[1], sys.argv[2]
d = json.load(open(src))
V7 = "https://gedcom.io/terms/v7/"
def short(u): return u[len(V7):] if u.startswith(V7) else None

# collect v7 structures
ids = set()
for sup, subs in d["substructure"].items():
    if sup == "" or short(sup):
        if sup: ids.add(short(sup))
        for tag, s in subs.items():
            if short(s["type"]): ids.add(short(s["type"]))
ids = sorted(ids)
index = {s: i + 1 for i, s in enumerate(ids)}  # 0 = root
CARD = {"{0:1}": "Card::Optional", "{1:1}": "Card::Required", "{0:M}": "Card::Many", "{1:M}": "Card::AtLeastOne"}

def payload(sid):
    p = d["payload"].get(V7 + sid) if sid else None
    if p is None: return "Payload::None"
    t = p.get("type")
    if t is None: return "Payload::None"
    if t == "pointer":
        return 'Payload::Pointer("%s")' % short(p["to"]).replace("record-", "")
    if t == "Y|<NULL>": return "Payload::YOrEmpty"
    if t in (V7 + "type-Enum", V7 + "type-List#Enum"):
        vals = sorted(d["set"][p["set"]].keys())
        lst = "true" if "List" in t else "false"
        return "Payload::Enum { list: %s, values: &[%s] }" % (lst, ", ".join('"%s"' % v for v in vals))
    if t.startswith(V7 + "type-Date"): return "Payload::Date"
    if t == V7 + "type-Age": return "Payload::Age"
    if t == V7 + "type-Time": return "Payload::Time"
    if t == "http://www.w3.org/ns/dcat#mediaType": return "Payload::MediaType"
    if t == "http://www.w3.org/2001/XMLSchema#Language": return "Payload::Language"
    return "Payload::Text"

def label(sid):
    l = d["label"].get(V7 + sid, {}).get("en-US", "")
    return l.replace('"', '\\"')

lines = ["// @generated par docs/schema/gen.py à partir du registre GEDCOM 7 de FamilySearch",
         "// (GEDCOM-registries, generated_files/g7validation.json). Ne pas modifier à la main.",
         "", "use super::{Card, Payload, StructDef, Sub};", "",
         "pub(super) static STRUCTS: &[StructDef] = &["]
def emit(sid, subs):
    items = []
    for tag in sorted(subs):
        s = subs[tag]
        ts = short(s["type"])
        if ts is None: continue
        items.append('Sub { tag: "%s", def: %d, card: %s }' % (tag, index[ts], CARD[s["cardinality"]]))
    lines.append('    StructDef { id: "%s", label: "%s", payload: %s, subs: &[%s] },' % (
        sid, label(sid) if sid else "", payload(sid), ", ".join(items)))
emit("", d["substructure"][""])
for sid in ids:
    emit(sid, d["substructure"].get(V7 + sid, {}))
lines.append("];")
open(out, "w").write("\n".join(lines) + "\n")
print(len(ids) + 1, "structures")
