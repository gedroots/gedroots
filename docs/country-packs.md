# Country packs

A country pack says what genealogy records a country kept, from when to when, who may read them,
and the legal age at marriage. GEDRoots uses it to estimate when a marriage took place and to say
where to look for a birth, a marriage, a death or the parents of a person
([agents](agents.md#research-leads-and-log)).

The packs are YAML files, one per country, in `crates/gedcom/countries/`. They are described by a
JSON Schema ([`schema.json`](../crates/gedcom/countries/schema.json)), so any language can read and
validate them. The data is dedicated to the public domain (CC0 1.0) so that other genealogy
software can reuse it. The build embeds every file in the app (`crates/gedcom/build.rs`), which
works offline.

## Files

- `XX.yml`: the pack of a country, named by its ISO 3166-1 alpha-2 code (`FR.yml`).
- `generic.yml`: the rules for countries without a pack, and for periods a pack does not cover (a
  pack with no record kept in the period). Its `names` lists the countries without a pack, so that a
  place in Canada is not read as being in the file's main country.

The country of a place is the last part of the place that matches one of the `names` of a pack.
When a place names none, the file's most frequent country is used.

## Format 1

```yaml
# Sources of the data, in comments.
format: 1
code: FR
name: France
names: [France]
marriage_age:
  - { until: 1792, male: 14, female: 12 }
  - { from: 1804, until: 2006, male: 18, female: 15 }
records:
  - type: civil_registration
    local_name: état civil
    goals: [birth, marriage, death, parents]
    from: 1792
    public_after: { birth: 75, marriage: 75 }
```

| Field | Meaning |
| --- | --- |
| `format` | Version of the format: `1`. |
| `code` | ISO 3166-1 alpha-2 code, or `generic`. Same as the file name. |
| `name` | English name. |
| `names` | Names of the country found in places, in any language, compared without case or accents. A name belongs to one pack only. |
| `marriage_age` | Minimum age at marriage, by period: `from` (included), `until` (excluded), `male`, `female`. The first period containing the year applies. |
| `records` | Kinds of records, best first. |

A record:

| Field | Meaning |
| --- | --- |
| `type` | `civil_registration`, `registration_index` (indexes of the civil registers by period: French decennial tables, GRO indexes), `church_registers`, `census`, `probate`, `marriage_contracts`, `military_registers`, `death_index`, `population_registers`. |
| `local_name` | Name in the country, as archives call it. |
| `goals` | What it helps to find: `birth`, `marriage`, `death`, `parents`. |
| `from`, `until` | First and last years kept, included. Open when absent. |
| `online_until` | Last year that archives usually put online. |
| `index` | An index of other records: suggested only when the estimated period spans three years or more. |
| `public_after` | Years after which a record is open to everyone: a number, or one per goal (`{ birth: 75, marriage: 75 }`; `parents` uses `birth`). Later records are still suggested, with the last public year, because the people concerned and their descendants can usually ask for them. |
| `years` | Years of the record when it was not kept every year (census years). Years still closed are left out. |
| `extends_by` | Years to add after the event (a probate record follows the death). |
| `sex` | Only for people of this sex: `male` or `female`. |
| `born`, `offset` | Only for people born in these years (`[1847, 1921]`); the record's year is the birth year plus `offset` (military registers: the class of the year of birth + 20). |

Unknown fields are refused, so that a typo is caught at build time. A change that readers must
understand differently raises `format`.

## Adding a country

1. Copy a pack of a similar country and rename it with the ISO code.
2. Fill the records from official sources (archives, laws on access delays), cited in comments.
3. Move the country's names from `generic.yml` to the new pack.
4. Run `mise run ci`: the tests read every pack and check codes, periods and names.
