// GEDRoots family book: the cover for a printer, on one spread (back cover, spine, front cover)
// with the bleed around it. The data comes from `cover.json` (written by `lib.rs`).
#let data = json("cover.json")
#let ink = rgb("#1F1B16")
#let muted = rgb("#6B6257")
#let paper2 = rgb("#F4EFE6")
#let serif = "Source Serif 4"
#let sans = "Inter"
#let bleed = data.bleed_mm * 1mm
#let w = data.width_mm * 1mm
#let h = data.height_mm * 1mm
#let spine = data.spine_mm * 1mm

#set document(title: data.title)
#set text(font: serif, fill: ink, lang: data.lang)
#set par(justify: false)
#set page(width: 2 * w + spine + 2 * bleed, height: h + 2 * bleed, margin: 0mm, fill: paper2)

// Back cover: what the book holds.
#place(top + left, dx: bleed + 20mm, dy: bleed + 30mm, block(width: w - 40mm)[
  #text(size: 11pt, style: "italic", data.subtitle)
  #v(4mm)
  #text(font: sans, size: 8pt, fill: muted, data.summary)
])
#place(bottom + left, dx: bleed + 20mm, dy: -(bleed + 16mm), block(width: w - 40mm,
  text(font: sans, size: 7pt, fill: muted, data.colophon)))

// Spine: the title, read from top to bottom, when the spine is wide enough for it.
#if data.spine_text {
  place(top + left, dx: bleed + w, dy: bleed, box(width: spine, height: h,
    align(center + horizon, rotate(90deg, reflow: true,
      text(size: calc.min(10pt, spine * 0.45), weight: 600)[#data.title#if data.volume != "" [ · #data.volume]]))))
}

// Front cover, as in the book.
#let fx = bleed + w + spine
#place(top + left, dx: fx + 20mm, dy: bleed + 26mm, block(width: w - 40mm)[
  #text(font: sans, size: 7pt, weight: 600, fill: muted, tracking: 0.12em, upper(data.subtitle))
  #v(6mm)
  #text(size: 32pt, weight: 600, tracking: -0.02em, data.title)
  #if data.volume != "" [#v(4mm) #text(size: 14pt, style: "italic", fill: muted, data.volume)]
])
#if data.fan {
  place(top + left, dx: fx + 16mm, dy: bleed + h - 30mm - (w - 32mm) * 0.56, image("fan-cover.svg", width: w - 32mm))
}
