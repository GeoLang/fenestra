# Fenestra

[![CI](https://github.com/GeoLang/fenestra/actions/workflows/ci.yml/badge.svg)](https://github.com/GeoLang/fenestra/actions)
[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](LICENSE)

OGC services gateway for the GeoLang GIS stack — the GeoServer-equivalent component.

[Documentation](https://geolang.github.io/fenestra/) · [GitHub](https://github.com/GeoLang/fenestra)

## Features

- **WMS** — GetCapabilities (XML 1.3.0), GetMap with server-side rendering (SLD styles → PNG/JPEG)
- **WFS** — GetCapabilities (XML 2.0.0), GetFeature with bbox filtering, GeoJSON response, feature count limiting
- **WMTS** — GetCapabilities, GetTile request parsing, tile matrix set definitions
- **OGC API Features** — Landing page, conformance, collections, feature CRUD, bbox filtering, pagination
- **Server-Side Map Rendering** — CPU (tiny-skia) and GPU (Vello/wgpu) backends for rendering styled maps to images
- **SLD/SE styling** — Parse Styled Layer Descriptors: NamedLayer, Rules, PointSymbolizer, LineSymbolizer, PolygonSymbolizer, TextSymbolizer, Fill, Stroke, Graphic, Mark
- **HTTP server** — Axum-based, async, production-ready with configurable host/port
- **Configuration** — JSON-based layer config with per-layer CRS, BBOX, and data source paths
- **MVT encoding** — Mapbox Vector Tile binary encoding with geometry command sequences, tile-coordinate scaling, and tag interning
- **Platform Integration** — Proxies to Ptolemy for feature data, part of `docker-compose.platform.yml`

## Usage

```sh
# Start the server
fenestra serve --host 0.0.0.0 --port 8080

# Print default config
fenestra config
```

### Endpoints

- `GET /health` — Health check
- `GET /wms?SERVICE=WMS&REQUEST=GetCapabilities` — WMS capabilities
- `GET /wms?SERVICE=WMS&REQUEST=GetMap&LAYERS=...&BBOX=...&WIDTH=256&HEIGHT=256&FORMAT=image/png`
- `GET /wfs?SERVICE=WFS&REQUEST=GetCapabilities` — WFS capabilities
- `GET /wfs?SERVICE=WFS&REQUEST=GetFeature&TYPENAMES=roads&COUNT=10`
- `GET /wmts?SERVICE=WMTS&REQUEST=GetCapabilities` — WMTS capabilities
- `GET /wmts?SERVICE=WMTS&REQUEST=GetTile&LAYER=...&TILEMATRIX=...&TILEROW=0&TILECOL=0`
- `GET /ogc/` — OGC API landing page
- `GET /ogc/conformance` — Conformance declaration
- `GET /ogc/collections` — List feature collections
- `GET /ogc/collections/{id}/items` — Query features with bbox, limit, offset

## Architecture

```
fenestra-core    — OGC protocol implementations (WMS, WFS, WMTS, OGC API, SLD)
fenestra-cli     — HTTP server and CLI
```

## License

AGPL-3.0-or-later
