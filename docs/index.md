<center>
<img src="banner.png" height="200">
<em>An extremely tiny and fast alternative to MkDocs</em>
</center>

Welcome to picodocs' documentation! picodocs is a static site generator for documentation, designed to be a lightweight and fast alternative to MkDocs. It is written in Rust and aims to provide a simple, efficient way to create documentation sites without the overhead of larger frameworks.

## Features

- Is installed as a single (tiny) binary. No dependencies!
- Generates static HTML that is _tiny_ and [can be free from JavaScript](about/no-javascript.md).
- Supports [Markdown extensions](about/markdown-extensions.md) like footnotes, admonitions, and more.

```
hyperfine -w10 'picodocs build' 'mdbook build -d /tmp/mdbook' 'mkdocs build -d /tmp/mkdocs' --export-csv docs/benchmark.csv
```

![Plot of benchmark comparison](benchmark_plot.svg)
