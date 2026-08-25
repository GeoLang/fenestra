# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Fixed

- 2026-08-21: GetMap and GetFeature helpers return `Response` directly.
  Clippy `result_large_err` failed CI after they were extracted as
  `Result<T, Response>`.

- Honest docs (2026-08-14): the README no longer lists MVT encoding as a server
  feature, it is an unwired `fenestra-core` module and is now noted as such. The
  docs page drops the unmeasured "∞ concurrent req" and "<1ms capability gen"
  stats and gains the WMTS, WCS, rendering, SLD and OGC API Features cards it
  was missing.

### Added

- 2026-08-25: client-consumable capabilities. WMS, WFS, WMTS and WCS declare
  their namespace and `xsi:schemaLocation`, every OnlineResource comes from
  `FENESTRA_PUBLIC_URL`, and each layer carries the extent of its own features
  in the form its document requires. WFS gains `ows:OperationsMetadata`,
  per-FeatureType `DefaultCRS`/`OtherCRS`, DescribeFeatureType, `STARTINDEX`
  and `RESULTTYPE=hits`. WMTS layers gain the Style, Format and
  `ows:WGS84BoundingBox` clients need. GDAL 3.11 reads all three.
- SLD filter parsing: property comparisons, `PropertyIsBetween` and `ElseFilter`.
- `POST /sld/symbology`, converting an SLD style into the viewer's symbology JSON and reporting every construct that shape cannot carry.

## [0.1.0] - 2026-05-30

### Added

- Initial release.
