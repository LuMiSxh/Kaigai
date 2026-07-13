# Kaigai documentation

The root [README](../README.md) is the place to start if you only want to
install and use Kaigai. Everything here is for people who want to understand,
measure or change the project.

## Pick the document that matches the job

| If you want to…                                      | Start here                                            |
| ---------------------------------------------------- | ----------------------------------------------------- |
| Run Kaigai locally or make a code change             | [Development guide](development.md)                   |
| Prepare the VTuber corpus and run model measurements | [Benchmark workflow](benchmarks/README.md)            |
| See why Medium + Core ML is the current default      | [Benchmark findings](benchmarks/findings.md)          |
| Understand the proposed two-pass Accuracy mode       | [Accuracy-mode design](architecture/accuracy-mode.md) |
| Decide whether a release is ready                    | [Release checklist](maintainers/release-checklist.md) |
| See what changed between versions                    | [Changelog](../CHANGELOG.md)                          |

## Layout

```text
docs/
├── README.md
├── development.md
├── architecture/
│   └── accuracy-mode.md
├── benchmarks/
│   ├── README.md
│   └── findings.md
└── maintainers/
    └── release-checklist.md
```

The split is intentional:

- `development.md` describes the repository as it exists today.
- `benchmarks/` keeps repeatable measurement instructions next to the
  conclusions drawn from them.
- `architecture/` is for designs that are not necessarily implemented yet.
- `maintainers/` contains release process rather than product documentation.

If a document describes planned work, it should say so near the top. That keeps
future designs from being mistaken for features already available in the app.
