// GEDRoots family book. The data comes from `data.json` (written by `content.rs`); the charts and
// portraits are files beside it. Every displayed text is in the data: nothing to translate here.
#let data = json("data.json")
#let L = data.labels
#let ink = rgb("#1F1B16")
#let muted = rgb("#6B6257")
#let rule = rgb("#D5CAB7")
#let paper2 = rgb("#F4EFE6")
#let violet = rgb("#6D3DF0")
#let serif = "Source Serif 4"
#let sans = "Inter"

#set document(title: data.title)
#set text(font: serif, size: 9.6pt, fill: ink, lang: data.lang, hyphenate: true)
#set par(justify: true, leading: 0.62em, spacing: 0.9em)
// For a printer, each page is larger by the bleed on every side, which the margins absorb.
#let bleed = data.bleed_mm * 1mm
#set page(
  width: data.width_mm * 1mm + 2 * bleed,
  height: data.height_mm * 1mm + 2 * bleed,
  margin: (inside: 18mm + bleed, outside: 16mm + bleed, top: 18mm + bleed, bottom: 20mm + bleed),
  binding: left,
  footer: context {
    let n = counter(page).get().first()
    if n > 2 {
      set text(font: sans, size: 7pt, fill: muted)
      if calc.even(n) [#n #h(1em) #data.title] else [#h(1fr) #data.subtitle #h(1em) #n]
    }
  },
)
#show heading: set text(font: serif, weight: 600)
#show heading.where(level: 1): it => {
  pagebreak(weak: true)
  set par(justify: false)
  v(2mm)
  text(size: 22pt, tracking: -0.01em, it.body)
  v(1mm)
}
#show heading.where(level: 2): it => {
  set par(justify: false)
  block(above: 6mm, below: 1mm, sticky: true, text(size: 15pt, it.body))
}

#let kicker(t) = text(font: sans, size: 7pt, weight: 600, fill: muted, tracking: 0.12em, upper(t))
#let caption(body) = text(font: sans, size: 7pt, fill: muted, body)
#let small(body) = { set par(justify: false); set text(font: sans, size: 7pt, fill: muted); body }
#let anchor(key) = [#metadata(key)#label(key)]
#let page-of(key) = context {
  let found = query(label(key))
  if found.len() > 0 { str(found.first().location().page()) }
}
#let ref-name(p) = if p.key != none [#p.name #text(font: sans, size: 6.6pt, fill: violet)[#L.page~#page-of(p.key)]] else [#p.name]

// --- Cover (or, for a printer, whose cover is a file of its own: the title page) -----------
#if data.print [
  #page(footer: none)[
    #set par(justify: false)
    #v(30mm)
    #kicker(data.subtitle)
    #v(6mm)
    #text(size: 26pt, weight: 600, tracking: -0.02em, data.title)
    #if data.volume != "" [#v(4mm) #text(size: 13pt, style: "italic", fill: muted, data.volume)]
    #v(1fr)
    #small(data.summary)
  ]
] else [
  #page(margin: 0mm, fill: paper2, footer: none)[
    #set par(justify: false)
    #place(top + left, dx: 20mm, dy: 26mm, block(width: data.width_mm * 1mm - 40mm)[
      #kicker(data.subtitle)
      #v(6mm)
      #text(size: 32pt, weight: 600, tracking: -0.02em, data.title)
      #if data.volume != "" [#v(4mm) #text(size: 14pt, style: "italic", fill: muted, data.volume)]
    ])
    #if data.fan {
      place(bottom + center, dy: -30mm, image("fan-cover.svg", width: data.width_mm * 1mm - 32mm))
    }
    #place(bottom + left, dx: 20mm, dy: -14mm, text(font: sans, size: 7pt, fill: muted, data.summary))
  ]
]

// --- Contents -----------------------------------------------------------------------------
#page[
  #v(14mm)
  #kicker(L.contents)
  #v(8mm)
  #set par(justify: false)
  #show outline.entry.where(level: 1): it => block(above: 4.5mm, text(size: 11pt, it))
  #outline(title: none, depth: 1, indent: 0mm)
  #v(1fr)
  #if data.levels { block(width: 80%, small(L.levels)); v(4mm) }
  #line(length: 30mm, stroke: 0.5pt + rule)
  #v(2mm)
  #small(data.colophon)
]

