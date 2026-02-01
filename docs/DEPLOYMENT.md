# Deployment Guide

This document explains how to release mmt to various package managers.

## Prerequisites

### 1. crates.io Token
1. Go to https://crates.io/settings/tokens
2. Create a new token with `publish-update` scope
3. Add to GitHub Secrets as `CARGO_REGISTRY_TOKEN`

### 2. Homebrew Tap Repository
1. Create a new repository: `krzmknt/homebrew-tap`
2. Add the formula file `Formula/mmt.rb` (use template from `.github/homebrew/mmt.rb.template`)
3. Create a workflow to receive updates:

```yaml
# .github/workflows/update-formula.yml
name: Update Formula

on:
  repository_dispatch:
    types: [update-formula]

jobs:
  update:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Update formula
        run: |
          VERSION="${{ github.event.client_payload.version }}"
          SHA256="${{ github.event.client_payload.sha256 }}"

          sed -i "s|url \".*\"|url \"https://github.com/krzmknt/macos-music-tui/releases/download/v${VERSION}/mmt-${VERSION}-darwin-arm64.tar.gz\"|" Formula/mmt.rb
          sed -i "s|sha256 \".*\"|sha256 \"${SHA256}\"|" Formula/mmt.rb

      - name: Commit and push
        run: |
          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"
          git add Formula/mmt.rb
          git commit -m "Update mmt to v${{ github.event.client_payload.version }}"
          git push
```

4. Create a Personal Access Token with `repo` scope
5. Add to macos-music-tui GitHub Secrets as `HOMEBREW_TAP_TOKEN`

## Release Process

### Automatic Release (Recommended)

1. Create and push a tag:
```bash
git tag v2026.02.01.19.17
git push origin v2026.02.01.19.17
```

2. GitHub Actions will automatically:
   - Build the binary
   - Create a GitHub Release
   - Publish to crates.io
   - Update the Homebrew tap

### Manual Release

1. Go to Actions → Release → Run workflow
2. Enter the version number (e.g., `2026.02.01.19.17`)

## Installation Methods

### Cargo
```bash
cargo install macos-music-tui
```

### Homebrew
```bash
brew tap krzmknt/tap
brew install mmt
```

### Nix
```bash
nix run github:krzmknt/macos-music-tui
```

Or add to flake.nix:
```nix
{
  inputs.mmt.url = "github:krzmknt/macos-music-tui";
}
```

### Manual
```bash
curl -L https://github.com/krzmknt/macos-music-tui/releases/latest/download/mmt-darwin-arm64.tar.gz | tar xz
sudo mv mmt /usr/local/bin/
```
