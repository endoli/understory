# RFC: Imaging IR & Backend Semantics for Understory

**Status:** Draft
**Target:** New crate, `understory_imaging`
**Audience:** Understory / Linebender / Canva collaborators
**Backends:** Vello CPU, Vello Hybrid, Vello Classic, Skia, etc.

---

## 1. Goals

This RFC defines a small but explicit layer between **presentation / display** and **render backends**:

* A **backend-agnostic imaging IR** (paths, paints, images, pictures).
* A **resource model** with stable keys (paths, images, etc.), including pre-uploaded images.
* A **backend trait** that Vello CPU/Hybrid/Classic, Skia, and others can implement.
* Explicit semantics for **caching**, **recordings**, and **transform classes**.
* A design that lets **Parley (text)** emit imaging IR, so renderers don’t need font/glyph logic.

The internal implementation details (wgpu, CPU rasterizer, Skia, etc.) are backend-specific and not visible at this level.

---

## 2. High-Level Architecture

Layers:

1. **Presentation / Display**
   Box trees, layout, styling, motion/timelines, etc.

2. **Imaging IR (this RFC, `understory_imaging`)**

   * Resource keys: paths, images, paints, pictures.
   * Stateless drawing ops: “fill this path with this paint under this transform”.

3. **Backends**

   * Vello CPU / Hybrid / Classic
   * Skia backend
   * Minimal CPU backend for tests
     All implementing the same `ImagingBackend` trait.

4. **Text / Parley**

   * A new `parley_core` produces imaging IR directly (no glyph-run ops).
   * COLRv1 is lowered into the same imaging IR.

---

## 3. Resource Model

The IR uses **opaque handles** for resources; creation & lifetime are managed *outside* the plain‑old‑data (POD) draw ops, but within the same crate.

### 3.1 Resource Keys

```rust
#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PathId(pub u32);

#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImageId(pub u32);

#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PaintId(pub u32);

#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PictureId(pub u32); // nested imaging program (for COLRv1, icons, etc.)
```

**Design goals:**

* Small (u32) keys that are cheap to store in IR and caches.
* Backends may map them to GPU handles, CPU structs, etc.
* The IR is fully plain‑old‑data (POD); lifetimes are handled by a **resource context**.

### 3.2 Resource Descriptors

Example shapes of descriptors (exact details can evolve):

```rust
pub struct PathDesc {
    pub commands: Box<[PathCmd]>,  // move_to/line_to/curve_to/close
}

pub struct ImageDesc {
    pub width: u32,
    pub height: u32,
    pub format: ImageFormat,       // RGBA8, BGRA8, etc.
}

pub struct PaintDesc {
    pub kind: PaintKind,           // solid, gradient, pattern, etc.
}

pub struct PictureDesc {
    pub recording: RecordedOps,    // nested imaging program + acceleration
}
```

### 3.3 Resource Management Interface

A separate trait for resource lifetime, implemented per backend:

```rust
pub trait ResourceBackend {
    // Paths
    fn create_path(&mut self, desc: PathDesc) -> PathId;
    fn destroy_path(&mut self, id: PathId);

    // Images
    fn create_image(&mut self, desc: ImageDesc, pixels: &[u8]) -> ImageId;
    fn update_image(&mut self, id: ImageId, rect: ImageUpdateRegion, pixels: &[u8]);
    fn destroy_image(&mut self, id: ImageId);

    // Paints
    fn create_paint(&mut self, desc: PaintDesc) -> PaintId;
    fn destroy_paint(&mut self, id: PaintId);

    // Pictures (sub-programs, e.g. COLRv1 glyphs, icons)
    fn create_picture(&mut self, desc: PictureDesc) -> PictureId;
    fn destroy_picture(&mut self, id: PictureId);
}
```

This lets callers:

* **Upload images in advance** and get an `ImageId`.
* Reuse the same image across frames.
* Treat COLRv1 glyphs and icons as **pictures** (nested IR).

---

## 4. Imaging IR: Ops

The IR is intentionally **font-agnostic**. Text shaping happens *before* we hit this layer.

### 4.1 StateOps

```rust
pub enum StateOp {
    SetTransform(Affine),
    PushClip(ClipOp),
    PopClip,
    SetPaint(PaintId),
    SetStroke(StrokeStyle),
    SetBlendMode(BlendMode),
    SetOpacity(f32),
}
```

StateOps:

* Mutate rendering state.
* Do *not* produce pixels.
* Are meant to be recorded verbatim.

### 4.2 DrawOps

No glyph runs, no font knowledge. Just primitives:

```rust
pub enum DrawOp {
    FillPath(PathId),
    StrokePath(PathId),
    DrawImage {
        image: ImageId,
        transform: Affine,
        sampling: ImageSampling,
    },
    DrawPicture {
        picture: PictureId,
        transform: Affine,
    },
}
```

Filters are expressed as layer effects (see `LayerOp::filter: Option<FilterDesc>`), not as a draw op.

