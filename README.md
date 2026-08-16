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

GPUドライバとOS別バックエンドの確認には、ウィンドウを開かない診断コマンド
を使います。

```bash
cargo run --manifest-path <repo-root>/Cargo.toml -- diagnostics
```

起動すると `js/game.js` を評価したあと、ネイティブウィンドウを開きます。
wgpu の直接描画でキューブが表示されれば、イベントループ・GPUサーフェス・描画
パイプラインが接続されています。

ゼロコピー読み込みの確認には、任意のバイナリを渡します。

```bash
cargo run --manifest-path <repo-root>/Cargo.toml -- \
  --asset /path/to/asset.glb
```

ゲームコードからは、プロジェクトルート配下のアセットをネイティブ側で
memory-mapできます。

```js
const model = HyperThreeNative.loadAsset("public/models/player.glb");
console.log(model.byteLength, model.meshCount, model.primitiveCount);

HyperThreeNative.drawAsset("public/models/player.glb", 0, 0, {
  x: 0, y: 0, z: 0, r: 0.8, g: 0.9, b: 1.0,
});
```

`drawAsset()`はglTF/GLBの指定プリミティブをネイティブ側でデコードし、
POSITION/index/UVバッファと対応するglTFベースカラーテクスチャ（RGBA8）を
GPUへ登録します。Three.jsの任意の位置属性を持つ
`BufferGeometry`は、`syncThreeScene(scene, camera)`がgeometry ID単位で登録・
キャッシュして描画します。

`src/game.mjs`などのES moduleエントリも、相対importとプロジェクトの
`node_modules`パッケージ解決に対応しています。IIFEへバンドルする既存の
Vite導線も引き続き利用できます。実行時には`performance.now()`、
`requestAnimationFrame()`、`cancelAnimationFrame()`、`window`、`self`、`global`
の基本互換もネイティブフレームループへ接続されます。

標準Loaderの縦切りは
[`tests/fixtures/threejs-gltf-loader-smoke`](tests/fixtures/threejs-gltf-loader-smoke/README.md)
で再現できます。Three.jsの`GLTFLoader.loadAsync()`がembedded bufferから
SkinnedMesh/SkeletonとAnimationMixerを生成し、標準WebGPURendererで描画します。
同fixtureでは、公開`AudioLoader`、ネイティブ`AudioContext`、Three.jsの
`AudioListener`/`Audio`を使ったWAVデコード経路も検証します。Three.js標準API全体の
互換性はまだ完了しておらず、未実装APIは段階的に追加します。

ゲームのセーブデータには、プロジェクト内の`.hyperthree/storage`へ保存される
`localStorage`と、セッション限定の`sessionStorage`を利用できます。
バイナリのセーブデータやAI生成アセットには、origin-privateなFile System Access
経路も利用できます。

```js
const root = await navigator.storage.getDirectory();
const saves = await root.getDirectoryHandle("saves", { create: true });
const file = await saves.getFileHandle("slot.bin", { create: true });
const writable = await file.createWritable();
await writable.write(new Uint8Array([1, 2, 3]));
await writable.close();
```

## 構成

- `src/main.rs`: ネイティブホストとイベントループ
- `src/js_runtime.rs`: ブラウザ非依存 JavaScript 実行境界
- `src/renderer.rs`: wgpu のネイティブサーフェスと描画パイプライン
- `src/asset.rs`: mmap ベースのアセット読み込み
- `src/bridge.rs`: JavaScriptからネイティブ描画状態へ渡す共有ブリッジ
- `src/platform.rs`: OS・GPUバックエンド診断
- `src/project.rs`: `hyperthree.toml`、雛形生成、npmビルド導線
- `src/storage.rs`: プロジェクトsandboxのlocalStorage永続化
- `js/`: Three.js 互換層へ接続するゲームエントリーポイント
- `docs/architecture.md`: 仕様書の各項目と実装状況
- `docs/platform-support.md`: macOS / Windows / Linux対応計画
- `docs/commerce-connect-plan.md`: Stripe Connect販売・手数料・振込・管理画面計画
- `docs/roadmap.md`: ランタイムとコマースの全体ロードマップ
- `.github/workflows/ci.yml`: macOS / Windows / LinuxのCI

現在の雛形はThree.jsのシーンをバンドルしてネイティブホストへ渡し、
`HyperThreeNative.setClearColor()`、`setCamera()`、`beginFrame()`、`pushCube()`で
描画状態を更新し、`HyperThreeGame.update(deltaSeconds)`で毎フレーム処理できる
開発導線までを提供します。`syncThreeScene(scene, camera)`は
`BoxGeometry`、`PlaneGeometry`、`SphereGeometry`と任意の位置/index/UV
`BufferGeometry`をネイティブ描画へ同期します。任意のブラウザ向けThree.js
ゲームをそのまま動かすには、完全なテクスチャ・マテリアル、DOM/WebGPU API
バインディング、入力・音声の全API、GPU Driven Cullingの大規模シーン対応、glTF/KTX2の
完全な直接VRAM転送を引き続き実装する必要があります。ComputePassで可視性フラグから
Indirect引数を生成して描画するGPUカリングの初期fixtureは検証済みです。現時点のGLTFLoader fixtureは
embedded buffer、GLB、外部buffer、PNG画像テクスチャ、Meshopt圧縮glTF、raw BC1/BasisLZ/UASTC KTX2を
標準`KTX2Loader`/`KHR_texture_basisu`経由で検証済みです。BasisLZ/UASTC向けnative transcoder
binding、raw KTX2のmip/face転送、Khronos公式Boxを使った標準`GLTFLoader`/`DRACOLoader`
経由のnative Draco decodeまでfixture検証済みです。UASTCはRGBA32/BC7のtarget fixtureまで
検証済みで、ASTC/BC3/BC1/ETC2のGPUバックエンド別matrix、Dracoの属性・point cloud・standalone
API網羅、その他の標準Web API互換性は継続対応します。RGBA画像を使う
`HTMLVideoElement`の連続フレーム境界と、2フレームGIFの遅延時間・`requestVideoFrameCallback()`・
`VideoTexture`利用経路、`TextureLoader`と`CubeTextureLoader`の画像／キューブ環境テクスチャ経路も
同じThree.js WebGPU fixtureで検証済みです。`Data3DTexture`をTSL `texture3D`でサンプリングする
ボリューム／arrayテクスチャ経路、GIF動画の`currentTime`／`fastSeek()`シークも検証済みです。
H.264/VP9/AV1等の動画codec、
動画音声トラック、ランダムシーク、OS hardware decoder接続は未実装の残タスクです。

Three.js互換を標準WebGPUレンダラーまで拡張する再設計と段階計画は
[`docs/threejs-compatibility-architecture.md`](docs/threejs-compatibility-architecture.md)
にまとめています。現在は移行第一段階として、法線、PBR係数、DirectionalLight、
`matrixWorld`をネイティブ直接光PBRパスへ接続しています。
`THREE.Points`もカメラ向きビルボード粒子としてネイティブ描画へ接続しています。

入力は`isKeyDown()`、`isMouseButtonDown()`、`getMousePosition()`から取得でき、
PerspectiveCameraとOrthographicCameraの両方を同期できます。
