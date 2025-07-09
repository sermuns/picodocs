#let background = rgb("#4F5D2F")
#let foreground = rgb("#f3e5c2")
#set page(width: 1073pt, height: 151pt, fill: none, margin: 0em)
#set text(fill: foreground, font: "Adwaita Mono", size: 110pt)
#set align(center + top)

#box(
  inset: (top: 0.25em),
  fill: background,
  width: 100%,
  height: 100%,
  radius: 10%,
  stack(
    dir: ltr,
    spacing: 0.05em,
    pad(right: .2em, image("ant.svg", height: .9em)),
    text(size: 0.6em, baseline: .2em)[pico],
    [docs],
  ),
)