// --- Fan ----------------------------------------------------------------------------------
#if data.fan_page != none [
  = #data.fan_page.title
  #text(size: 10.5pt, style: "italic", fill: muted, data.fan_page.subtitle)
  #v(4mm)
  #image("fan.svg", width: 100%)
  #v(4mm)
  #grid(columns: (1fr, 1fr, 1fr), row-gutter: 2.5mm, column-gutter: 3mm,
    ..data.legend.map(l => box[
      #box(width: 3mm, height: 3mm, radius: 0.6mm, fill: rgb(l.color), stroke: if l.unknown { (paint: rule, dash: "dashed") } else { none }, baseline: 0.4mm)
      #h(1.5mm) #text(font: sans, size: 7.5pt, l.label) #h(1mm) #text(font: sans, size: 7.5pt, fill: muted, str(l.count))
    ])
  )
]

// --- Map ----------------------------------------------------------------------------------
#if data.map != none [
  = #data.map.title
  #text(size: 10.5pt, style: "italic", fill: muted, L.places_sub)
  #v(4mm)
  #image("map.svg", width: 100%)
  #v(2mm)
  #caption(data.map.caption)
]

// --- Lifespans ----------------------------------------------------------------------------
#if data.lifespans [
  = #L.lives
  #text(size: 10.5pt, style: "italic", fill: muted, L.lives_sub)
  #v(4mm)
  #image("lifespans.svg", width: 100%)
]

// --- What an agent wrote: text, context (with sources), hypotheses (with their clues) ------
#let ochre = rgb("#C9A35B")
#let written(blocks) = for b in blocks {
  if b.kind == "text" {
    for p in b.paragraphs [#par(p)]
  } else if b.kind == "context" {
    block(width: 100%, fill: paper2, inset: (x: 4mm, y: 3mm), stroke: (left: 2pt + ochre), above: 4mm, below: 4mm, breakable: true)[
      #kicker(L.context)
      #v(0.5mm)
      #text(weight: 600, b.title)
      #v(0.5mm)
      #for p in b.paragraphs [#par(p)]
      #small[#b.notes.join("\n")]
    ]
  } else {
    block(width: 100%, inset: (left: 4mm, y: 1mm), stroke: (left: (paint: violet, thickness: 1pt, dash: "dashed")), above: 4mm, below: 4mm, breakable: true)[
      #text(font: sans, size: 7pt, weight: 600, fill: violet, tracking: 0.12em, upper(L.hypothesis))
      #v(0.5mm)
      #set text(style: "italic")
      #for p in b.paragraphs [#par(p)]
      #small[#L.basis #b.notes.join(" · ")]
    ]
  }
}

// --- Records, one chapter per generation -------------------------------------------------
#let strip(points) = {
  let y0 = points.first().year
  let y1 = points.last().year
  let span = calc.max(y1 - y0, 1)
  box(width: 100%, height: 12mm)[
    #let x(y) = (y - y0) / span * 100%
    #place(top + left, dy: 4mm, line(length: 100%, stroke: 2.2pt + rgb("#C9A35B")))
    #for (k, p) in points.enumerate() {
      place(top + left, dx: x(p.year) - 1.2mm, dy: 2.8mm, circle(radius: 1.2mm, fill: white, stroke: 1pt + ink))
      let close = k > 0 and (p.year - points.at(k - 1).year) / span < 0.12
      place(top + left, dx: x(p.year) - 10mm, dy: if close and calc.odd(k) { -5.5mm } else { 7mm },
        box(width: 20mm, align(center, text(font: sans, size: 6.2pt)[*#p.year* \ #text(fill: muted, p.label)])))
    }
  ]
}

#let family(lines) = {
  set par(justify: false, leading: 0.5em)
  set text(font: sans, size: 7pt)
  for l in lines {
    block(below: 2mm)[
      #text(weight: 600, l.label) \
      #for p in l.people [#ref-name(p) \ ]
    ]
  }
}

#let full-sheet(s) = block(breakable: true, above: 8mm)[
  // The number, the name and the strip never stay alone at the bottom of a page.
  #block(breakable: false, sticky: true, below: 3mm)[
    #anchor(s.key)
    #kicker(s.number)
    #v(1mm)
    == #s.name
    #text(size: 10pt, style: "italic", fill: muted)[#s.years#if s.subtitle != "" [ · #s.subtitle]]
    #if s.strip.len() > 0 { v(4mm); strip(s.strip) }
  ]
  #grid(columns: (1fr, 34mm), column-gutter: 5mm,
    [
      #for p in s.paragraphs [#par(p)]
      #written(s.blocks)
    ],
    [
      #if s.portrait != none { image(s.portrait, width: 100%); v(3mm) }
      #family(s.family)
      #if s.sources.len() > 0 {
        small[#text(weight: 600, fill: ink, L.sources) \ #s.sources.join("\n")]
      }
    ],
  )
]

