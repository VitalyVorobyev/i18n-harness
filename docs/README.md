# docs/

Documentation for people who work **on** i18n-harness. If you want to use
it, read the [user guide](https://vitalyvorobyev.github.io/i18n-harness/)
(its sources are in [`../book/`](../book/)).

- [`contributing/`](contributing/): how to build, test, extend and
  release. Start with [`../CONTRIBUTING.md`](../CONTRIBUTING.md).
  - [Architecture](contributing/architecture.md)
  - [Building and testing](contributing/building.md)
  - [Extending the harness](contributing/extending.md)
  - [Prompt profiles](contributing/prompt-profiles.md)
  - [The round-trip contract](contributing/round-trip.md)
  - [Releasing](contributing/releasing.md)
- [`internal/`](internal/): maintainer design notes, roadmaps and
  specifications. They record decisions as they were made and may lag
  behind the code.
- [`benchmarks/`](benchmarks/): model comparisons on real catalogs, made
  with `harness-lab`.
