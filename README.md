# HARetroPanel

HARetroPanel is a lightweight Rust-based dashboard designed to display and control Home Assistant entities using a simple and legacy‑friendly web interface.
It is optimized for old devices (like early‑generation iPads) by rendering server‑side HTML without any modern JavaScript requirements.

![Dashboard](/assets/haretropanel.png)

## 🐳 Docker (Recommended)

The easiest way to run HARetroPanel is using Docker from Docker Hub.

### Using `docker run`

```bash
docker run -d \
  --name haretropanel \
  -p 8080:8080 \
  -e HA_BASE_URL=http://homeassistant.local:8123 \
  -e HA_TOKEN=YOUR_LONG_LIVED_TOKEN \
  -e HARETROPANEL_PORT=8080 \
  -v ./data:/app/data \
  -v ./logs:/app/logs \
  rozgonyiadam/haretropanel:latest
```

### Using `docker-compose`

```yaml
services:
  haretropanel:
    image: rozgonyiadam/haretropanel:latest
    container_name: haretropanel
    env_file:
      - .env.local
    environment:
      HARETROPANEL_PORT: "8080"
      HARETROPANEL_LOG_DIR: "/app/logs"
      HARETROPANEL_LOG_ROTATION: "daily"
      HARETROPANEL_LOG_LEVEL: "haretropanel=info,tower_http=info"
    ports:
      - "8080:8080"
    volumes:
      - ./logs:/app/logs
      - ./data:/app/data
    restart: unless-stopped
```

---

## 📦 Running Without Docker (Prebuilt Releases)

Download from: **GitHub → Releases → Assets**

Available packages:
- Linux (x86_64): `haretropanel-vX.Y.Z-linux-x86_64.tar.gz`
- Linux (ARM64): `haretropanel-vX.Y.Z-linux-arm64.tar.gz`
- Windows (x86_64): `haretropanel-vX.Y.Z-windows-x86_64.zip`

Extract the archive.

## 🔧 Configuration

Create `.env` file:

```
HARETROPANEL_PORT=8080
HA_BASE_URL=http://homeassistant.local:8123
HA_TOKEN=YOUR_LONG_LIVED_TOKEN

HARETROPANEL_LOG_DIR=./logs
HARETROPANEL_LOG_ROTATION=daily
HARETROPANEL_LOG_LEVEL=haretropanel=info,tower_http=info

HARETROPANEL_CACHE_TTL_DEFAULT_SECS=5
```

## ▶️ Run

Linux / macOS:
```
./haretropanel
```

Windows:
```
haretropanel.exe
```

Open in browser:
```
http://localhost:8080
```

## ⚙️ Development

```bash
cargo run
```

## 📌 Notes

- Not intended as Lovelace replacement
- Built for reliability and old hardware