#let short-sheet(s) = block(breakable: false, above: 5mm)[
  #anchor(s.key)
  #text(weight: 600, size: 10.5pt, s.name) #h(1mm) #text(font: sans, size: 7pt, fill: muted)[#s.number · #s.years]
  #v(-1mm)
  #s.paragraphs.join(" ")
  #written(s.blocks)
]

#for (i, ch) in data.chapters.enumerate() [
  = #ch.title
  #text(size: 10.5pt, style: "italic", fill: muted, ch.subtitle)
  #if ch.intro.len() > 0 { v(4mm); written(ch.intro) }
  #for s in ch.full { full-sheet(s) }
  #for s in ch.short { short-sheet(s) }
  #if ch.rows.len() > 0 {
    v(6mm)
    kicker(L.also_known)
    v(2mm)
    set text(font: sans, size: 7pt)
    set par(justify: false)
    table(
      columns: (auto, 1fr, 1fr, 1fr),
      stroke: (x, y) => (bottom: 0.4pt + rule),
      inset: (x: 1.5mm, y: 1.6mm),
      table.header(text(fill: muted)[\#], text(fill: muted)[ ], text(fill: muted, L.birth), text(fill: muted, L.death)),
      ..ch.rows.map(r => (text(fill: muted, r.number), [#anchor(r.key)#r.name], r.birth, r.death)).flatten()
    )
  }
]

// --- Figures ------------------------------------------------------------------------------
#if data.completeness or data.longevity != none or data.occupations.len() > 0 [
  = #L.figures
  #if data.completeness [
    #v(2mm)
    #kicker(L.found)
    #v(2mm)
    #image("completeness.svg", width: 100%)
  ]
  #if data.longevity != none [
    #v(5mm)
    #kicker(L.longevity)
    #v(1mm)
    #image("longevity.svg", width: 100%)
    #caption(data.longevity)
  ]
  #if data.occupations.len() > 0 [
    #v(5mm)
    #kicker(L.occupations)
    #v(2mm)
    #let mx = data.occupations.first().at(1)
    #grid(columns: (30mm, 1fr), row-gutter: 1.6mm, align: horizon,
      ..data.occupations.map(((o, n)) => (text(font: sans, size: 7.5pt, o), box(width: 100%)[#box(width: n / mx * 85%, height: 3mm, fill: violet.lighten(25%)) #h(1.5mm) #text(font: sans, size: 7pt, fill: muted, str(n))])).flatten()
    )
  ]
]

// --- Index --------------------------------------------------------------------------------
= #L.index
#text(font: sans, size: 6.6pt, fill: muted, L.index_note)
#v(4mm)
#set par(justify: false, leading: 0.42em, spacing: 0.42em)
#columns(3, gutter: 5mm)[
  #set text(font: sans, size: 6.4pt)
  #for group in data.index {
    block(breakable: false, below: 2.2mm)[
      #text(weight: 600, size: 7pt, group.surname) \
      #for e in group.people [#e.given#if e.years != "" [, #e.years] #h(1fr) #text(fill: muted, page-of(e.key)) \ ]
    ]
  }
]
