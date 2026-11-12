#!/usr/bin/env bash
set -e

REPO="${REPO:-SirAshesh/MyTranslation-rs}"
ASSET_NAME="ImageTrans-rs-ubuntu-x64.tar.gz"
WORK_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$WORK_DIR"

echo "=== ImageTrans-rs Deployment & Update Runner ==="
echo "Working directory: $WORK_DIR"

# Verify system architecture
ARCH=$(uname -m)
if [ "$ARCH" != "x86_64" ]; then
    echo "Warning: ImageTrans-rs release binary is built for x86_64, but system is $ARCH."
fi

# Ensure basic utilities exist
for tool in curl tar; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "Required tool '$tool' is not installed."
        if command -v apt-get >/dev/null 2>&1; then
            echo "Installing $tool via apt..."
            sudo apt-get update -qq && sudo apt-get install -y "$tool"
        else
            echo "Please install $tool using your package manager."
            exit 1
        fi
    fi
done

# Read current version
CURRENT_VERSION=""
if [ -f ".version" ]; then
    CURRENT_VERSION=$(cat .version | tr -d '[:space:]')
fi

# Query latest release tag
echo "Checking for latest release from https://github.com/$REPO..."
LATEST_TAG=""
API_RESPONSE=$(curl -sSL "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null || true)
if [ -n "$API_RESPONSE" ]; then
    LATEST_TAG=$(echo "$API_RESPONSE" | grep -Po '"tag_name":\s*"\K[^"]*' || true)
fi

if [ -z "$LATEST_TAG" ]; then
    # Fallback to redirect URL to avoid GitHub API rate limits
    EFFECTIVE_URL=$(curl -sSLI -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest" 2>/dev/null || true)
    LATEST_TAG=$(echo "$EFFECTIVE_URL" | rev | cut -d'/' -f1 | rev)
fi

if [ -z "$LATEST_TAG" ] || [ "$LATEST_TAG" = "latest" ]; then
    if [ -f "./server-rs" ]; then
        echo "Could not check latest release online (offline or rate limited). Running existing binary."
    else
        echo "Error: Could not retrieve release tag and no existing binary found."
        exit 1
    fi
elif [ ! -f "./server-rs" ] || [ "$CURRENT_VERSION" != "$LATEST_TAG" ]; then
    echo "New release detected: $LATEST_TAG (current: ${CURRENT_VERSION:-none})"
    DOWNLOAD_URL="https://github.com/$REPO/releases/download/$LATEST_TAG/$ASSET_NAME"
    echo "Downloading $DOWNLOAD_URL..."

    HTTP_STATUS=$(curl -fSL -w "%{http_code}" -o "$ASSET_NAME" "$DOWNLOAD_URL" 2>/dev/null || true)
    if [ ! -f "$ASSET_NAME" ] || [ ! -s "$ASSET_NAME" ]; then
        echo "Failed to download $ASSET_NAME from $DOWNLOAD_URL (status: $HTTP_STATUS)."
        if [ -f "./server-rs" ]; then
            echo "Falling back to existing server-rs binary."
        else
            echo "No existing binary available to fall back to."
            exit 1
        fi
    else
        echo "Extracting release package..."
        tar -xzf "$ASSET_NAME" --strip-components=1
        rm -f "$ASSET_NAME"
        chmod +x server-rs
        if [ -f "run.sh" ]; then
            chmod +x run.sh
        fi

        if [ ! -f "config.json" ] && [ -f "config.example.json" ]; then
            cp config.example.json config.json
            echo "Created default config.json from template."
        fi

        echo "$LATEST_TAG" > .version
        echo "Successfully updated to $LATEST_TAG."
    fi
else
    echo "Server is up to date ($CURRENT_VERSION)."
fi

if [ ! -x "./server-rs" ]; then
    echo "Error: server-rs binary is missing or not executable."
    exit 1
fi

echo "Starting ImageTrans-rs server..."
exec ./server-rs "$@"

