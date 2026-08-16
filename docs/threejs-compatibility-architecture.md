# Three.js互換ネイティブ実行アーキテクチャ

## 目標

HyperThree Nativeの目標は、Three.jsのシーン記述・マテリアル・アニメーション・
エフェクトを別APIへ書き換えさせることではない。Three.jsをAIが生成しやすい開発
言語として維持し、ブラウザのDOM、WebGLコンテキスト、ブラウザ内GPUプロセスを
取り除いたネイティブホストで実行することである。

したがって、`syncThreeScene()`のように完成済みのThree.jsオブジェクトを単純な
キューブ一覧へ変換する方式は最終アーキテクチャではない。この方式は検証用の
移行レイヤーとして残すが、標準Three.jsレンダラー互換の下位層へ置き換える。

## 互換性の境界

```text
Three.js / TSL / ゲームコード
        |
        |  標準WebGPUオブジェクトとRenderer契約
        v
Embedded JS runtime (V8目標)
        |
        |  WebGPU IDL binding: GPUDevice, GPUBuffer, GPUTexture,
        |  GPURenderPipeline, GPUBindGroup, CommandEncoder
        v
Native render graph
        |
        |  wgpu native backend / Metal / DX12 / Vulkan
        v
Native swapchain
```

Three.jsのコードをそのまま動かすには、ネイティブ側で独自のMesh APIを増やす
だけでは足りない。Three.js WebGPUレンダラーが期待するWebGPUのオブジェクト、
非同期Promise、typed array、GPU resource lifetime、shader module、bind group、
render/compute passを同じ意味で提供する必要がある。

## 現在の移行実装

現時点では、移行レイヤーにも次の情報を持たせている。

- Geometry: position、normal、UV、index
- Material: base color、metalness、roughness、emissive、unlit、texture ID
- Transform: Three.js `matrixWorld` を優先し、アニメーションで変化する行列を保持
- Light: DirectionalLightの方向、色、強度、ambient
- Asset: glTF/GLBの法線、UV、PBR係数、ベースカラーテクスチャ
- Effects: Three.js `Points` をネイティブのカメラ向きビルボード粒子へ変換

これらはネイティブの直接光PBRパスと粒子パスまで接続済みである。従来の位置・色だけの
簡略化から、アニメーション変換を失わない汎用描画契約へ移行する最初の縦切り
になっている。winitのキーボード、マウス、pointer、wheel、touchイベントは
ブラウザ互換の`window`/canvasイベントへ変換し、Three.js Controlsを標準の
イベント購読で動かせる境界も追加している。`requestPointerLock()`、
`document.exitPointerLock()`、`document.pointerLockElement`もwinitのcursor grabと
ネイティブカーソル可視性へ接続し、`pointerlockchange`を配送する。

## 実装フェーズ

### Phase A: 標準WebGPU binding

V8のIsolateをホストへ組み込み、Three.js WebGPUレンダラーが使用する最小の
WebGPU IDLを実装する。各GPUオブジェクトはRust側の世代付きハンドルで管理し、
JS GCとは独立して参照数と破棄を管理する。

