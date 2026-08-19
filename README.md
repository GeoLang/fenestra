# Fenestra

[![CI](https://github.com/GeoLang/fenestra/actions/workflows/ci.yml/badge.svg)](https://github.com/GeoLang/fenestra/actions)
[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](LICENSE)

OGC services gateway for the GeoLang GIS stack — the GeoServer-equivalent component.

[Documentation](https://geolang.github.io/fenestra/) · [GitHub](https://github.com/GeoLang/fenestra)

## Features

- **WMS** — GetCapabilities (XML 1.3.0), GetMap with server-side rendering to PNG, EPSG:4326 and EPSG:3857. Output is always PNG: `FORMAT` and `STYLES` are parsed and ignored
- **WFS** — GetCapabilities (XML 2.0.0), GetFeature with bbox filtering, feature count limiting. Features come back as GeoJSON with content type `application/json`; `OUTPUTFORMAT` is ignored
- **WMTS** — GetCapabilities, GetTile (KVP and RESTful), tiles rendered through the same path as WMS on the WebMercatorQuad grid. Tiles are rendered unstyled: no SLD is applied
- **GetCapabilities documents** — minimal and not yet client-consumable. The WFS document emits `wfs:` and `ows:` prefixes with no `xmlns:` declarations and lists only Name and Title per FeatureType; the WMS document has no xmlns, Service Name, OnlineResource, Capability/Request section or layer bounding boxes; the WMTS TileMatrixSet has an Identifier and SupportedCRS but no TileMatrix levels. QGIS and other conformant clients will not consume them
- **WCS** — 2.0.1 core (KVP): GetCapabilities, DescribeCoverage, GetCoverage with bbox subsetting in the native CRS, GeoTIFF output. Coverages are GeoTIFF files in `COVERAGE_DIR` (default `./coverages`), one coverage per file, id = file stem. Single-band float64 only, no reprojection or scaling; files without a CRS geokey are declared EPSG:4326
- **OGC API Features** — Landing page, conformance, collections, items with bbox filtering and pagination (read access). Not conformant yet: there is no single-feature route (`items/{featureId}`), no OpenAPI `service-desc` document, and item responses carry no `links`
- **Server-Side Map Rendering** — CPU (tiny-skia) backend rendering styled maps to PNG. A GPU (Vello/wgpu) backend exists behind the optional `vello` feature and is experimental
- **SLD/SE styling** — Parse Styled Layer Descriptors: NamedLayer, Rules, filters (property comparisons, ranges, else), PointSymbolizer, LineSymbolizer, PolygonSymbolizer, TextSymbolizer, Fill, Stroke, Graphic, Mark. Rendering limits: filters and scale bounds are parsed but not applied, so every rule paints every feature and the last rule wins; text symbolizers are never drawn; only the first symbolizer of each type in a rule is kept
- **SLD to symbology** — `POST /sld/symbology` converts a style into the viewer's graduated, categorized or rule-based symbology, and reports every SLD construct that shape cannot carry instead of approximating it
- **HTTP server** — Axum-based, async, with configurable host/port. Not tuned for production: every WMS request and WMTS tile re-fetches up to 100,000 features from Ptolemy with no cache, and there is no rate limit or upstream timeout
- **Configuration** — `fenestra config` prints a default config. Layers are derived from Ptolemy collections, not from a config file
- **Platform Integration** — Proxies to Ptolemy for feature data, part of `docker-compose.platform.yml`

## Usage

```sh
# Start the server
fenestra serve --host 0.0.0.0 --port 8080

# Print default config
fenestra config

# Fetch a GeoTIFF subset from a coverage in COVERAGE_DIR
curl "http://localhost:8080/wcs?SERVICE=WCS&REQUEST=GetCoverage&COVERAGEID=dem&SUBSET=x(10.5,11.5)&SUBSET=y(49,50)" -o subset.tif
```

### Endpoints

- `GET /health` — Health check
- `GET /healthz`, `GET /readyz` — Liveness and readiness probes
- `GET /metrics` — Prometheus metrics
- `GET /wms?SERVICE=WMS&REQUEST=GetCapabilities` — WMS capabilities
- `GET /wms?SERVICE=WMS&REQUEST=GetMap&LAYERS=...&BBOX=...&WIDTH=256&HEIGHT=256&FORMAT=image/png`
- `GET /wfs?SERVICE=WFS&REQUEST=GetCapabilities` — WFS capabilities
- `GET /wfs?SERVICE=WFS&REQUEST=GetFeature&TYPENAMES=roads&COUNT=10`
- `GET /wmts?SERVICE=WMTS&REQUEST=GetCapabilities` — WMTS capabilities
- `GET /wmts?SERVICE=WMTS&REQUEST=GetTile&LAYER=...&TILEMATRIX=...&TILEROW=0&TILECOL=0`
- `GET /wcs?SERVICE=WCS&REQUEST=GetCapabilities` — WCS capabilities
- `GET /wcs?SERVICE=WCS&REQUEST=DescribeCoverage&COVERAGEID=dem`
- `GET /wcs?SERVICE=WCS&REQUEST=GetCoverage&COVERAGEID=dem&SUBSET=x(10.5,11.5)&SUBSET=y(49,50)`
- `GET /ogc/` — OGC API landing page
- `GET /ogc/conformance` — Conformance declaration
- `GET /ogc/collections` — List feature collections
- `GET /ogc/collections/{id}` — Single collection description
- `GET /ogc/collections/{id}/items` — Query features with bbox, limit, offset
- `POST /sld/symbology` — Convert an SLD document (request body) into viewer symbology JSON; `?layer=` and `?style=` pick one of several in the document

### Environment

| Variable | Default | Purpose |
|---|---|---|
| `PTOLEMY_URL` | `http://ptolemy:3000` | Feature source |
| `COVERAGE_DIR` | `./coverages` | GeoTIFF coverages for WCS |
| `FENESTRA_JWT_SECRET` | unset (auth off) | JWT secret; health and metrics stay public |
| `FENESTRA_PUBLIC_URL` | `http://<host>:<port>` | Externally reachable base URL, path prefix included, for the absolute URLs in WMTS capabilities and the OGC API links. Set it when fenestra sits behind a reverse proxy |

## Architecture

```
fenestra-core    — OGC protocol implementations (WMS, WFS, WMTS, WCS, OGC API, SLD)
fenestra-cli     — HTTP server and CLI
```

The workspace also holds four library crates that the server does not depend on yet:
`fenestra-inspire` (stub: CSW request/response types with no parsing or XML generation, plus
three substring checks for INSPIRE metadata), `fenestra-geofence` (spatial access control),
`fenestra-cascade` (stub: rewrites an upstream URL and caches, but makes no HTTP request), and
`fenestra-printing` (PDF output, still a stub).
`fenestra-core` also carries a Mapbox Vector Tile encoder (`mvt` module) that no endpoint
serves yet.

## License

AGPL-3.0-or-later, see [LICENSE](LICENSE).

Copyright (C) 2026 Grok Image Compression Inc.
