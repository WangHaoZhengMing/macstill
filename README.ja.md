# Macstill

[English](README.md) · [简体中文](README.zh-CN.md) · [Deutsch](README.de.md) · [日本語](README.ja.md)

Macstill は **Rust、GPUI、Hyprland** で構築された、macOS に着想を得た Linux デスクトップ環境です。デスクトップシェル、Finder ファイルマネージャー、システム設定、システム情報、スクリーンショットツール、AirDrop 連携を統合しています。

## プレビュー

### デスクトップの概要
![Macstill デスクトップの概要](assets/img/overview.png)

### 動作デモ
![Macstill デスクトップのデモ](https://github.com/WangHaoZhengMing/assets/raw/main/ezgif-screen-recording-2026-10-26.mp4)

## 機能

- **Launchpad** — アプリの起動と管理。
- **Finder** — ファイル検索を統合したファイルマネージャー。
- **Stage Manager** — ウィンドウのグループ化と切り替え。
- **Mission Control** — ワークスペースとウィンドウの一覧。
- **Dock** — アプリの固定と実行状態の表示。
- **カスタムアプリアイコン** — アイコンの置き換えや、使わないプリセットアイコンの非表示。
- **システム情報** — ハードウェアとシステム状態の表示。一部実装済み。
- **システム設定** — UI、アイコン、操作性を継続的に改善中。
- **スクリーンショット** — 一部実装済み。既知の不具合があります。
- **ミュージック** — Windows Media Center に着想を得たアプリ。
- **パスワード** — 現時点では UI のみ。
- **AirDrop** — [owly](https://github.com/t4t5/owly) を基にした連携。

現在も開発中であり、一部のコンポーネントは未完成です。

## リポジトリ構成

| ディレクトリ | 内容 |
| --- | --- |
| `apps/` | デスクトップアプリとエントリーポイント |
| `crates/` | 共通コンポーネントとシステムサービス |
| `plugins/` | Hyprland のグローバルメニューと Stage Manager 連携 |
| `assets/` | アイコン、画像などの静的リソース |
| `config/` | デスクトップと Hyprland の既定設定 |
| `packaging/` | デスクトップエントリーとサービスのテンプレート |
| `vendor/` | 修正を加えたサードパーティのソースコード |

## 開発

対象は Hyprland を使用する Linux です。Rust 2024 edition を使用しています。ビルドには対応する Rust ツールチェーンと、GPUI およびシステム連携に必要なネイティブライブラリが必要です。

```sh
cargo build --workspace --locked
cargo run -p finder
cargo run -p macstill-info -- --about
```

詳細は [Finder のドキュメント](apps/finder/README.md)と[システム情報のドキュメント](apps/info/README.md)を参照してください。Hyprland の設定例は `config/`、パッケージ用テンプレートは `packaging/` にあります。

## コントリビューション

Issue や Pull Request を歓迎します。不具合の報告には再現手順と実行環境の情報を添えてください。

## 謝辞とライセンス

GPUI、Hyprland、[owly](https://github.com/t4t5/owly) をはじめとするオープンソースコミュニティに感謝します。

Macstill のソースコードは [MIT ライセンス](LICENSE)で公開されています。サードパーティのコードや画像にはそれぞれのライセンスが適用されます。ソースコードのライセンスは参考画像の再ライセンスを行うものではありません。