現在はこのフェーズの初期段階として、Boa上の`navigator.gpu`、adapter/device、
`GPUBuffer`、`GPUTexture`、`GPUShaderModule`、Queue uploadに加え、BindGroup、
Pipeline、CommandEncoder、RenderPass、ComputePassのnative実行sliceを実装済みで
ある。パイプラインからの`getBindGroupLayout()`、バッファ／テクスチャのcopy command、
テクスチャ配列・mip・sample・view descriptor、typed upload layoutにも対応した。初期の
`GPUCanvasContext.configure()`、`getCurrentTexture()`、native
swapchainへのpresentまで接続済みである。BoaのThree.js node-cacheが一時的に生成する
`null`/`undefined`キーをキャッシュミスとして扱い、空のChainMapキー参照を
安全なミスへ正規化する互換層も追加し、Three.js 0.179の
`WebGPURenderer.renderAsync()`をMeshStandardMaterial、DirectionalLight、Pointsを
含むシーンでApple M4/Metal上に接続できることを確認した。さらにAnimationMixerで
変化するTransform、morph target、SkinnedMesh/Skeletonのbone transformを同じ標準Renderer
経路で検証し、Three.jsが生成するtexture-array `textureLoad`のLOD型を現行Naga向けに
正規化した。さらにプロジェクト相対`fetch()`、`Request`、`Response`、`Headers`、
`Blob`、`ArrayBuffer`、`TextDecoder`、`createImageBitmap`の互換境界を追加し、GLB／glTFを標準Loaderへ渡す入口を
用意した。`navigator.userAgent`、`console`、data URL、PNG decode、
`GPUQueue.copyExternalImageToTexture`、GPUBuffer readbackも含め、Three.js 0.179の
標準`GLTFLoader.loadAsync()`でembedded bufferを読み込み、SkinnedMesh/Skeleton、
AnimationMixer、標準WebGPURenderer描画までfixtureで検証済みである。さらにGLB、外部buffer、
外部PNG、canvas resizeを同じfixtureで検証済みである。Three.js 0.179の
`MeshStandardNodeMaterial`、TSL `colorNode`、
`PostProcessing`、`pass()`、Bloomノードも標準`WebGPURenderer`経路でApple M4/Metal上の
実シーンをスモーク済みである。InstancedMesh、BatchedMesh、Line、Sprite、DirectionalLight
shadow、equirectangular environment、MRT、indirect drawとそのGPU readbackも同じfixtureで
スモーク済みである。`GPURenderBundleEncoder`の記録・finish・`executeBundles()`、
`GPUQuerySet`のocclusion query、`beginOcclusionQuery()`/
`endOcclusionQuery()`、`resolveQuerySet()`とMAP_READ readbackもnative wgpuへ接続した。
`GPUCommandEncoder.clearBuffer()`、`copyBufferToTexture()`、`GPUQueue.writeBuffer()`の
dataOffset/size、`writeTexture()`のdataLayout.offsetもnative wgpuコマンドへ接続した。
Adapter/Deviceの実GPU limitsを`GPUSupportedLimits`互換のcamelCaseで公開し、
`GPUQueue.onSubmittedWorkDone()`はwgpuの`Maintain::Wait`で実際のsubmit完了を待つ。
storage bufferをComputePassで更新し、別のGPUBufferへcopyしてMAP_READするnative
dispatch経路もfixtureで検証している。
標準のr/rg/rgba integer・float、HDR packed、depth/stencil texture formatをwgpuへ
変換し、adapterが提供するdepth clip、depth32float-stencil8、indirect-first-instance、
HDR renderable、BGRA storage、float32 filterable featureだけをDevice/JSへ公開する。
Three.js共通バックエンドがRenderBundleEncoderへ記録時に呼ぶviewport/scissor/blend/stencil
setterは、WebGPUの仕様上RenderPass側の状態が権威となるため、実行時にRenderBundleへ
誤って状態を持ち込まない互換no-opとして公開している。
`GPUCanvasContext.configure()`/`unconfigure()`、canvas surface textureの
寿命管理、Lost/Outdated時のnative再configureも標準Renderer経路へ接続した。ネイティブsurfaceが
opaque合成しか公開しない場合は、Three.jsのpremultiplied要求をopaqueへフォールバックする。
この場合の透明canvas合成はまだブラウザとピクセル同値ではない。`GPUDevice.lost`はnative wgpu device-lost callbackからreason/messageを
Promiseへ配送し、error scopeはnative `push_error_scope`/`pop_error_scope`へ接続した。
ホストは同じloss recordをフレーム境界で検出し、無効化されたGPUへ追加submitせず安全に
停止する。`tests/device-loss-restart-smoke.js`では同じWindow上でnative device、Renderer、
JS sessionを再生成してentry pointを再実行するところまで検証済みである。ゲームJSのヒープ状態
は再初期化され、アプリ固有の永続状態は次のセーブ／復元層で扱う。
constructor内lexical bindingを`var`へ限定正規化する
Boa 0.21.1互換層と、JS評価panicをエラーへ変換する保護も追加した。次は透明合成を含む
present/device-loss lifecycle、timestamp queryの実GPU検証、DRACOの追加属性、未実装の標準WebGPU APIを
段階的に埋める。

