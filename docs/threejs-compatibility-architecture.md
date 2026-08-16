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
になっている。

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
`GPUQueue.copyExternalImageToTexture`も含め、Three.js 0.179の
標準`GLTFLoader.loadAsync()`でembedded bufferを読み込み、SkinnedMesh/Skeleton、
AnimationMixer、標準WebGPURenderer描画までfixtureで検証済みである。さらにGLB、外部buffer、
外部PNG、canvas resizeを同じfixtureで検証済みである。Three.js 0.179の
`MeshStandardNodeMaterial`、TSL `colorNode`、
`PostProcessing`、`pass()`、Bloomノードも標準`WebGPURenderer`経路でApple M4/Metal上の
実シーンをスモーク済みである。constructor内lexical bindingを`var`へ限定正規化する
Boa 0.21.1互換層と、JS評価panicをエラーへ変換する保護も追加した。次はdevice-lost／
present lifecycle、KTX2/Basis、DRACO/Meshopt、完全なreadback、未実装の標準WebGPU APIを
段階的に埋める。

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
NodeMaterialとpost-processingの標準経路はスモーク済みだが、公式サンプル全体、画像／
環境テクスチャ、MRT、shadow、indirect drawはまだ互換性テストを追加する段階である。

## 互換性の判定基準

「Three.js対応」と呼ぶ条件を、キューブが表示できることにはしない。最低限、
Three.js公式のWebGPUサンプルを次のカテゴリで実行する。

1. Standard / Physical material、texture、normal、roughness、metalness
2. GLTFLoader、AnimationMixer、skin、morph target（embedded/external/GLBとPNG textureのsmoke済み）
3. InstancedMesh、BatchedMesh、Points、Line、Sprite
4. TSL/NodeMaterial、compute、post-processing
5. Shadow、environment map、multiple render target、indirect draw

各カテゴリにブラウザ版との画像比較、GPU validation、メモリ使用量、フレーム時間
のテストを用意し、未対応APIをゲーム起動後に黙って簡略化しない。未対応の場合は
起動時に機能名と代替策を明示する。
