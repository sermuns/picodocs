#let background = rgb("#4F5D2F")
#let foreground = rgb("#f3e5c2")
#set page(width: 1073pt, height: 151pt, fill: none, margin: 0em)
#set text(fill: foreground, font: "Libertinus Sans", size: 130pt)
#set align(center + horizon)

#box(
  inset: (bottom: 5pt),
  fill: background,
  width: 100%,
  height: 100%,
  radius: 10%,
  stack(
    dir: ltr,
    spacing: 0.05em,
    text(size: 0.6em, baseline: 5pt)[pico],
    [docs],
  ),
)
