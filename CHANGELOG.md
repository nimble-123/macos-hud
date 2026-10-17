# Changelog

## [0.5.0](https://github.com/nimble-123/macos-hud/compare/v0.4.0...v0.5.0) (2026-10-17)


### Neue Funktionen

* **domain:** add usage model and price table ([#82](https://github.com/nimble-123/macos-hud/issues/82)) ([d92f723](https://github.com/nimble-123/macos-hud/commit/d92f723fdd55fecba5d8324c12040375fef9c771))
* **tui:** add AI tab with usage and cost per period ([#85](https://github.com/nimble-123/macos-hud/issues/85)) ([a6556d2](https://github.com/nimble-123/macos-hud/commit/a6556d21ac05f2fe40fe6bbaf618f9b261698511))
* **tui:** show telemetry values in the AI tab ([#88](https://github.com/nimble-123/macos-hud/issues/88)) ([3012936](https://github.com/nimble-123/macos-hud/commit/30129369ec9f0138d5100bf4aaeb65ffe259bbc9))
* **usage:** read Claude Code logs and summarise usage per period ([#84](https://github.com/nimble-123/macos-hud/issues/84)) ([cbbd8eb](https://github.com/nimble-123/macos-hud/commit/cbbd8eb560aec40ec7f1beaabb43e73dc3e5e608))
* **usage:** receive Claude Code telemetry on localhost ([#86](https://github.com/nimble-123/macos-hud/issues/86)) ([4fbfbf9](https://github.com/nimble-123/macos-hud/commit/4fbfbf9bfabd794b7ca67e7566f9d9549d8fd0c1))
* **usage:** switch on Claude Code telemetry from the AI tab ([#87](https://github.com/nimble-123/macos-hud/issues/87)) ([578e4f2](https://github.com/nimble-123/macos-hud/commit/578e4f26a5ea46caab55d6c55687ca12104aeb49))

## [0.4.0](https://github.com/nimble-123/macos-hud/compare/v0.3.0...v0.4.0) (2026-10-17)


### Neue Funktionen

* ask before quitting while installations run ([#77](https://github.com/nimble-123/macos-hud/issues/77)) ([9034f08](https://github.com/nimble-123/macos-hud/commit/9034f08ba15285e3c5308aa0b175a688e4bd8935))
* **cli:** add flags to reset configuration and data ([#71](https://github.com/nimble-123/macos-hud/issues/71)) ([9b4fd6b](https://github.com/nimble-123/macos-hud/commit/9b4fd6bcf9657ddc8508e0f005419ada3ef82b44))
* **tui:** add log tab with search and filter ([#75](https://github.com/nimble-123/macos-hud/issues/75)) ([d299b89](https://github.com/nimble-123/macos-hud/commit/d299b8984cc43d48704d315ec874e2ce169346c3))
* **tui:** add settings tab that writes the config in place ([#72](https://github.com/nimble-123/macos-hud/issues/72)) ([00591fc](https://github.com/nimble-123/macos-hud/commit/00591fccd74e7d33d99c6502ae653d321c7bf4a6))
* **tui:** wrap list selection at both ends ([#76](https://github.com/nimble-123/macos-hud/issues/76)) ([c69f29d](https://github.com/nimble-123/macos-hud/commit/c69f29d292cbfa1eda5082a3e9f9d5ea5fc31e29))
* **updates:** install opted-in updates automatically ([#68](https://github.com/nimble-123/macos-hud/issues/68)) ([1317131](https://github.com/nimble-123/macos-hud/commit/131713137815f1faec74fe6d2e8fa5d5796e6b0b))
* **updates:** log the end of the output of successful installs ([#78](https://github.com/nimble-123/macos-hud/issues/78)) ([1e3e41d](https://github.com/nimble-123/macos-hud/commit/1e3e41d030c417c045f0128ef4bd66a40dade5ed))

## [0.3.0](https://github.com/nimble-123/macos-hud/compare/v0.2.0...v0.3.0) (2026-10-17)


### Neue Funktionen

* **app:** run update checks and installations as jobs ([#66](https://github.com/nimble-123/macos-hud/issues/66)) ([727757b](https://github.com/nimble-123/macos-hud/commit/727757b114671097277f3f840f36032f8c9f1fae))
* **tui:** add updates tab with confirmation dialog ([#67](https://github.com/nimble-123/macos-hud/issues/67)) ([95245c2](https://github.com/nimble-123/macos-hud/commit/95245c2563b67c496b23094ef6859a12bb9aa5ec))
* **updates:** add adapter for global npm packages ([#64](https://github.com/nimble-123/macos-hud/issues/64)) ([5975a65](https://github.com/nimble-123/macos-hud/commit/5975a656ee8184451cc867f71b273afa03a17efa))
* **updates:** add install decision, update source port and Homebrew adapter ([#62](https://github.com/nimble-123/macos-hud/issues/62)) ([f1c6631](https://github.com/nimble-123/macos-hud/commit/f1c6631a189caf298f0738b875dec3af5f104381))
* **updates:** add Skills CLI adapter with update-all ([#65](https://github.com/nimble-123/macos-hud/issues/65)) ([6b979da](https://github.com/nimble-123/macos-hud/commit/6b979da89d795e031bfee9a0744344074af4a080))

## [0.2.0](https://github.com/nimble-123/macos-hud/compare/v0.1.0...v0.2.0) (2026-10-17)


### Neue Funktionen

* **release:** attach universal builds and update the Homebrew tap ([#61](https://github.com/nimble-123/macos-hud/issues/61)) ([2384cd3](https://github.com/nimble-123/macos-hud/commit/2384cd3afec89e3f62d1da18d43ebfee43455d31))


### Fehlerbehebungen

* **tui:** show repositories as they are scanned with a progress bar ([#59](https://github.com/nimble-123/macos-hud/issues/59)) ([82e2f1c](https://github.com/nimble-123/macos-hud/commit/82e2f1c181184f952b12a449f7a4b899003b08bd))

## 0.1.0 (2026-10-17)


### Neue Funktionen

* **app:** add workspace scan, sync and manual fast-forward use cases ([#52](https://github.com/nimble-123/macos-hud/issues/52)) ([06195b4](https://github.com/nimble-123/macos-hud/commit/06195b411a7d1b8b9a2ea000004f08f935fdcc57))
* **domain:** add repository status, sync decision and fast-forward permit ([#48](https://github.com/nimble-123/macos-hud/issues/48)) ([0febb1b](https://github.com/nimble-123/macos-hud/commit/0febb1b2973fafc8c9c60523d8c151b5ea23cd40))
* **git:** add GitCli adapter for status, fetch and fast-forward ([#49](https://github.com/nimble-123/macos-hud/issues/49)) ([51b2462](https://github.com/nimble-123/macos-hud/commit/51b24625a1924665ce57650e113dc49730b170cf))
* **infra:** add SQLite store with action log and scan cache ([#51](https://github.com/nimble-123/macos-hud/issues/51)) ([557f515](https://github.com/nimble-123/macos-hud/commit/557f5151159fdce41c14ce122094156c83886188))
* **infra:** load config and resolve the login shell PATH ([#41](https://github.com/nimble-123/macos-hud/issues/41)) ([4cb03ad](https://github.com/nimble-123/macos-hud/commit/4cb03ad7943ac36339ad0a2ccfdb9bf53a3d4cb7))
* **infra:** resolve the login shell PATH in the background ([#54](https://github.com/nimble-123/macos-hud/issues/54)) ([647b132](https://github.com/nimble-123/macos-hud/commit/647b1322232a62a0f72824b094bd7c0a3aa8e35f))
* **tui:** add first-run setup for workspaces and symbol set ([#56](https://github.com/nimble-123/macos-hud/issues/56)) ([e553c09](https://github.com/nimble-123/macos-hud/commit/e553c098752b18976aee2780c7d011fcf8fbd30c))
* **tui:** add i18n, theme resolution and symbol sets ([#39](https://github.com/nimble-123/macos-hud/issues/39)) ([a74c148](https://github.com/nimble-123/macos-hud/commit/a74c148ee42238c9c77dd8d2e18104d9d3920dda))
* **tui:** add terminal lifecycle, file logging and Elm event loop ([#37](https://github.com/nimble-123/macos-hud/issues/37)) ([f6f9b7a](https://github.com/nimble-123/macos-hud/commit/f6f9b7a70ed4f74ecb31ee9d9ffcde0d21e65895))
* **tui:** show workspaces with status, reasons and manual fast-forward ([#55](https://github.com/nimble-123/macos-hud/issues/55)) ([484b568](https://github.com/nimble-123/macos-hud/commit/484b568551de8fc556e403bad05028c96320511e))