> **Text**: `parley_core` (or equivalent) consumes text + fonts and emits one or more sequences of:
>
> * `SetPaint`, `SetTransform`
> * `FillPath(PathId)` / `DrawPicture(PictureId)`
>   For outline fonts, it will typically emit paths.
>   For COLRv1, it will generate a `PictureId` describing the glyph as a mini-imaging program.

---

## 5. Transform Classes & Caching

Backends differ in how far they can reuse cached geometry or rasterization.
We make this explicit via **TransformClass**:

```rust
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TransformClass {
    Exact,          // valid only if the current transform matrix (CTM) is exactly identical
    TranslateOnly,  // valid if transforms differ by pure translation
    Orthonormal,    // valid under rotation/reflection (no shear, uniform scale)
    Affine,         // valid under any affine transform
}
```

Any cache entry (path raster, picture raster, etc.) must carry:

* `original_ctm: Affine`
* `valid_under: TransformClass`

A caller may reuse the cache only if the transform difference fits within `valid_under`.

### 5.1 Backend Cache Policies

```rust
pub struct PathCachePolicy {
    pub min_transform_class: TransformClass,
    pub tolerance_px: f32,
    pub supports_sparse_strips: bool,
    pub supports_stroke_expansion: bool,
}

pub struct PictureCachePolicy {
    pub min_transform_class: TransformClass,
}

pub struct FilterCachePolicy {
    pub min_transform_class: TransformClass,
    pub tile_expansion: TileExpansionRules,
}
```

No `GlyphCachePolicy` here — glyphs and fonts are the responsibility of Parley / text layer, not the imaging backend.

---

## 6. Backend Semantics & Trait

### 6.1 BackendSemantics

```rust
pub struct BackendSemantics {
    pub path_cache: PathCachePolicy,
    pub picture_cache: PictureCachePolicy,
    pub filter_cache: FilterCachePolicy,

    pub supports_recording: bool,
    pub supports_pass_merging: bool,
    pub supports_parallel_raster: bool,
}
```

Examples:

* **Vello CPU**:

  * `path_cache.min_transform_class = Exact` (today)
  * `supports_recording = true/false` (experiment)

* **Vello Hybrid**:

  * may be `TranslateOnly` for some caches
  * `supports_parallel_raster = true`

* **Skia backend**:

  * might declare `Orthonormal` for some caches.

### 6.2 ImagingBackend Trait

```rust
pub trait ImagingBackend: ResourceBackend {
    fn semantics(&self) -> &BackendSemantics;

    // --- State & draw ---
    fn state(&mut self, op: StateOp);
    fn draw(&mut self, op: DrawOp);

    // --- Structural boundaries ---
    fn begin_effect(&mut self, ef: EffectBoundary);
    fn end_effect(&mut self);

    fn begin_pass(&mut self, pass: PassDesc);
    fn end_pass(&mut self);

    // --- Recordings ---
    fn begin_record(&mut self) -> RecordToken;
    fn end_record(&mut self, token: RecordToken) -> RecordedOps;

    // --- Sync ---
    fn sync(&mut self, sp: SyncPoint);
}
```

* Backends can be Vello CPU/Hybrid/Classic, Skia, or others.
* The trait doesn’t care if they're based on wgpu, Metal, CPU scanline, etc.

---

## 7. Recordings

Recordings let us reuse backend-specific prepared work while staying portable.

```rust
pub struct RecordedOps {
    pub ops: std::sync::Arc<[ImagingOp]>,   // plain‑old‑data (POD) IR slice
    pub acceleration: Option<Box<dyn std::any::Any>>,
    pub valid_under: TransformClass,
    pub original_ctm: Option<Affine>,
}
```

**Contract:**

* `ops` must always be present and sufficient to replay the recording.
* `acceleration` is optional backend-specific data (e.g., GPU command buffer, sparse-strip representation).
* `valid_under` states the transform-class under which this recording remains valid.

This allows:

* cross-backend debugging (ignore acceleration, replay IR),
* serialization of IR-level recordings,
* backend-specific performance wins.

### 7.1 Design Note — Resource Environment Bound Recordings (v1)

In the current Imaging IR design, all drawing operations reference **external resources** by handle:

* `PathId`
* `ImageId`
* `PaintId`
* `PictureId`

These IDs refer to objects created through the backend’s `ResourceBackend` trait.
A **Recording** therefore inherently depends on the *same* resource environment being present when it is replayed.

**Implications**

* Recordings are **not portable** across backend instances.
* Recordings cannot embed literal resource data (e.g. inline `PathDesc` or image bytes).
* A recording **cannot reconstruct its dependencies** unless the caller creates the same resources with the same IDs beforehand.
* This model keeps v1 simple and efficient, but intentionally scoped.

**One-Off Paths**

Because all resources must be created in the external resource environment:

* One-off paths must be created (and often destroyed) explicitly as resources.
* There is currently no concept of inline paths/images inside a recording.

This is an acknowledged friction point.

**Future Directions (Not in v1)**

