# Fenestra

[![CI](https://github.com/GeoLang/fenestra/actions/workflows/ci.yml/badge.svg)](https://github.com/GeoLang/fenestra/actions)
[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](LICENSE)

OGC services gateway for the GeoLang GIS stack, the GeoLang counterpart to GeoServer. It serves Ptolemy datasets over WMS, WFS, WMTS, OGC API Features and MVT vector tiles, and GeoTIFF files over WCS.

[Documentation](https://geolang.github.io/fenestra/) · [GitHub](https://github.com/GeoLang/fenestra)

## Features

- **WMS 1.3.0.** GetCapabilities and GetMap, rendered to PNG in EPSG:4326 or EPSG:3857. Output is always PNG, and `FORMAT` and `STYLES` are parsed and ignored. There are no stored styles, so a styled GetMap carries the SLD itself: `SLD_BODY` holds an inline document and `SLD` holds a URL the server fetches. The NamedLayer whose name matches the requested layer is used, otherwise the first one. No GetFeatureInfo.
- **WFS 2.0.0 KVP.** GetCapabilities, DescribeFeatureType, and GetFeature with bbox filtering, `COUNT`/`STARTINDEX` paging and `RESULTTYPE=hits`. Features come back as GeoJSON. An `OUTPUTFORMAT` other than `application/json` or its aliases, or an `SRSNAME` other than EPSG:4326, gets an OWS ExceptionReport. DescribeFeatureType reads property names and types off the first feature of each collection, so a collection with mixed properties is described by that one feature.
- **WMTS 1.0.0.** GetCapabilities and GetTile, KVP and RESTful, on WebMercatorQuad levels 0 to 18. Tiles go through the WMS render path unstyled.
- **WCS 2.0.1 core, KVP.** GetCapabilities, DescribeCoverage, and GetCoverage with bbox subsetting in the native CRS, returned as GeoTIFF. Each GeoTIFF file in `COVERAGE_DIR` is one coverage, and its id is the file stem. Single-band float64 only, with no reprojection or scaling. A file without a CRS geokey is declared EPSG:4326.
- **OGC API Features.** Landing page, conformance, collections, items with bbox filtering and pagination, single features at `items/{featureId}`, and the OpenAPI 3.0 document at `/ogc/api`. An items page links `self`, `collection` and whichever of `next` and `prev` exist, each carrying the request's `limit` and `bbox`. The single-item route pages through Ptolemy's GeoJSON export 10,000 rows at a time, so a missing id costs one request per 10,000 features. Read only, with no CQL2 or `datetime` filtering.
- **Vector tiles.** `/ogc/collections/{id}/tiles/WebMercatorQuad/{tileMatrix}/{tileRow}/{tileCol}` returns MVT with one layer named after the collection, features in web mercator at a 4096 extent. Tiles are unclipped and unsimplified: a feature that touches the tile is encoded whole. Properties that are not a string, number or boolean are dropped, and so is any feature id that is not a number.
- **Capabilities documents.** WMS, WFS, WMTS and WCS each declare their namespace and `xsi:schemaLocation` and build every OnlineResource from `FENESTRA_PUBLIC_URL`. Each layer carries the extent of its own features, or the world extent when it has none. WFS lists its operations and the conformance classes the server implements in `ows:OperationsMetadata`. GDAL's WFS, WMS and WMTS drivers read them: `ogrinfo -ro -so "WFS:http://localhost:8080/wfs"`, `gdalinfo "WMS:http://localhost:8080/wms?"` and `gdalinfo "WMTS:http://localhost:8080/wmts?request=GetCapabilities"`.
- **Rendering.** CPU rendering with tiny-skia. A GPU backend (Vello/wgpu) sits behind the `vello` feature of `fenestra-core`, is off in the `fenestra` binary and is experimental.
- **SLD/SE styling.** Parses NamedLayer, Rules, filters (property comparisons, ranges, `ElseFilter`), scale denominators, and the Point, Line, Polygon and Text symbolizers with Fill, Stroke, Graphic and Mark. A rule draws every symbolizer in document order. Labels use the bundled Caladea font, and `font-family` is parsed and unused.
- **SLD to symbology.** `POST /sld/symbology` converts a style into the viewer's graduated, categorized or rule-based symbology and lists every SLD construct that shape cannot carry instead of approximating it.

Fenestra is not tuned for production. Every WMS request, WMTS tile, GetFeature and capabilities document re-fetches up to 100,000 features per layer from Ptolemy with no cache, and there is no rate limit or upstream timeout.

A layer or collection name is a Ptolemy dataset name. Features come from the dataset's `main` branch, or its first branch when there is no `main`. Requests to Ptolemy carry no credentials. Fenestra runs as part of viewtopia's `docker-compose.platform.yml`, and tagged releases publish `ghcr.io/geolang/fenestra`.

## Usage

```sh
cargo run --release -p fenestra-cli -- serve --port 8080
# or, with the binary installed
fenestra serve --host 0.0.0.0 --port 8080

# prints the built-in defaults as JSON, the server reads no config file
fenestra config

curl "http://localhost:8080/wcs?SERVICE=WCS&REQUEST=GetCoverage&COVERAGEID=dem&SUBSET=x(10.5,11.5)&SUBSET=y(49,50)" -o subset.tif
```

### Endpoints

- `GET /health`: health check
- `GET /healthz`, `GET /readyz`: liveness and readiness probes
- `GET /metrics`: Prometheus metrics
- `GET /wms?SERVICE=WMS&REQUEST=GetCapabilities`
- `GET /wms?SERVICE=WMS&REQUEST=GetMap&LAYERS=...&BBOX=...&WIDTH=256&HEIGHT=256&FORMAT=image/png`
- `GET /wms?SERVICE=WMS&REQUEST=GetMap&LAYERS=...&SLD_BODY=<url-encoded SLD>`: `SLD=<url>` fetches the document instead
- `GET /wfs?SERVICE=WFS&REQUEST=GetCapabilities`
- `GET /wfs?SERVICE=WFS&REQUEST=DescribeFeatureType&TYPENAMES=roads`: every feature type when `TYPENAMES` is omitted
- `GET /wfs?SERVICE=WFS&REQUEST=GetFeature&TYPENAMES=roads&COUNT=10&STARTINDEX=20`
- `GET /wfs?SERVICE=WFS&REQUEST=GetFeature&TYPENAMES=roads&RESULTTYPE=hits`: match count only
- `GET /wmts?SERVICE=WMTS&REQUEST=GetCapabilities`
- `GET /wmts?SERVICE=WMTS&REQUEST=GetTile&LAYER=...&TILEMATRIX=...&TILEROW=0&TILECOL=0`
- `GET /wmts/{layer}/{tileMatrixSet}/{tileMatrix}/{tileRow}/{tileCol}.png`: `.png` is optional, and the tile matrix set segment is not checked
- `GET /wcs?SERVICE=WCS&REQUEST=GetCapabilities`
- `GET /wcs?SERVICE=WCS&REQUEST=DescribeCoverage&COVERAGEID=dem`
- `GET /wcs?SERVICE=WCS&REQUEST=GetCoverage&COVERAGEID=dem&SUBSET=x(10.5,11.5)&SUBSET=y(49,50)`
- `GET /ogc/`: landing page
- `GET /ogc/api`: OpenAPI 3.0 document
- `GET /ogc/conformance`
- `GET /ogc/collections`
- `GET /ogc/collections/{id}`
- `GET /ogc/collections/{id}/items`: `bbox`, `limit` (default 10) and `offset`
- `GET /ogc/collections/{id}/items/{featureId}`
- `GET /ogc/collections/{id}/tiles/WebMercatorQuad/{tileMatrix}/{tileRow}/{tileCol}`: MVT, `tileMatrix` up to 24
- `POST /sld/symbology`: SLD document in the body, symbology JSON out. `?layer=` and `?style=` pick one of several in the document

### Environment

| Variable | Default | Purpose |
|---|---|---|
| `PTOLEMY_URL` | `http://ptolemy:3000` | feature source |
| `COVERAGE_DIR` | `coverages` | GeoTIFF coverages for WCS |
| `FENESTRA_JWT_SECRET` | unset (auth off) | JWT secret for `Authorization: Bearer`. Health, probes and metrics stay public |
| `FENESTRA_PUBLIC_URL` | `http://<host>:<port>` | externally reachable base URL, path prefix included, for every capabilities document and OGC API link. Set it behind a reverse proxy |

## Crates

- `fenestra-core`: OGC protocol implementations (WMS, WFS, WMTS, WCS, OGC API, MVT, SLD) and the renderer
- `fenestra-cli`: the `fenestra` binary and HTTP server

## License

AGPL-3.0-or-later, see [LICENSE](LICENSE).

Copyright (C) 2026 Grok Image Compression Inc.
