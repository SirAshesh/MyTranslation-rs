# Installation Guide

Follow these steps to install and start the **MyTranslation-rs** server on Windows or Linux.

## Windows (x64)

1. Download `MyTranslation-rs-windows-x64.zip` from the [Releases](https://github.com/SirAshesh/MyTranslation-rs/releases) page.
2. Extract the archive into a folder.
3. Run `server-rs.exe`.
4. Open your browser at `http://127.0.0.1:5000`.

## Linux / Ubuntu (x64)

You can install, update, and run the server using the single-line automated runner:

```bash
curl -sSL https://raw.githubusercontent.com/SirAshesh/MyTranslation-rs/main/run.sh -o run.sh
chmod +x run.sh
./run.sh
```

The script automatically detects updates from GitHub Releases, downloads missing binaries/models, and launches the server.

### Manual Installation

1. Download `MyTranslation-rs-ubuntu-x64.tar.gz` from the [Releases](https://github.com/SirAshesh/MyTranslation-rs/releases) page.
2. Extract the archive and launch the server:
   ```bash
   tar -xzf MyTranslation-rs-ubuntu-x64.tar.gz
   cd MyTranslation-rs
   chmod +x server-rs run.sh
   ./run.sh
   ```
3. Open `http://127.0.0.1:5000` in your browser.

### Docker Installation

You can self-host the server using the prebuilt image from GitHub Container Registry (`ghcr.io/sirashesh/mytranslation-rs:latest`) or build locally from [Dockerfile](../Dockerfile). All persistent data (`config.json`, `storage.db`, and downloaded local models) is stored in `/app/data`.

Using Docker Compose (prebuilt image, capped at 4 GB RAM via `mem_limit: 4g`):
```bash
curl -sSL https://raw.githubusercontent.com/SirAshesh/MyTranslation-rs/main/docker-compose.yml -o docker-compose.yml
docker compose up -d
```

Or build from source with Docker Compose:
```bash
git clone https://github.com/SirAshesh/MyTranslation-rs.git
cd MyTranslation-rs
docker compose up -d --build
```

Or using Docker CLI:
```bash
docker run -d \
  --name mytranslation-rs \
  --memory=4g \
  -p 5000:5000 \
  -v mytranslation_data:/app/data \
  --restart unless-stopped \
  ghcr.io/sirashesh/mytranslation-rs:latest
```

Once the container is healthy, open `http://127.0.0.1:5000` (or your host IP on port `5000`) in your browser.

## Building from Source

If you prefer compiling from source instead of using prebuilt binaries:

### Prerequisites

- [Rust Toolchain](https://rustup.rs/) (1.75 or newer)
- **Windows**: Windows 10/11 x64 with a DirectX 12 compatible GPU for DirectML acceleration.
- **Ubuntu / Debian**:
  ```bash
  sudo apt-get update
  sudo apt-get install -y build-essential libssl-dev pkg-config
  ```

### Build Commands

```bash
git clone https://github.com/SirAshesh/MyTranslation-rs.git
cd MyTranslation-rs
cargo build --release
```

- **Windows binary**: `target/release/server-rs.exe` (requires `DirectML.dll` from the `target/release` directory).
- **Linux binary**: `target/release/server-rs`.

## Next Step

Once the server is running, continue to [Connect Local Client](Connect-Local-Client.md) to configure the dashboard and connect your browser extension.
