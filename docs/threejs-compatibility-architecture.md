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

これらはネイティブの直接光PBRパスまで接続済みである。従来の位置・色だけの
簡略化から、アニメーション変換を失わない汎用描画契約へ移行する最初の縦切り
になっている。

## 実装フェーズ

### Phase A: 標準WebGPU binding

V8のIsolateをホストへ組み込み、Three.js WebGPUレンダラーが使用する最小の
WebGPU IDLを実装する。各GPUオブジェクトはRust側の世代付きハンドルで管理し、
JS GCとは独立して参照数と破棄を管理する。

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

TSL/NodeMaterialのshader graph、particle、instancing、post-processing、shadow、
environment lighting、compute culling、indirect drawを同じWebGPU契約で実行する。

## 互換性の判定基準

「Three.js対応」と呼ぶ条件を、キューブが表示できることにはしない。最低限、
Three.js公式のWebGPUサンプルを次のカテゴリで実行する。

1. Standard / Physical material、texture、normal、roughness、metalness
2. GLTFLoader、AnimationMixer、skin、morph target
3. InstancedMesh、BatchedMesh、Points、Line、Sprite
4. TSL/NodeMaterial、compute、post-processing
5. Shadow、environment map、multiple render target、indirect draw

各カテゴリにブラウザ版との画像比較、GPU validation、メモリ使用量、フレーム時間
のテストを用意し、未対応APIをゲーム起動後に黙って簡略化しない。未対応の場合は
起動時に機能名と代替策を明示する。
