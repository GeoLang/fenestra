# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Changed

- 2026-11-04: JWT checks come from the shared `platform-auth` crate. `FENESTRA_JWT_SECRET` is read once at startup instead of on every request. Requests now get 401 for a token with an empty `sub`, a `token_use`, `agora_use` or `geolang_use` claim with any value including JSON null, a non-string `aud`, or a `name` that is a number, boolean, array or object, and for every token when the secret is set but empty or only whitespace.
- 2026-11-04: `ci.yml` and `release.yml` call the reusable workflows in `GeoLang/.github`.
- 2026-09-02: `terrano-core` pinned at git tag `v0.2.0`, up from `v0.1.0`. The WCS coverage path still reads single-band float64 through `read_geotiff`.

### Removed

- 2026-09-02: the `fenestra-core::processes` module. It held OGC API Processes
  request, job and process description types plus two built-in process
  descriptions, and no endpoint or crate ever called them.
- 2026-09-02: the `fenestra-inspire`, `fenestra-geofence`, `fenestra-printing`
  and `fenestra-cascade` crates, all stubs that the server never depended on,
  and with them the `fenestra-core::plugin` module whose `Plugin` trait and
  registry they were the only implementors of.

### Fixed

- 2026-11-05: a `FENESTRA_JWT_SECRET` that is set but not valid UTF-8 stops the
  server at startup with an error naming the variable. It used to count as
  unset, which turned auth off.
- 2026-08-28: SLD GetMap draws TextSymbolizer labels and every symbolizer in a
  rule, in document order.

- 2026-08-21: GetMap and GetFeature helpers return `Response` directly.
  Clippy `result_large_err` failed CI after they were extracted as
  `Result<T, Response>`.

- README SLD rendering limits: filters and scale bounds are applied.
- Honest docs (2026-08-14): the README no longer lists MVT encoding as a server
  feature, it is an unwired `fenestra-core` module and is now noted as such. The
  docs page drops the unmeasured "∞ concurrent req" and "<1ms capability gen"
  stats and gains the WMTS, WCS, rendering, SLD and OGC API Features cards it
  was missing.

### Added

- 2026-08-30: OGC API Features reaches the core conformance class it declares.
  `GET /ogc/collections/{id}/items/{featureId}` returns one feature, `/ogc/api`
  serves the OpenAPI 3.0 document the landing page already advertised as
  `service-desc`, and collection, items and feature responses carry `links`
  (`self`, `collection`, and the `next`/`prev` pages that exist, each keeping
  the request's `limit` and `bbox`). Items and features are served as
  `application/geo+json`.
- 2026-08-30: vector tiles.
  `GET /ogc/collections/{id}/tiles/WebMercatorQuad/{tileMatrix}/{tileRow}/{tileCol}`
  encodes a collection's features through the `fenestra-core` MVT encoder,
  which until now no endpoint called.
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
