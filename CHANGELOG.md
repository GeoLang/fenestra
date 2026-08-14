# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Fixed

- Honest docs (2026-08-14): the README no longer lists MVT encoding as a server
  feature, it is an unwired `fenestra-core` module and is now noted as such. The
  docs page drops the unmeasured "∞ concurrent req" and "<1ms capability gen"
  stats and gains the WMTS, WCS, rendering, SLD and OGC API Features cards it
  was missing.

### Added

- SLD filter parsing: property comparisons, `PropertyIsBetween` and `ElseFilter`.
- `POST /sld/symbology`, converting an SLD style into the viewer's symbology JSON and reporting every construct that shape cannot carry.

## [0.1.0] - 2026-05-30

### Added

- Initial release.
