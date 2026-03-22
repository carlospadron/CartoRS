# CartoRS

A high-performance, headless GIS engine built in Rust. A cloud-native alternative to GeoServer designed for DevOps. Serve WMS and WFS directly from GeoPackage using simple YAML configurations. No UI, no Java, just raw speed.

## Features

- **WMS 1.3.0** — `GetCapabilities` and `GetMap` (PNG output)
- **WFS 2.0** — `GetCapabilities` and `GetFeature` (GeoJSON output)
- **GeoPackage** data source (SQLite-based, zero external dependencies)
- **YAML configuration** — one file to define server, services, datasources, and layers
- **Health check** endpoint at `/health`
- **CORS** enabled by default for browser-based map clients

## Quick Start

### Build

```bash
cargo build --release
```

### Configure

Create a `config.yaml` (see `config.example.yaml` for a full example):

```yaml
server:
  host: "0.0.0.0"
  port: 8080

services:
  wms:
    enabled: true
    title: "My WMS"
  wfs:
    enabled: true
    title: "My WFS"

datasources:
  - name: "mydata"
    type: "geopackage"
    path: "/data/my.gpkg"

layers:
  - name: "buildings"
    title: "Buildings"
    datasource: "mydata"
    table: "buildings"
    srs: "EPSG:4326"
    geometry_column: "geom"
    bbox: [-180.0, -90.0, 180.0, 90.0]
```

### Run

```bash
cartors --config config.yaml
```

### Endpoints

| Endpoint | Example |
|----------|---------|
| Health | `GET /health` |
| WMS GetCapabilities | `GET /wms?SERVICE=WMS&REQUEST=GetCapabilities` |
| WMS GetMap | `GET /wms?SERVICE=WMS&REQUEST=GetMap&LAYERS=buildings&BBOX=-74,40,-73,41&WIDTH=800&HEIGHT=600&FORMAT=image/png` |
| WFS GetCapabilities | `GET /wfs?SERVICE=WFS&REQUEST=GetCapabilities` |
| WFS GetFeature | `GET /wfs?SERVICE=WFS&REQUEST=GetFeature&TYPENAMES=buildings&BBOX=-74,40,-73,41&COUNT=100` |

## Configuration Reference

### `server`
| Field | Default | Description |
|-------|---------|-------------|
| `host` | `0.0.0.0` | Bind address |
| `port` | `8080` | Bind port |

### `services.wms` / `services.wfs`
| Field | Default | Description |
|-------|---------|-------------|
| `enabled` | `true` | Enable/disable the service |
| `title` | `CartoRS Service` | Service title in capabilities |

### `datasources[]`
| Field | Required | Description |
|-------|----------|-------------|
| `name` | yes | Unique identifier |
| `type` | yes | `geopackage` |
| `path` | yes | Path to `.gpkg` file |

### `layers[]`
| Field | Default | Description |
|-------|---------|-------------|
| `name` | required | Layer identifier (used in WMS/WFS requests) |
| `title` | required | Human-readable title |
| `datasource` | required | References a datasource by name |
| `table` | required | Table name in the datasource |
| `srs` | `EPSG:4326` | Spatial reference system |
| `geometry_column` | `geom` | Name of the geometry column |
| `bbox` | `[-180,-90,180,90]` | Bounding box `[minx, miny, maxx, maxy]` |

## Environment Variables

| Variable | Description |
|----------|-------------|
| `RUST_LOG` | Log level filter (e.g. `info`, `debug`, `cartors=debug`) |

## License

Apache-2.0