The v1 model is deliberately simple. Extensions we may add later include:

1. Inline or Mixed Resource References

```rust
enum PathRef {
    Id(PathId),
    Inline(PathDesc),
}
```

2. Recording-Local Resource Tables
Recordings could include a mini resource dictionary that allows replay in fresh backends.

3. Ephemeral Resource Arenas
Paths created inside a recording could live in a scoped arena that automatically cleans up afterward.

These options give us escape hatches without complicating v1.

**Rationale for v1 Choice**

* Keeps implementation straightforward
* Allows fast prototyping of backends (Vello CPU/Hybrid/Classic, Skia, etc.)
* Matches existing renderer architectures
* Helps us validate the Imaging IR structure before designing portability layers
* Does not preclude future improvements

**Summary**

* **Recordings are environment-bound in v1.**
* **Resources must be pre-created**, even for one-off paths.
* **Portability and inline resources are future extensions.**
* The current model is intentionally simple to enable rapid backend prototyping.

---

## 8. Translation-Stable Path & Picture Reuse

A key performance scenario is **scrolling** and other translation-heavy motions.

A cached rasterization (for a path or picture) is reusable when:

1. `cache_entry.valid_under >= TranslateOnly`.
2. The new current transform matrix (CTM) is the old CTM multiplied by a pure translation.
3. The paint / stroke / filter parameters referenced by the IR are unchanged (or are themselves stable according to backend policy).

### 8.1 Sparse-strips type backends (e.g., Canva’s renderer, future Vello variants)

* Natural fit for translation-stable reuse:

  * underlying representation is strips in local space,
  * translation is just an offset applied at draw time.

### 8.2 GPU vector backends (e.g., Vello Classic / Hybrid)

Possible strategies:

* Raster into an atlas once, then reuse via translated quads.
* Store path geometry and reissue draws with adjusted vertex constants.
* Warp-based path shifting at the shader level.

The backend advertises what it actually supports via `PathCachePolicy` and `PictureCachePolicy`.

---

## 9. Text, Parley, and COLRv1

This design assumes a **font- and glyph-free renderer**:

* The imaging layer has **no `DrawGlyphRun` op**.
* Fonts, glyph shaping, and glyph caches are owned by **Parley / text engine**.
* COLRv1 is treated as a **mini imaging program** that lowers naturally to `PictureId`.

### 9.1 New `parley_core` Role

* Input: text, fonts, directionality, shaping options.
* Output: a sequence of resource creations + imaging ops:

  * `create_path` / `create_picture` calls for glyph outlines or COLRv1 glyphs.
  * `SetTransform`, `SetPaint`, `FillPath` / `DrawPicture` sequences.

**Important:**
Backends see only paths, paints, images, pictures.
They do not need to know:

* glyph IDs,
* font metrics,
* hinting,
* OpenType features.

### 9.2 COLRv1 as Picture

COLRv1 glyphs:

* Are interpreted by Parley (or a dedicated COLRv1 subsystem).
* Lower into `PictureDesc` (a nested imaging IR program).
* Create a `PictureId` via `create_picture`.
* Text drawing then uses `DrawPicture { picture: glyph_picture_id, transform }`.

This matches how COLRv1 is conceptually “a little imaging program”.

---

## 10. Integration & Experiment Plan

### Phase 0: RFC & prototype crate

* Add `understory_imaging` as an experimental crate.
* Implement:

  * resource keys,
  * ops,
  * `ImagingBackend` skeleton,
  * basic CPU test backend.

### Phase 1: Adapt a real backend

* Implement `ImagingBackend` for:

  * Vello CPU **or**
  * Vello Classic (whichever is easier to spike).

### Phase 2: Hook Understory display

* Add a lowering path from `understory_display` → `understory_imaging` IR.
* Keep the existing direct-to-Vello path for comparison.

### Phase 3: Parley integration

* Prototype a new `parley_core` that emits imaging IR:

  * no glyph-run ops,
  * simple mapping from text → paths/pictures/paints.

---

## 11. Open Questions

1. **Resource lifetime model:**

   * Do we want explicit “frames” / arenas?
   * Or rely purely on RAII-style creation/destruction?

2. **Binary format:**

   * Should recordings + PictureDescs have a standard binary serialization for caching / disk?

3. **Filters and effects:**

   * How deep do we want the filter graph in this crate, vs. deferring to backends?

4. **Error reporting & diagnostics:**

   * Do we need a structured error/diagnostic type at this layer?

5. **Stroke representation between display and imaging:**

   * Today, `understory_display::Op::StrokePath` carries an abstract stroke id, while `understory_imaging::StateOp::SetStroke` expects a concrete `kurbo::Stroke`.
   * We have not yet decided whether to:
     * inline `kurbo::Stroke` directly in display ops,
     * use a `StrokeId`→`Stroke` resolver in the display→imaging lowering crate, or
     * promote strokes to first-class imaging resources.
   * For v1, it is acceptable for display→imaging lowering to ignore `StrokePath` ops until that decision is made.
