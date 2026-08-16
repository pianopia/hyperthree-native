# HyperThree Native

仕様書「Three.js 超高速ネイティブ実行・大容量メモリ解放アーキテクチャ」
をもとにした、ブラウザを使わずに JavaScript とネイティブ GPU を起動する
最小の実行プロトタイプです。

## 開発環境を作る

AIでゲームを作る場合は、まずプロジェクト雛形を生成します。

```bash
cargo run --manifest-path <repo-root>/Cargo.toml -- \
  init <game-project>
cd <game-project>
npm install
```

AIには `src/game.js` と `hyperthree.toml` を編集させます。Three.jsの依存を
含むIIFEバンドルを作成し、ネイティブホストへ渡せます。

```bash
cargo run --manifest-path <repo-root>/Cargo.toml -- \
  build --project <game-project>
cargo run --manifest-path <repo-root>/Cargo.toml -- \
  run --project <game-project>
```

## 単体ホスト起動

```bash
cargo run --manifest-path <repo-root>/Cargo.toml
```

起動すると `js/game.js` を評価したあと、ネイティブウィンドウを開きます。
wgpu の直接描画で三角形が表示されれば、イベントループ・GPUサーフェス・描画
パイプラインが接続されています。

ゼロコピー読み込みの確認には、任意のバイナリを渡します。

```bash
cargo run --manifest-path <repo-root>/Cargo.toml -- \
  --asset /path/to/asset.glb
```

## 構成

- `src/main.rs`: ネイティブホストとイベントループ
- `src/js_runtime.rs`: ブラウザ非依存 JavaScript 実行境界
- `src/renderer.rs`: wgpu のネイティブサーフェスと描画パイプライン
- `src/asset.rs`: mmap ベースのアセット読み込み
- `src/bridge.rs`: JavaScriptからネイティブ描画状態へ渡す共有ブリッジ
- `src/project.rs`: `hyperthree.toml`、雛形生成、npmビルド導線
- `js/`: Three.js 互換層へ接続するゲームエントリーポイント
- `docs/architecture.md`: 仕様書の各項目と実装状況
- `docs/platform-support.md`: macOS / Windows / Linux対応計画
- `docs/commerce-connect-plan.md`: Stripe Connect販売・手数料・振込・管理画面計画
- `docs/roadmap.md`: ランタイムとコマースの全体ロードマップ

現在の雛形はThree.jsのシーンをバンドルしてネイティブホストへ渡し、
`HyperThreeNative.setClearColor()` / `setTriangleColor()`で描画状態を変更する
開発導線
までを提供します。任意のブラウザ向けThree.jsゲームをそのまま動かすには、
DOM/WebGPU APIバインディング、入力・音声、GPU Driven Culling、Indirect Draw、
glTF/KTX2の直接VRAM転送を引き続き実装する必要があります。
