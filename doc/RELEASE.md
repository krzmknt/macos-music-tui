# Release Guide

## 配布チャネル

- **Cargo** (crates.io): `cargo install macos-music-tui`
- **Homebrew**: `brew install krzmknt/tap/mmt`

## 事前準備 (初回のみ)

### 1. GitHub Secrets の設定

リポジトリの Settings > Secrets and variables > Actions に以下を追加:

| Secret名 | 説明 | 取得方法 |
|----------|------|----------|
| `CARGO_REGISTRY_TOKEN` | crates.io APIトークン | https://crates.io/settings/tokens |
| `HOMEBREW_TAP_TOKEN` | GitHub PAT (repo権限) | https://github.com/settings/tokens |

### 2. crates.io トークン作成

1. https://crates.io/settings/tokens にアクセス
2. 「New Token」をクリック
3. 以下のスコープを選択:
   - `publish-new` - 新規クレート公開
   - `publish-update` - バージョン更新
4. トークンをコピーして `CARGO_REGISTRY_TOKEN` に設定

### 3. Homebrew tap リポジトリ作成

1. GitHub で `krzmknt/homebrew-tap` リポジトリを作成
2. `homebrew-tap/` ディレクトリの内容をコピー:
   ```
   homebrew-tap/
   ├── Formula/
   │   └── mmt.rb
   └── .github/
       └── workflows/
           └── update-formula.yml
   ```

### 4. GitHub PAT 作成

1. https://github.com/settings/tokens にアクセス
2. 「Generate new token (classic)」をクリック
3. スコープ: `repo` (Full control of private repositories)
4. トークンをコピーして `HOMEBREW_TAP_TOKEN` に設定

## リリース手順

### 1. バージョン更新

```bash
# Cargo.toml のバージョンを更新
vim Cargo.toml  # version = "0.2.0" など

# コミット
git add Cargo.toml
git commit -m "Bump version to 0.2.0"
git push
```

### 2. タグ作成 & プッシュ

```bash
git tag v0.2.0
git push origin v0.2.0
```

### 3. 自動実行される処理

1. **build**: macOS でバイナリをビルド
2. **publish-cargo**: crates.io に公開
3. **publish-github-release**: GitHub Release 作成
4. **update-homebrew**: Homebrew formula 更新

### 手動リリース (オプション)

GitHub Actions の「Run workflow」から手動実行も可能:
1. Actions タブを開く
2. 「Release」ワークフローを選択
3. 「Run workflow」をクリック
4. バージョン (例: `0.2.0`) を入力して実行

## トラブルシューティング

### crates.io 公開エラー

- トークンのスコープを確認 (`publish-new`, `publish-update`)
- `Cargo.toml` のメタデータ (description, license, repository) を確認

### Homebrew 更新エラー

- `HOMEBREW_TAP_TOKEN` の権限を確認 (`repo` スコープ)
- `homebrew-tap` リポジトリの存在を確認
