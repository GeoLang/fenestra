# Fenestra

OGC services gateway for the TileTopia-HQ GIS stack.

## Features

- **WMS** — GetCapabilities, GetMap request parsing
- **WFS** — GetCapabilities, GetFeature with bbox filtering
- **HTTP server** — Axum-based, async, production-ready
- **Configuration** — JSON-based layer and service config

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

## License

AGPL-3.0-or-later
