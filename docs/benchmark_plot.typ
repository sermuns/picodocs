#import "@preview/cetz:0.4.0": canvas, draw
#import "@preview/cetz-plot:0.1.2": chart

#let background = rgb("#4F5D2F")
#let foreground = rgb("#f3e5c2")
#let accent = rgb("#9aaa65")

#set text(font: "JetBrains Mono", weight: 500)
#set page(width: auto, height: auto, margin: .5cm, fill: foreground)

#let benchmark = (
  csv("benchmark.csv")
    .slice(1) // remove header
    .sorted(key: row => (row.at(1))) // sort by mean value
    .map(row => {
      let millis = 1000 * float(row.at(1))
      (
        [#row.at(0).split().at(0) *(#calc.round(millis, digits: 1) ms)*],
        millis,
      )
    })
)

#align(center)[Milliseconds to build _this_ site]

#v(1em)

#canvas({
  draw.set-style(barchart: (bar-width: .5))
  chart.barchart(
    size: (7, auto),
    bar-style: i => {
      (fill: accent.darken(i * 30%))
    },
    x-tick-step: 50,
    x-format: it => [#it],
    x-grid: false,
    benchmark,
  )
})
