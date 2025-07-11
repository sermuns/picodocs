#import "@preview/cetz:0.4.0": canvas, draw
#import "@preview/cetz-plot:0.1.2": chart

#let background = rgb("#4F5D2F")
#let foreground = rgb("#f3e5c2")
#let accent = rgb("#9aaa65")

#set text(font: "Hanken Grotesk")
#set page(width: auto, height: auto, margin: .5cm, fill: foreground)


#let benchmark = (
  csv("benchmark.csv")
    .slice(1) // remove header
    .sorted(key: row => (row.at(1))) // sort by mean value
    .map(row => (
      row.at(0),
      1000 * float(row.at(1)),
    )) // extract command and mean value, scaled to milliseconds
)

#canvas({
  draw.set-style(barchart: (bar-width: .4, cluster-gap: 1))
  chart.barchart(
    size: (11, auto),
    bar-style: (
      i => {
        (fill: accent.darken(i * 30%))
      }
    ),
    benchmark,
  )
})