ネイティブ`AssetStore`のglTF経路には`EXT_meshopt_compression`の属性、三角形インデックス、
インデックスシーケンスの3モード展開も接続し、NONE/OCTAHEDRAL/QUATERNION/
EXPONENTIALフィルタを含む圧縮・復元ラウンドトリップをテスト済みである。圧縮ビューを
使わない資産は従来のgltf-rs readerへフォールバックするため、sparse accessorなどの
既存挙動を不用意に狭めない。なお、これはネイティブAssetStore経路の対応であり、
Three.js側の`GLTFLoader`へnative MeshoptDecoderを自動注入し、属性・三角形インデックスの
圧縮glTFを標準`loadAsync()`で読み込むfixtureまで検証済みである。さらに標準
`KTX2Loader`を`GLTFLoader.setKTX2Loader()`へ接続し、`KHR_texture_basisu`のraw BC1 KTX2を
GPU圧縮テクスチャとして読み込むfixtureも検証済みである。さらにnative `basisu` transcoderを
`KTX2Loader`のworker境界へ差し込み、raw KTX2のmip/face payloadと、BasisLZ/UASTCを
ASTC/BC7/BC3/BC1/ETC2/RGBA32へ実行時選択できるようにした。BasisLZの8×8実ファイル
fixtureとraw BC1 fixtureは標準GLTFLoader経由で検証済みである。さらに4×4 raw UASTC fixtureを
RGBA32/BC7へ変換するbinding testと、標準GLTFLoader経由のend-to-end fixtureを追加した。

### Draco

`DRACOLoader`のworker decode境界には`__hyperthreeDecodeDraco`を注入する。Rust側は
`draco-oxide-decoder`で三角形meshを復号し、POSITION/NORMAL/TANGENT/TEXCOORD/COLOR/skin
属性とindexをThree.jsの`BufferGeometry`形状へ戻す。Khronos公式Boxの
`KHR_draco_mesh_compression` glTFを、標準GLTFLoader/DRACOLoader経由でnative decodeする
fixtureを検証済みである。UASTCのGPUターゲット別fixture、Dracoのpoint cloud/standalone API、
および全属性・morph/animationの網羅は継続課題である。

WebGPU側は、実GPUが提供するBC/ETC2/ASTC圧縮機能を`GPUAdapter.features`/`GPUDevice.features`
へ公開し、Three.jsの圧縮テクスチャ経路が使う形式名をwgpuへ変換する。`queue.writeTexture()`
は圧縮ブロックのbytesPerRowとmipLevelを保持してネイティブへ渡す。raw KTX2はnative
bridgeがKTX2Loader互換のlevel/face配列を作り、`CompressedTexture`のBC1データを
ネイティブGPUへ渡せる。
BasisLZ/UASTCのKTX2コンテナはnative transcoderが全mip/face/layerを展開し、
raw KTX2とBasisLZ/UASTCの結果をworker互換の`faces[].mipmaps[]`としてThree.jsへ返す。
BasisLZの実ファイルfixtureと、UASTCのRGBA32/BC7 target fixtureは検証済みである。
ASTC/BC3/BC1/ETC2の実GPU別target matrixは次段階である。

### Phase B: Three.js renderer実行

`navigator.gpu.requestAdapter()`、`requestDevice()`、canvas context、WGSL、
uniform/storage buffer、sampler、texture、render pass、compute pass、
indirect drawを実装する。`syncThreeScene()`は互換診断用へ縮小し、通常のゲーム
エントリはThree.jsのRendererを直接使用する。

### Phase C: 完全な資産・アニメーション

glTFの複数primitive、material texture set、KTX2/Basis、skin、morph target、
animation clipを、JSヒープを経由せずネイティブ側でストリーミングする。Three.js
のAnimationMixerが更新した行列も、GPU skinning用のstorage bufferへ接続する。

### Phase D: エフェクトとGPU-driven rendering

TSL/NodeMaterialのshader graph、custom particle、instancing、post-processing、shadow、
environment lighting、compute culling、indirect drawを同じWebGPU契約で実行する。
NodeMaterialとpost-processing、画像／環境テクスチャ、MRT、shadow、indirect drawの
初期経路はスモーク済みだが、公式サンプル全体と高度な環境／shadow／GPU-driven経路は
まだ互換性テストを追加する段階である。

## 互換性の判定基準

「Three.js対応」と呼ぶ条件を、キューブが表示できることにはしない。最低限、
Three.js公式のWebGPUサンプルを次のカテゴリで実行する。

1. Standard / Physical material、texture、normal、roughness、metalness
2. GLTFLoader、AnimationMixer、skin、morph target（embedded/external/GLBとPNG textureのsmoke済み）
3. InstancedMesh、BatchedMesh、Points、Line、Sprite（InstancedMesh/Line/Sprite smoke済み）
4. TSL/NodeMaterial、compute、post-processing（TSL/compute/post-processing smoke済み）
5. Shadow、environment map、multiple render target、indirect draw（fixture smoke済み）

各カテゴリにブラウザ版との画像比較、GPU validation、メモリ使用量、フレーム時間
のテストを用意し、未対応APIをゲーム起動後に黙って簡略化しない。未対応の場合は
起動時に機能名と代替策を明示する。
