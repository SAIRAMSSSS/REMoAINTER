//! SBS-style parametric graph engine — the authored value-stream analog of
//! reference PBR suite's `DataGraph_` (see `MULTI_MATERIAL_SUBSTACK_HEIGHT_BLENDING.md`
//! and the reference Composer model).
//!
//! A [`Graph`] is a small DAG of typed nodes. Generator nodes synthesize RGBA
//! buffers from parameters; filter nodes consume upstream buffers through
//! ordered input slots; links are stored on the consumer node's slots so the
//! whole graph serializes position-independently (values only, no UI layout).
//! [`GraphEngine`] evaluates the DAG topologically at a given resolution,
//! memoizes per-node results keyed by `(node, output, width, height)`, reports
//! cycles as a diagnostic [`GraphError::CycleDetected`], and exposes per-node
//! dirty invalidation for interactive UI re-evaluation.
//!
//! Buffers reuse the painting modules' image format: `Vec<[f32; 4]>` RGBA in
//! the 0..1 range, row-major, matching `layer.rs` / `filter.rs`.
//!
//! Sharp edges to keep in mind:
//! - All evaluation is `Result`-based; user input never panics.
//! - Node seeds mix with [`GraphContext::seed`], so whole-document reseeding is
//!   one field write, and per-node results stay bit-deterministic (per-pixel
//!   independent math, no scheduling-dependent accumulation).
#![allow(dead_code)]

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::sync::Arc;

use super::filter::{apply_levels_scalar, gaussian_blur_rgba};
use super::layer::{blend_colors, BlendMode};

/// RGBA image buffer, row-major — the same layout `layer.rs` and `filter.rs`
/// use for layer color buffers.
pub type ImageRgba = Vec<[f32; 4]>;

/// Neutral slot default: 50% gray, fully opaque.
pub const GRAY: [f32; 4] = [0.5, 0.5, 0.5, 1.0];
/// White slot default.
pub const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
/// Black slot default.
pub const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
/// Fully transparent slot default.
pub const TRANSPARENT: [f32; 4] = [0.0, 0.0, 0.0, 0.0];

/// Hard cap on total pixels of a single evaluation, so a stale UI resolution
/// slider cannot OOM the process.
const MAX_PIXELS: u64 = 8192 * 8192;
/// Hard cap on FBM octaves.
const MAX_OCTAVES: u32 = 12;

// ─────────────────────────────────────────────────────────────────────────────
// Identifiers
// ─────────────────────────────────────────────────────────────────────────────

/// Stable identifier of a node inside one graph. Serialized as a plain `u32`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub struct GraphNodeId(pub u32);

/// Identifier of a node output slot (`u32` node id + `u32` output index).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct OutputRef(pub GraphNodeId, pub u32);

/// One directed edge of the graph, materialized for inspection and UI wiring.
/// Edges are stored on the consumer node's slot, so this struct is a view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphEdge {
    /// Source node.
    pub from: GraphNodeId,
    /// Consumer node.
    pub to: GraphNodeId,
    /// Input slot index on `to` this edge feeds.
    pub slot: usize,
}

// ─────────────────────────────────────────────────────────────────────────────
// Evaluation context
// ─────────────────────────────────────────────────────────────────────────────

/// An externally supplied sample buffer (imported texture, baked map).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SampleImage {
    pub width: u32,
    pub height: u32,
    pub data: ImageRgba,
}

impl SampleImage {
    /// Validates that `data` matches the declared size.
    pub fn is_consistent(&self) -> bool {
        self.data.len() == self.width as usize * self.height as usize
    }
}

/// Per-evaluation context carried by the engine: target resolution, the
/// document-wide PRNG seed root, and named external sample buffers that
/// [`GraphNodeBody::ExternalImage`] nodes pull from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphContext {
    /// Default resolution used by [`GraphEngine::render`].
    pub width: u32,
    pub height: u32,
    /// Document seed root; every node seed is XOR-mixed with this.
    pub seed: u32,
    /// Named sample buffers for `ExternalImage` nodes.
    pub samples: BTreeMap<String, SampleImage>,
}

impl GraphContext {
    pub fn new(width: u32, height: u32, seed: u32) -> Self {
        Self {
            width: width.max(1),
            height: height.max(1),
            seed,
            samples: BTreeMap::new(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Slots, nodes, node bodies
// ─────────────────────────────────────────────────────────────────────────────

/// One ordered input slot of a node. `reference` distinguishes connections
/// from constant instance values; we mirror that exactly.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum GraphSlot {
    /// Nothing connected: the node's built-in default is fed in.
    Default,
    /// Linked to the primary output of another node.
    Link(GraphNodeId),
    /// Unlinked but overridden with a constant RGBA value.
    Value([f32; 4]),
}

/// Gradient control point used by [`GraphNodeBody::GradientMap`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    /// Position along the gradient in 0..1.
    pub t: f32,
    /// Color at this stop (RGBA 0..1).
    pub color: [f32; 4],
}

impl GradientStop {
    pub const BLACK_WHITE_START: Self = Self {
        t: 0.0,
        color: BLACK,
    };
    pub const BLACK_WHITE_END: Self = Self {
        t: 1.0,
        color: WHITE,
    };
}

/// Ramp shapes supported by the gradient generator node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GradientKind {
    Linear,
    Radial,
    Angle,
}

/// The full node catalogue. New nodes append variants; `slot_count`,
/// `output_count`, `label` and `eval` must all stay in sync.
///
/// Parameter values are the authored, serializable "instance values" of each
/// node (the SBS `dynamic_value` table analog). Input images always arrive
/// through [`GraphSlot`]s, never through these params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GraphNodeBody {
    /// Solid fill generator.
    ConstantColor { color: [f32; 4] },
    /// Named sample from [`GraphContext::samples`]; resized to the render
    /// resolution with wrapped bilinear sampling when sizes differ.
    ExternalImage { name: String },
    /// Tileable fractal value noise (FBM), plain gray or ramped between two
    /// colors. `scale` is the base lattice period in cells; it doubles per
    /// octave so results stay tiling-safe.
    FbmNoise {
        seed: u32,
        octaves: u32,
        scale: f32,
        gain: f32,
        contrast: f32,
        color_a: [f32; 4],
        color_b: [f32; 4],
    },
    /// Per-cell hashed grain. `cells` is the grain period across the image.
    WhiteNoise {
        seed: u32,
        cells: u32,
        color_a: [f32; 4],
        color_b: [f32; 4],
    },
    /// Black & white spot generator: one jittered feature point per Voronoi
    /// cell, radially blended with a power falloff. `cells` is the grid
    /// period, `radius` the spot size in cell units.
    BnWSpots {
        seed: u32,
        cells: u32,
        radius: f32,
        falloff: f32,
        invert: bool,
    },
    /// Two-color checkerboard. `cells` = squares per row/column.
    Checker {
        cells: u32,
        color_a: [f32; 4],
        color_b: [f32; 4],
    },
    /// Linear / radial / angular ramp.
    Gradient {
        kind: GradientKind,
        angle_deg: f32,
        from: [f32; 4],
        to: [f32; 4],
    },
    /// Running-bond brick wall with mortar, edge bevel and seeded tone jitter.
    /// `mortar` (0..0.5) and `bevel` (0..0.25) are fractions of a brick cell.
    Brick {
        seed: u32,
        cols: u32,
        rows: u32,
        mortar: f32,
        bevel: f32,
        brick_color: [f32; 4],
        mortar_color: [f32; 4],
    },
    /// Veined marble: sine bands warped by FBM turbulence.
    Marble {
        seed: u32,
        veins: f32,
        turbulence: f32,
        octaves: u32,
        color_a: [f32; 4],
        color_b: [f32; 4],
    },
    /// Crystal / Voronoi panels: seeded cell colors darkened near cell edges.
    Crystal {
        seed: u32,
        cells: u32,
        edge_width: f32,
        edge_darkness: f32,
        color_a: [f32; 4],
        color_b: [f32; 4],
    },
    /// Authored soft blob scatter: `count` overlapping radial blobs placed by
    /// the seed, maxed over each other. Tileable via wrapped UV distance.
    BlobScatter {
        seed: u32,
        count: u32,
        size: f32,
        falloff: f32,
        blob_color: [f32; 4],
        bg_color: [f32; 4],
    },
    /// Layer-style blend of `base` (slot 0) and `top` (slot 1).
    Blend { mode: BlendMode, opacity: f32 },
    /// In/out levels with gamma, per channel.
    Levels {
        min_in: f32,
        max_in: f32,
        gamma: f32,
        min_out: f32,
        max_out: f32,
    },
    /// Per-channel RGB inversion; alpha passes through.
    Invert,
    /// Separable gaussian blur (`filter.rs` implementation).
    GaussianBlur { radius: f32 },
    /// Scale about the image centre, rotate, translate. Samples the input
    /// bilinearly with repeat wrapping (seamless-results friendly).
    Transform {
        scale: f32,
        rotation_deg: f32,
        offset_x: f32,
        offset_y: f32,
    },
    /// Repeats the input `cols` x `rows` times with gaps, per-tile jitter and
    /// an optional zoom-in window per tile.
    TileGenerator {
        seed: u32,
        cols: u32,
        rows: u32,
        gap: f32,
        jitter: f32,
        zoom: f32,
        mortar: [f32; 4],
    },
    /// Blur that follows the local gradient of the height input (slot 1):
    /// smears `input` along slope, sharpening creases. Height must be in 0..1.
    SlopeBlur { radius: f32, taps: u32 },
    /// Warps the input along a fixed direction, offset by the height input.
    /// `strength` is the maximum shift in pixels (at full 0->1 height swing).
    DirectionalWarp { angle_deg: f32, strength: f32 },
    /// Colorizes the luminance of the input through `stops`.
    GradientMap { stops: Vec<GradientStop>, mix: f32 },
    /// Graph output: passes its input through unchanged.
    Output,
}

// ─────────────────────────────────────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────────────────────────────────────

/// Evaluation / structural errors. `Clone` + `PartialEq` so results can be
/// cheaply compared; `Send` for cross-thread evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraphError {
    /// Referenced node does not exist.
    MissingNode(GraphNodeId),
    /// Referenced slot index does not exist.
    MissingSlot { node: GraphNodeId, slot: usize },
    /// Structural duplicate id (possible only with hand-crafted graphs).
    DuplicateNode(GraphNodeId),
    /// Slot vector length does not match the node body's arity.
    SlotCount {
        node: GraphNodeId,
        expected: usize,
        actual: usize,
    },
    /// A link points to an output index the source node does not have.
    MissingOutput { node: GraphNodeId, output: u32 },
    /// A link references a node id not present in the graph.
    BadLink { from: GraphNodeId, to: GraphNodeId },
    /// `ExternalImage` references an unknown context sample.
    MissingSample(String),
    /// An input buffer does not match the requested resolution.
    SizeMismatch { expected: usize, actual: usize },
    /// Topological cycle; carries the path taken to the repeated node.
    CycleDetected(Vec<GraphNodeId>),
    /// Structurally fine but semantically invalid parameters.
    InvalidParams(String),
    /// [`Graph::output`] is unset.
    NoOutput,
}

impl fmt::Display for GraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingNode(id) => write!(f, "missing node {:?}", id),
            Self::MissingSlot { node, slot } => write!(f, "node {:?} has no slot {}", node, slot),
            Self::DuplicateNode(id) => write!(f, "duplicate node id {:?}", id),
            Self::SlotCount {
                node,
                expected,
                actual,
            } => write!(
                f,
                "node {:?} expects {} slots, has {}",
                node, expected, actual
            ),
            Self::MissingOutput { node, output } => {
                write!(f, "node {:?} has no output {}", node, output)
            }
            Self::BadLink { from, to } => {
                write!(f, "link from {:?} into slot of {:?} is invalid", from, to)
            }
            Self::MissingSample(name) => write!(f, "external image {:?} not in context", name),
            Self::SizeMismatch { expected, actual } => {
                write!(
                    f,
                    "buffer size mismatch: expected {} px, got {}",
                    expected, actual
                )
            }
            Self::CycleDetected(path) => {
                let ids: Vec<String> = path.iter().map(|i| i.0.to_string()).collect();
                write!(f, "cycle detected: {}", ids.join(" -> "))
            }
            Self::InvalidParams(msg) => write!(f, "invalid parameters: {}", msg),
            Self::NoOutput => write!(f, "graph has no output node set"),
        }
    }
}

impl std::error::Error for GraphError {}

// ─────────────────────────────────────────────────────────────────────────────
// Node trait + metadata
// ─────────────────────────────────────────────────────────────────────────────

/// Uniform vtable over the node catalogue.
pub trait NodeImpl {
    /// Number of output samplers this node produces (v1: always 1).
    fn output_count(&self) -> u8 {
        1
    }
    /// Number of ordered input slots.
    fn slot_count(&self) -> usize;
    /// RGBA constant fed to `slot` when the slot is unset. Defaults to 50% gray.
    fn slot_default(&self, slot: usize) -> [f32; 4] {
        let _ = slot;
        GRAY
    }
    /// Evaluate one node at `(width, height)`. Inputs arrive pre-resolved and
    /// are guaranteed to have `width * height` pixels; implementations return
    /// exactly [`NodeImpl::output_count`] buffers.
    fn eval(
        &self,
        ctx: &GraphContext,
        width: u32,
        height: u32,
        inputs: &[ImageRgba],
    ) -> Result<Vec<ImageRgba>, GraphError>;
}

impl GraphNodeBody {
    /// UI-friendly display name ("Constants Color", "Slope Blur (Gray)", ...).
    pub fn label(&self) -> &'static str {
        match self {
            Self::ConstantColor { .. } => "Constant Color",
            Self::ExternalImage { .. } => "External Image",
            Self::FbmNoise { .. } => "FBM Noise",
            Self::WhiteNoise { .. } => "White Noise",
            Self::BnWSpots { .. } => "BnW Spots",
            Self::Checker { .. } => "Checker",
            Self::Gradient { .. } => "Gradient",
            Self::Brick { .. } => "Bricks",
            Self::Marble { .. } => "Marble",
            Self::Crystal { .. } => "Crystal",
            Self::BlobScatter { .. } => "Blob Scatter",
            Self::Blend { .. } => "Blend",
            Self::Levels { .. } => "Levels",
            Self::Invert => "Invert",
            Self::GaussianBlur { .. } => "Gaussian Blur",
            Self::Transform { .. } => "Transform 2D",
            Self::TileGenerator { .. } => "Tile Generator",
            Self::SlopeBlur { .. } => "Slope Blur (Gray)",
            Self::DirectionalWarp { .. } => "Directional Warp",
            Self::GradientMap { .. } => "Gradient Map",
            Self::Output => "Graph Output",
        }
    }

    /// Palette grouping for the node library.
    pub fn category(&self) -> &'static str {
        match self {
            Self::Output => "Output",
            Self::Blend { .. }
            | Self::Levels { .. }
            | Self::Invert
            | Self::GaussianBlur { .. }
            | Self::Transform { .. }
            | Self::TileGenerator { .. }
            | Self::SlopeBlur { .. }
            | Self::DirectionalWarp { .. }
            | Self::GradientMap { .. } => "Filter",
            _ => "Generator",
        }
    }

    /// Convenience constructors used by tests, plugins and the future UI.
    pub fn constant(color: [f32; 4]) -> Self {
        Self::ConstantColor { color }
    }

    /// The canonical terminal node.
    pub fn output() -> Self {
        Self::Output
    }
}

impl NodeImpl for GraphNodeBody {
    fn slot_count(&self) -> usize {
        match self {
            Self::ConstantColor { .. }
            | Self::ExternalImage { .. }
            | Self::FbmNoise { .. }
            | Self::WhiteNoise { .. }
            | Self::BnWSpots { .. }
            | Self::Checker { .. }
            | Self::Gradient { .. }
            | Self::Brick { .. }
            | Self::Marble { .. }
            | Self::Crystal { .. }
            | Self::BlobScatter { .. } => 0,
            Self::Blend { .. } | Self::SlopeBlur { .. } | Self::DirectionalWarp { .. } => 2,
            Self::Levels { .. }
            | Self::Invert
            | Self::GaussianBlur { .. }
            | Self::Transform { .. }
            | Self::TileGenerator { .. }
            | Self::GradientMap { .. }
            | Self::Output => 1,
        }
    }

    fn eval(
        &self,
        ctx: &GraphContext,
        width: u32,
        height: u32,
        inputs: &[ImageRgba],
    ) -> Result<Vec<ImageRgba>, GraphError> {
        if inputs.len() != self.slot_count() {
            return Err(GraphError::SlotCount {
                node: GraphNodeId(0),
                expected: self.slot_count(),
                actual: inputs.len(),
            });
        }
        match self {
            Self::ConstantColor { color } => Ok(vec![vec![*color; area(width, height)]]),
            Self::ExternalImage { name } => eval_external_image(ctx, name, width, height),
            Self::FbmNoise {
                seed,
                octaves,
                scale,
                gain,
                contrast,
                color_a,
                color_b,
            } => Ok(vec![eval_fbm_noise(
                *seed ^ ctx.seed,
                *octaves,
                *scale,
                *gain,
                *contrast,
                *color_a,
                *color_b,
                width,
                height,
            )]),
            Self::WhiteNoise {
                seed,
                cells,
                color_a,
                color_b,
            } => Ok(vec![eval_white_noise(
                *seed ^ ctx.seed,
                *cells,
                *color_a,
                *color_b,
                width,
                height,
            )]),
            Self::BnWSpots {
                seed,
                cells,
                radius,
                falloff,
                invert,
            } => Ok(vec![eval_bnw_spots(
                *seed ^ ctx.seed,
                *cells,
                *radius,
                *falloff,
                *invert,
                width,
                height,
            )]),
            Self::Checker {
                cells,
                color_a,
                color_b,
            } => Ok(vec![eval_checker(
                *cells, *color_a, *color_b, width, height,
            )]),
            Self::Gradient {
                kind,
                angle_deg,
                from,
                to,
            } => Ok(vec![eval_gradient(
                *kind, *angle_deg, *from, *to, width, height,
            )]),
            Self::Brick {
                seed,
                cols,
                rows,
                mortar,
                bevel,
                brick_color,
                mortar_color,
            } => Ok(vec![eval_brick(
                *seed ^ ctx.seed,
                *cols,
                *rows,
                *mortar,
                *bevel,
                *brick_color,
                *mortar_color,
                width,
                height,
            )]),
            Self::Marble {
                seed,
                veins,
                turbulence,
                octaves,
                color_a,
                color_b,
            } => Ok(vec![eval_marble(
                *seed ^ ctx.seed,
                *veins,
                *turbulence,
                *octaves,
                *color_a,
                *color_b,
                width,
                height,
            )]),
            Self::Crystal {
                seed,
                cells,
                edge_width,
                edge_darkness,
                color_a,
                color_b,
            } => Ok(vec![eval_crystal(
                *seed ^ ctx.seed,
                *cells,
                *edge_width,
                *edge_darkness,
                *color_a,
                *color_b,
                width,
                height,
            )]),
            Self::BlobScatter {
                seed,
                count,
                size,
                falloff,
                blob_color,
                bg_color,
            } => Ok(vec![eval_blob_scatter(
                *seed ^ ctx.seed,
                *count,
                *size,
                *falloff,
                *blob_color,
                *bg_color,
                width,
                height,
            )]),
            Self::Blend { mode, opacity } => eval_blend(*mode, *opacity, inputs),
            Self::Levels {
                min_in,
                max_in,
                gamma,
                min_out,
                max_out,
            } => eval_levels(*min_in, *max_in, *gamma, *min_out, *max_out, inputs),
            Self::Invert => Ok(vec![invert_colors(&inputs[0])]),
            Self::GaussianBlur { radius } => eval_gaussian_blur(*radius, width, height, inputs),
            Self::Transform {
                scale,
                rotation_deg,
                offset_x,
                offset_y,
            } => eval_transform(
                *scale,
                *rotation_deg,
                *offset_x,
                *offset_y,
                width,
                height,
                inputs,
            ),
            Self::TileGenerator {
                seed,
                cols,
                rows,
                gap,
                jitter,
                zoom,
                mortar,
            } => eval_tile_generator(
                *seed ^ ctx.seed,
                *cols,
                *rows,
                *gap,
                *jitter,
                *zoom,
                *mortar,
                width,
                height,
                inputs,
            ),
            Self::SlopeBlur { radius, taps } => {
                eval_slope_blur(*radius, *taps, width, height, inputs)
            }
            Self::DirectionalWarp {
                angle_deg,
                strength,
            } => eval_directional_warp(*angle_deg, *strength, width, height, inputs),
            Self::GradientMap { stops, mix } => eval_gradient_map(stops, *mix, inputs),
            Self::Output => Ok(vec![inputs[0].clone()]),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Graph
// ─────────────────────────────────────────────────────────────────────────────

/// One authored node: a body plus its resolved slots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: GraphNodeId,
    pub body: GraphNodeBody,
    /// Ordered input slots, length always equals `body.slot_count()`.
    pub inputs: Vec<GraphSlot>,
    /// Authored enable flag. Disabled nodes are evaluated as pass-through of
    /// slot 0 (or gray for slot-less nodes) instead of their body.
    pub enabled: bool,
}

/// The serializable value-only graph (no UI positions). Nodes live in an
/// ordered map; links live on consumer slots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Graph {
    nodes: BTreeMap<GraphNodeId, GraphNode>,
    output: Option<GraphNodeId>,
}

impl Graph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn nodes(&self) -> impl Iterator<Item = &GraphNode> {
        self.nodes.values()
    }

    pub fn node(&self, id: GraphNodeId) -> Result<&GraphNode, GraphError> {
        self.nodes.get(&id).ok_or(GraphError::MissingNode(id))
    }

    pub fn node_mut(&mut self, id: GraphNodeId) -> Result<&mut GraphNode, GraphError> {
        self.nodes.get_mut(&id).ok_or(GraphError::MissingNode(id))
    }

    /// Graph output pointer (SBS "root node").
    pub fn output(&self) -> Option<GraphNodeId> {
        self.output
    }

    pub fn set_output(&mut self, id: GraphNodeId) -> Result<(), GraphError> {
        self.node(id)?;
        self.output = Some(id);
        Ok(())
    }

    fn alloc_id(&self) -> GraphNodeId {
        let next = self.nodes.keys().map(|k| k.0).max().unwrap_or(0) + 1;
        GraphNodeId(next)
    }

    /// Adds a node; its slot vector is initialized to the body's arity.
    pub fn add_node(&mut self, body: GraphNodeBody) -> Result<GraphNodeId, GraphError> {
        if self.node_count() >= u32::MAX as usize {
            return Err(GraphError::InvalidParams("graph node limit reached".into()));
        }
        let id = self.alloc_id();
        let slots = body.slot_count();
        let node = GraphNode {
            id,
            body,
            inputs: vec![GraphSlot::Default; slots],
            enabled: true,
        };
        self.nodes.insert(id, node);
        Ok(id)
    }

    /// Replaces a node's body, resizing the slot vector (new slots default).
    pub fn set_body(&mut self, id: GraphNodeId, body: GraphNodeBody) -> Result<(), GraphError> {
        let node = self.node_mut(id)?;
        node.inputs.resize(body.slot_count(), GraphSlot::Default);
        node.body = body;
        Ok(())
    }

    pub fn set_enabled(&mut self, id: GraphNodeId, enabled: bool) -> Result<(), GraphError> {
        self.node_mut(id)?.enabled = enabled;
        Ok(())
    }

    /// Sets up `to.inputs[slot] = Link(from)`.
    pub fn connect(
        &mut self,
        from: GraphNodeId,
        to: GraphNodeId,
        slot: usize,
    ) -> Result<(), GraphError> {
        if slot >= self.node(to)?.body.slot_count() {
            return Err(GraphError::SlotCount {
                node: to,
                expected: slot + 1,
                actual: self.node(to)?.body.slot_count(),
            });
        }
        self.node_mut(to)?.inputs[slot] = GraphSlot::Link(from);
        Ok(())
    }

    /// Clears `to`'s `slot` back to [`GraphSlot::Default`].
    pub fn disconnect(&mut self, to: GraphNodeId, slot: usize) -> Result<(), GraphError> {
        let node = self.node_mut(to)?;
        if slot >= node.inputs.len() {
            return Err(GraphError::MissingSlot { node: to, slot });
        }
        node.inputs[slot] = GraphSlot::Default;
        Ok(())
    }

    /// Overrides an unconnected slot with a constant RGBA value.
    pub fn set_constant(
        &mut self,
        to: GraphNodeId,
        slot: usize,
        value: [f32; 4],
    ) -> Result<(), GraphError> {
        if slot >= self.node(to)?.body.slot_count() {
            return Err(GraphError::MissingSlot { node: to, slot });
        }
        self.node_mut(to)?.inputs[slot] = GraphSlot::Value(value);
        Ok(())
    }

    /// Removes a node and clears every link that pointed at it; drops the
    /// output pointer if it referenced this node.
    pub fn remove_node(&mut self, id: GraphNodeId) -> Result<(), GraphError> {
        self.node(id)?;
        self.nodes.remove(&id);
        for node in self.nodes.values_mut() {
            for slot in &mut node.inputs {
                if let GraphSlot::Link(src) = slot {
                    if *src == id {
                        *slot = GraphSlot::Default;
                    }
                }
            }
        }
        if self.output == Some(id) {
            self.output = None;
        }
        Ok(())
    }

    /// All dependency edges as view structs.
    pub fn edges(&self) -> Vec<GraphEdge> {
        let mut out = Vec::new();
        for node in self.nodes.values() {
            for (slot, input) in node.inputs.iter().enumerate() {
                if let GraphSlot::Link(from) = input {
                    out.push(GraphEdge {
                        from: *from,
                        to: node.id,
                        slot,
                    });
                }
            }
        }
        out
    }

    /// Nodes that consume `id` (transitive dependents via BFS).
    pub fn dependents(&self, id: GraphNodeId) -> Vec<GraphNodeId> {
        let mut order = Vec::new();
        let mut queue = vec![id];
        let mut seen = vec![id];
        while let Some(cur) = queue.pop() {
            for edge in self.edges() {
                if edge.from == cur && !seen.contains(&edge.to) {
                    seen.push(edge.to);
                    order.push(edge.to);
                    queue.push(edge.to);
                }
            }
        }
        order
    }

    /// Structural validation: arity, link targets, duplicate-free ids, output
    /// pointer. Cycle detection happens at evaluation (see [`GraphError::CycleDetected`]).
    pub fn validate(&self) -> Result<(), GraphError> {
        for node in self.nodes.values() {
            let arity = node.body.slot_count();
            if node.inputs.len() != arity {
                return Err(GraphError::SlotCount {
                    node: node.id,
                    expected: arity,
                    actual: node.inputs.len(),
                });
            }
            for input in &node.inputs {
                if let GraphSlot::Link(from) = input {
                    let src = self.node(*from)?;
                    if src.body.output_count() == 0 {
                        return Err(GraphError::BadLink {
                            from: *from,
                            to: node.id,
                        });
                    }
                }
            }
        }
        if let Some(out) = self.output {
            self.node(out)?;
        }
        Ok(())
    }
}

impl GraphSlot {
    /// True when this slot has no authored link or constant.
    pub fn is_unset(&self) -> bool {
        matches!(self, GraphSlot::Default)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Math & image helpers
// ─────────────────────────────────────────────────────────────────────────────

#[inline]
fn area(width: u32, height: u32) -> usize {
    width as usize * height as usize
}

#[inline]
fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

/// Smooth Hermite fade used by all value noise and soft masks.
#[inline]
fn fade(t: f32) -> f32 {
    let t = clamp01(t);
    t * t * (3.0 - 2.0 * t)
}

#[inline]
fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[inline]
fn mix_rgba(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        mix(a[0], b[0], t),
        mix(a[1], b[1], t),
        mix(a[2], b[2], t),
        mix(a[3], b[3], t),
    ]
}

#[inline]
fn mul_rgba(c: [f32; 4], k: f32) -> [f32; 4] {
    [
        clamp01(c[0] * k),
        clamp01(c[1] * k),
        clamp01(c[2] * k),
        clamp01(c[3] * k),
    ]
}

#[inline]
fn luma(c: [f32; 4]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// Rec.709 luma in RGBA form.
#[inline]
fn gray(v: f32) -> [f32; 4] {
    [v, v, v, 1.0]
}

/// Inverts RGB, preserving alpha.
fn invert_colors(data: &ImageRgba) -> ImageRgba {
    data.iter()
        .map(|c| [1.0 - c[0], 1.0 - c[1], 1.0 - c[2], c[3]])
        .collect()
}

/// Bilinear sample of `img` (own dimensions `w`*`h`) at pixel-space coordinates
/// `x`/`y` with repeat wrapping. Pixel index `i` corresponds to coordinate `i`;
/// callers own their pixel-centre / mapping conventions.
fn sample_bilinear_wrap(img: &[[f32; 4]], w: usize, h: usize, x: f32, y: f32) -> [f32; 4] {
    if img.is_empty() || w == 0 || h == 0 {
        return TRANSPARENT;
    }
    let (wn, hn) = (w as f32, h as f32);
    let x = ((x % wn) + wn) % wn;
    let y = ((y % hn) + hn) % hn;
    let x0 = x.floor() as usize % w;
    let y0 = y.floor() as usize % h;
    let x1 = (x0 + 1) % w;
    let y1 = (y0 + 1) % h;
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let c00 = img[y0 * w + x0];
    let c10 = img[y0 * w + x1];
    let c01 = img[y1 * w + x0];
    let c11 = img[y1 * w + x1];
    mix_rgba(mix_rgba(c00, c10, tx), mix_rgba(c01, c11, tx), ty)
}

/// Ensures every input buffer matches the requested resolution.
fn check_inputs(width: u32, height: u32, inputs: &[ImageRgba]) -> Result<(), GraphError> {
    let expected = area(width, height);
    for img in inputs {
        if img.len() != expected {
            return Err(GraphError::SizeMismatch {
                expected,
                actual: img.len(),
            });
        }
    }
    Ok(())
}

#[inline]
fn row_major(i: usize, w: usize) -> (usize, usize) {
    (i % w, i / w)
}

/// Parallel per-pixel map: builds an RGBA buffer where pixel `i`'s color is
/// `f(i)` with `(x, y)` passed for convenience. Deterministic by construction.
fn pixels_for_each<F>(width: u32, height: u32, f: F) -> ImageRgba
where
    F: Fn(usize, usize, usize) -> [f32; 4] + Sync,
{
    let mut data: ImageRgba = vec![TRANSPARENT; area(width, height)];
    let w = width as usize;
    data.par_iter_mut().enumerate().for_each(|(i, px)| {
        let (x, y) = row_major(i, w);
        *px = f(i, x, y);
    });
    data
}

// ─────────────────────────────────────────────────────────────────────────────
// Deterministic PRNG & lattice noise (no external crates)
// ─────────────────────────────────────────────────────────────────────────────

/// Stateless 2.5D integer hash in [0, 1). Fully deterministic across threads.
#[inline]
pub fn hash2(x: i32, y: i32, seed: u32) -> f32 {
    let mut n = (x as u32).wrapping_mul(0x9E37_79B1)
        ^ (y as u32).wrapping_mul(0x85EB_CA77)
        ^ seed.wrapping_mul(0xC2B2_AE3D);
    n = (n ^ (n >> 15)).wrapping_mul(0x2C1B_3C6D);
    n = (n ^ (n >> 12)).wrapping_mul(0x297A_2D39);
    n ^= n >> 15;
    (n >> 8) as f32 / 16_777_216.0
}

/// Jittered feature position inside lattice cell `(ix, iy)`.
#[inline]
fn cell_jitter(ix: i32, iy: i32, seed: u32) -> (f32, f32) {
    (hash2(ix, iy, seed), hash2(ix, iy, seed ^ 0xA5A5_5A5A))
}

/// Tileable value noise: `u`/`v` in [0, 1), `period` lattice cells across.
fn value_noise(u: f32, v: f32, period: i32, seed: u32) -> f32 {
    let p = period.max(1);
    let xf = clamp01(u.fract()) * p as f32;
    let yf = clamp01(v.fract()) * p as f32;
    let ix = xf.floor() as i32;
    let iy = yf.floor() as i32;
    let tx = fade(xf - ix as f32);
    let ty = fade(yf - iy as f32);
    let x0 = ix.rem_euclid(p);
    let y0 = iy.rem_euclid(p);
    let x1 = (x0 + 1) % p;
    let y1 = (y0 + 1) % p;
    mix(
        mix(hash2(x0, y0, seed), hash2(x1, y0, seed), tx),
        mix(hash2(x0, y1, seed), hash2(x1, y1, seed), tx),
        ty,
    )
}

/// Tileable FBM over [`value_noise`]; `base_period` doubles each octave so the
/// result stays period-aligned with the image.
fn fbm(u: f32, v: f32, seed: u32, octaves: u32, base_period: u32, gain: f32) -> f32 {
    let mut amp = 1.0f32;
    let mut sum = 0.0f32;
    let mut norm = 0.0f32;
    let mut period = base_period.max(1);
    for o in 0..octaves.clamp(1, MAX_OCTAVES) {
        let s = seed.wrapping_add((o as u32).wrapping_mul(0x9E37_79B9));
        sum += value_noise(u, v, period as i32, s) * amp;
        norm += amp;
        amp *= gain.clamp(0.05, 1.0);
        period = (period * 2).min(4096);
    }
    if norm <= 0.0 {
        0.5
    } else {
        sum / norm
    }
}

/// Wrapped uv distance (toroidal) for tileable feature placement.
#[inline]
fn wrapped_uv_delta(a: f32, b: f32) -> f32 {
    let d = (a - b).abs();
    d.min(1.0 - d)
}

// ─────────────────────────────────────────────────────────────────────────────
// Generator node kernels
// ─────────────────────────────────────────────────────────────────────────────

fn eval_external_image(
    ctx: &GraphContext,
    name: &str,
    width: u32,
    height: u32,
) -> Result<Vec<ImageRgba>, GraphError> {
    let sample = ctx
        .samples
        .get(name)
        .ok_or_else(|| GraphError::MissingSample(name.to_string()))?;
    if !sample.is_consistent() || sample.width == 0 || sample.height == 0 {
        return Err(GraphError::InvalidParams(format!(
            "sample {:?} has inconsistent dimensions",
            name
        )));
    }
    let (sw, sh, w, h) = (
        sample.width as usize,
        sample.height as usize,
        width as usize,
        height as usize,
    );
    let data = sample.data.as_slice();
    let out = pixels_for_each(width, height, |_, x, y| {
        let u = x as f32 / w as f32;
        let v = y as f32 / h as f32;
        sample_bilinear_wrap(data, sw, sh, u * sw as f32, v * sh as f32)
    });
    Ok(vec![out])
}

fn eval_fbm_noise(
    seed: u32,
    octaves: u32,
    scale: f32,
    gain: f32,
    contrast: f32,
    color_a: [f32; 4],
    color_b: [f32; 4],
    width: u32,
    height: u32,
) -> ImageRgba {
    let base = scale.max(1.0).min(2048.0) as u32;
    let oc = octaves.clamp(1, MAX_OCTAVES);
    let g = gain.clamp(0.05, 1.0);
    let c = contrast.max(0.05);
    pixels_for_each(width, height, move |_, x, y| {
        let u = x as f32 / width as f32;
        let v = y as f32 / height as f32;
        let n = fbm(u, v, seed, oc, base, g);
        mix_rgba(color_a, color_b, clamp01((n - 0.5) * c + 0.5))
    })
}

fn eval_white_noise(
    seed: u32,
    cells: u32,
    color_a: [f32; 4],
    color_b: [f32; 4],
    width: u32,
    height: u32,
) -> ImageRgba {
    let cells = cells.max(1) as i32;
    pixels_for_each(width, height, |_, x, y| {
        let u = x as f32 / width as f32;
        let v = y as f32 / height as f32;
        let n = hash2((u * cells as f32) as i32, (v * cells as f32) as i32, seed);
        mix_rgba(color_a, color_b, n)
    })
}

/// One softly-blended feature spot per Voronoi cell, tile-safe.
fn eval_bnw_spots(
    seed: u32,
    cells: u32,
    radius: f32,
    falloff: f32,
    invert: bool,
    width: u32,
    height: u32,
) -> ImageRgba {
    let p = cells.max(1) as i32;
    let r = radius.clamp(0.02, 2.0);
    let fo = falloff.clamp(0.05, 16.0);
    pixels_for_each(width, height, |_, x, y| {
        let cx = x as f32 / width as f32 * p as f32;
        let cy = y as f32 / height as f32 * p as f32;
        let ix = cx.floor() as i32;
        let iy = cy.floor() as i32;
        let mut best = f32::INFINITY;
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (nx, ny) = (ix + dx, iy + dy);
                let jx = nx as f32 + cell_jitter(nx, ny, seed).0;
                let jy = ny as f32 + cell_jitter(nx, ny, seed ^ 0x5BDB_1D69).1;
                let d = ((cx - jx).hypot(cy - jy) / r).powf(fo);
                best = best.min(d);
            }
        }
        let mut t = clamp01(best);
        if invert {
            t = 1.0 - t;
        }
        gray(t)
    })
}

fn eval_checker(
    cells: u32,
    color_a: [f32; 4],
    color_b: [f32; 4],
    width: u32,
    height: u32,
) -> ImageRgba {
    let cells = cells.max(1) as i32;
    pixels_for_each(width, height, |_, x, y| {
        let ix = (x as f32 / width as f32 * cells as f32).floor() as i32;
        let iy = (y as f32 / height as f32 * cells as f32).floor() as i32;
        if (ix + iy).rem_euclid(2) == 0 {
            color_a
        } else {
            color_b
        }
    })
}

fn eval_gradient(
    kind: GradientKind,
    angle_deg: f32,
    from: [f32; 4],
    to: [f32; 4],
    width: u32,
    height: u32,
) -> ImageRgba {
    let theta = angle_deg.to_radians();
    let (c, s) = (theta.cos(), theta.sin());
    let denom = (c.abs() + s.abs()).max(1e-3);
    pixels_for_each(width, height, |_, x, y| {
        let u = x as f32 / width as f32;
        let v = y as f32 / height as f32;
        let t = match kind {
            GradientKind::Linear => ((u - 0.5) * c + (v - 0.5) * s) / denom + 0.5,
            GradientKind::Radial => ((u - 0.5).hypot(v - 0.5) * 2.0).min(1.0),
            GradientKind::Angle => {
                let a = (v - 0.5).atan2(u - 0.5);
                (a + std::f32::consts::PI) / (2.0 * std::f32::consts::PI)
            }
        };
        mix_rgba(from, to, clamp01(t))
    })
}

fn eval_brick(
    seed: u32,
    cols: u32,
    rows: u32,
    mortar: f32,
    bevel: f32,
    brick_color: [f32; 4],
    mortar_color: [f32; 4],
    width: u32,
    height: u32,
) -> ImageRgba {
    let c = cols.max(1) as f32;
    let r = rows.max(1) as f32;
    let half = mortar.clamp(0.0, 0.5) * 0.5;
    let zone = bevel.clamp(0.0, 0.25).max(1e-3);
    pixels_for_each(width, height, |_, x, y| {
        let fx = x as f32 / width as f32 * c;
        let fy = y as f32 / height as f32 * r;
        let row = fy.floor() as i32;
        let shift = if row.rem_euclid(2) == 1 { 0.5 } else { 0.0 };
        let bx = fx + shift;
        let col = bx.floor() as i32;
        let ex = (bx - bx.floor()).min(1.0 - bx.fract());
        let ey = (fy - fy.floor()).min(1.0 - fy.fract());
        let d = ex.min(ey);
        if d < half {
            mortar_color
        } else {
            // Seeded per-brick tone jitter keeps walls from looking flat.
            let jitter = hash2(col, row, seed);
            let edge_t = fade((d - half) / (0.25 - half + zone).min(1.0));
            let shade = mix(1.0 - zone, 1.0, edge_t);
            let col = mul_rgba(brick_color, shade * (1.0 - 0.18 * jitter));
            [col[0], col[1], col[2], brick_color[3]]
        }
    })
}

fn eval_marble(
    seed: u32,
    veins: f32,
    turbulence: f32,
    octaves: u32,
    color_a: [f32; 4],
    color_b: [f32; 4],
    width: u32,
    height: u32,
) -> ImageRgba {
    let freq = veins.clamp(0.0, 4096.0) * std::f32::consts::TAU;
    let turb = turbulence.clamp(0.0, 16.0);
    pixels_for_each(width, height, |_, x, y| {
        let u = x as f32 / width as f32;
        let v = y as f32 / height as f32;
        let t = fbm(u, v, seed, octaves, 4, 0.55) - 0.5;
        let band = (u * freq + t * turb).sin();
        mix_rgba(color_a, color_b, band.mul_add(0.5, 0.5))
    })
}

fn eval_crystal(
    seed: u32,
    cells: u32,
    edge_width: f32,
    edge_darkness: f32,
    color_a: [f32; 4],
    color_b: [f32; 4],
    width: u32,
    height: u32,
) -> ImageRgba {
    let p = cells.max(1) as i32;
    let ew = edge_width.clamp(0.001, 1.0);
    let ed = edge_darkness.clamp(0.0, 1.0);
    pixels_for_each(width, height, |_, x, y| {
        let cx = x as f32 / width as f32 * p as f32;
        let cy = y as f32 / height as f32 * p as f32;
        let ix = cx.floor() as i32;
        let iy = cy.floor() as i32;
        let (mut f1, mut f2, mut ncx, mut ncy) = (f32::INFINITY, f32::INFINITY, ix, iy);
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (nx, ny) = (ix + dx, iy + dy);
                let (wx, wy) = (nx.rem_euclid(p), ny.rem_euclid(p));
                let jx = nx as f32 + cell_jitter(wx, wy, seed).0;
                let jy = ny as f32 + cell_jitter(wx, wy, seed ^ 0x5BDB_1D69).1;
                let d = (cx - jx).hypot(cy - jy);
                if d < f1 {
                    f2 = f1;
                    f1 = d;
                    ncx = nx;
                    ncy = ny;
                } else if d < f2 {
                    f2 = d;
                }
            }
        }
        let cell_n = hash2(ncx.rem_euclid(p), ncy.rem_euclid(p), seed ^ 0x7F4A_7C15);
        let edge_t = fade(clamp01(1.0 - (f2 - f1) / ew));
        let shade = 1.0 - ed * edge_t;
        let col = mix_rgba(color_a, color_b, cell_n);
        [
            clamp01(col[0] * shade),
            clamp01(col[1] * shade),
            clamp01(col[2] * shade),
            col[3],
        ]
    })
}

fn eval_blob_scatter(
    seed: u32,
    count: u32,
    size: f32,
    falloff: f32,
    blob_color: [f32; 4],
    bg_color: [f32; 4],
    width: u32,
    height: u32,
) -> ImageRgba {
    let n = count.clamp(1, 512);
    let base_r = size.clamp(0.005, 0.5);
    let fo = falloff.clamp(0.05, 16.0);
    // Precompute the blob cloud once; per-pixel work stays branch-light.
    let blobs: Vec<(f32, f32, f32)> = (0..n)
        .map(|i| {
            let px = hash2(i as i32, 0x1234, seed);
            let py = hash2(i as i32, 0x5678, seed);
            let r = base_r * mix(0.45, 1.3, hash2(i as i32, 0x9ABC, seed));
            (px, py, r)
        })
        .collect();
    pixels_for_each(width, height, |_, x, y| {
        let u = x as f32 / width as f32;
        let v = y as f32 / height as f32;
        let mut acc = 0.0f32;
        for &(bx, by, br) in &blobs {
            let dx = wrapped_uv_delta(u, bx);
            let dy = wrapped_uv_delta(v, by);
            let d = dx.hypot(dy) / br;
            if d < 1.0 {
                let glow = (1.0 - d).powf(fo);
                acc = acc.max(glow);
            }
        }
        mix_rgba(bg_color, blob_color, acc)
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Filter node kernels
// ─────────────────────────────────────────────────────────────────────────────

fn eval_blend(
    mode: BlendMode,
    opacity: f32,
    inputs: &[ImageRgba],
) -> Result<Vec<ImageRgba>, GraphError> {
    let expected = inputs.first().map(|i| i.len()).unwrap_or(0);
    for img in inputs {
        if img.len() != expected {
            return Err(GraphError::SizeMismatch {
                expected,
                actual: img.len(),
            });
        }
    }
    let o = opacity.clamp(0.0, 1.0);
    let out = inputs[0]
        .iter()
        .zip(inputs[1].iter())
        .map(|(&base, &top)| {
            let a = o * clamp01(top[3]);
            let blended = blend_colors([base[0], base[1], base[2]], [top[0], top[1], top[2]], mode);
            [
                mix(base[0], blended[0], a),
                mix(base[1], blended[1], a),
                mix(base[2], blended[2], a),
                a + base[3] * (1.0 - a),
            ]
        })
        .collect();
    Ok(vec![out])
}

fn eval_levels(
    min_in: f32,
    max_in: f32,
    gamma: f32,
    min_out: f32,
    max_out: f32,
    inputs: &[ImageRgba],
) -> Result<Vec<ImageRgba>, GraphError> {
    let out = inputs[0]
        .iter()
        .map(|&c| {
            [
                apply_levels_scalar(c[0], min_in, max_in, gamma, min_out, max_out),
                apply_levels_scalar(c[1], min_in, max_in, gamma, min_out, max_out),
                apply_levels_scalar(c[2], min_in, max_in, gamma, min_out, max_out),
                apply_levels_scalar(c[3], min_in, max_in, gamma, min_out, max_out),
            ]
        })
        .collect();
    Ok(vec![out])
}

fn eval_gaussian_blur(
    radius: f32,
    width: u32,
    height: u32,
    inputs: &[ImageRgba],
) -> Result<Vec<ImageRgba>, GraphError> {
    let expected = area(width, height);
    if inputs[0].len() != expected {
        return Err(GraphError::SizeMismatch {
            expected,
            actual: inputs[0].len(),
        });
    }
    Ok(vec![gaussian_blur_rgba(
        &inputs[0],
        width as usize,
        height as usize,
        radius,
    )])
}

fn eval_transform(
    scale: f32,
    rotation_deg: f32,
    offset_x: f32,
    offset_y: f32,
    width: u32,
    height: u32,
    inputs: &[ImageRgba],
) -> Result<Vec<ImageRgba>, GraphError> {
    let (w, h) = (width as usize, height as usize);
    check_inputs(width, height, inputs)?;
    let s = scale.max(1e-3);
    let theta = rotation_deg.to_radians();
    let (c, sn) = (theta.cos(), theta.sin());
    let src = inputs[0].as_slice();
    let out = pixels_for_each(width, height, |_, x, y| {
        let u = (x as f32 + 0.5) / w as f32 - 0.5;
        let v = (y as f32 + 0.5) / h as f32 - 0.5;
        let rx = (u * c + v * sn) / s + 0.5 + offset_x;
        let ry = (-u * sn + v * c) / s + 0.5 + offset_y;
        sample_bilinear_wrap(src, w, h, rx * w as f32 - 0.5, ry * h as f32 - 0.5)
    });
    Ok(vec![out])
}

fn eval_tile_generator(
    seed: u32,
    cols: u32,
    rows: u32,
    gap: f32,
    jitter: f32,
    zoom: f32,
    mortar: [f32; 4],
    width: u32,
    height: u32,
    inputs: &[ImageRgba],
) -> Result<Vec<ImageRgba>, GraphError> {
    let (w, h) = (width as usize, height as usize);
    check_inputs(width, height, inputs)?;
    let (c, r) = (cols.max(1), rows.max(1));
    let g = gap.clamp(0.0, 0.9);
    let j = jitter.clamp(0.0, 1.0);
    let z = zoom.clamp(0.05, 1.0);
    let src = inputs[0].as_slice();
    let out = pixels_for_each(width, height, |_, x, y| {
        let tw = w as f32 / c as f32;
        let th = h as f32 / r as f32;
        let tu = x as f32 / tw;
        let tv = y as f32 / th;
        let (tx, ty) = (tu.floor() as i32, tv.floor() as i32);
        let (lx, ly) = (tu - tx as f32, tv - ty as f32);
        if lx < g || ly < g {
            return mortar;
        }
        let jx = (hash2(tx, ty, seed) - 0.5) * j;
        let jy = (hash2(tx, ty, seed ^ 0x5BDB_1D69) - 0.5) * j;
        let sx = (tx as f32 + jx + (lx - g) / (1.0 - g) * z) / c as f32 * w as f32;
        let sy = (ty as f32 + jy + (ly - g) / (1.0 - g) * z) / r as f32 * h as f32;
        sample_bilinear_wrap(src, w, h, sx, sy)
    });
    Ok(vec![out])
}

/// Blur that follows the local gradient of the height input: smears the base
/// image along the slope, sharpening creases like reference's Slope Blur Gray.
fn eval_slope_blur(
    radius: f32,
    taps: u32,
    width: u32,
    height: u32,
    inputs: &[ImageRgba],
) -> Result<Vec<ImageRgba>, GraphError> {
    let (w, h) = (width as usize, height as usize);
    check_inputs(width, height, inputs)?;
    let r = radius.clamp(0.0, 512.0);
    let k = taps.clamp(2, 64) as usize;
    let base = inputs[0].as_slice();
    let heights = inputs[1].as_slice();
    let out = pixels_for_each(width, height, |_, x, y| {
        let i = y * w + x;
        let hl = heights[y * w + (x + w - 1) % w][0];
        let hr = heights[y * w + (x + 1) % w][0];
        let hd = heights[(y + 1) % h * w + x][0];
        let hu = heights[(y + h - 1) % h * w + x][0];
        let (gx, gy) = ((hr - hl) * 0.5, (hd - hu) * 0.5);
        let len = gx.hypot(gy);
        if len < 1e-5 || r < 1e-3 {
            return base[i];
        }
        let (dx, dy) = (gx / len, gy / len);
        let (mut sum, mut wsum) = ([0.0f32; 4], 0.0f32);
        for t in 0..k {
            let off = (t as f32 / (k - 1) as f32 * 2.0 - 1.0) * r;
            let wt = 1.0 - off.abs() / r;
            let s = sample_bilinear_wrap(base, w, h, x as f32 + dx * off, y as f32 + dy * off);
            for ch in 0..4 {
                sum[ch] += s[ch] * wt;
            }
            wsum += wt;
        }
        if wsum <= 0.0 {
            return base[i];
        }
        [sum[0] / wsum, sum[1] / wsum, sum[2] / wsum, sum[3] / wsum]
    });
    Ok(vec![out])
}

fn eval_directional_warp(
    angle_deg: f32,
    strength: f32,
    width: u32,
    height: u32,
    inputs: &[ImageRgba],
) -> Result<Vec<ImageRgba>, GraphError> {
    let (w, h) = (width as usize, height as usize);
    check_inputs(width, height, inputs)?;
    let theta = angle_deg.to_radians();
    let (dx, dy) = (theta.cos(), theta.sin());
    let s = strength.clamp(-4096.0, 4096.0);
    let src = inputs[0].as_slice();
    let heights = inputs[1].as_slice();
    let out = pixels_for_each(width, height, |_, x, y| {
        let deviation = clamp01(heights[y * w + x][0]) * 2.0 - 1.0;
        sample_bilinear_wrap(
            src,
            w,
            h,
            x as f32 + dx * deviation * s,
            y as f32 + dy * deviation * s,
        )
    });
    Ok(vec![out])
}

/// Colorizes the luminance of the input through sorted gradient stops, then
/// lerp-mixes the result back over the input by `mix`.
fn eval_gradient_map(
    stops: &[GradientStop],
    mix_amount: f32,
    inputs: &[ImageRgba],
) -> Result<Vec<ImageRgba>, GraphError> {
    if stops.is_empty() {
        return Err(GraphError::InvalidParams(
            "GradientMap needs at least one stop".into(),
        ));
    }
    let mut sorted = stops.to_vec();
    sorted.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
    let first = sorted[0];
    let last = sorted[sorted.len() - 1];
    let m = mix_amount.clamp(0.0, 1.0);
    let out = inputs[0]
        .iter()
        .map(|&c| {
            let l = clamp01(luma(c));
            let color = ramp_color(&sorted, first, last, l);
            [
                mix(c[0], color[0], m),
                mix(c[1], color[1], m),
                mix(c[2], color[2], m),
                mix(c[3], color[3], m),
            ]
        })
        .collect();
    Ok(vec![out])
}

#[inline]
fn ramp_color(stops: &[GradientStop], first: GradientStop, last: GradientStop, l: f32) -> [f32; 4] {
    if l <= first.t {
        first.color
    } else if l >= last.t {
        last.color
    } else {
        let mut lo = first;
        for stop in stops.iter().skip(1) {
            if stop.t >= l {
                let t = (l - lo.t) / (stop.t - lo.t).max(1e-6);
                return mix_rgba(lo.color, stop.color, t);
            }
            lo = *stop;
        }
        last.color
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Evaluator
// ─────────────────────────────────────────────────────────────────────────────

/// Memoized per-(node, output, resolution) results and the dirty/invalidation
/// bookkeeping on top of a [`Graph`]. Owns an owned snapshot of the context.
///
/// The engine is the UI-facing evaluation handle: mutate the graph (or a
/// context sample), call [`GraphEngine::invalidate_node`], request
/// [`GraphEngine::render`]. Cycles surface as a diagnostic error, never a panic.
#[derive(Debug, Clone)]
pub struct GraphEngine {
    pub graph: Graph,
    pub context: GraphContext,
    /// `(node, output, width, height) -> evaluated buffer`.
    cache: HashMap<(OutputRef, u32, u32), Arc<ImageRgba>>,
}

impl GraphEngine {
    pub fn new(graph: Graph, context: GraphContext) -> Self {
        Self {
            graph,
            context,
            cache: HashMap::new(),
        }
    }

    /// Number of memoized buffers (UI/debug).
    pub fn cache_len(&self) -> usize {
        self.cache.len()
    }

    /// Renders the graph output at the context resolution.
    pub fn render(&mut self) -> Result<Arc<ImageRgba>, GraphError> {
        let id = self.graph.output().ok_or(GraphError::NoOutput)?;
        self.render_node(id, self.context.width, self.context.height)
    }

    /// Renders any node's primary output at `(width, height)`. Memoized; a
    /// repeated call is a cache hit.
    pub fn render_node(
        &mut self,
        id: GraphNodeId,
        width: u32,
        height: u32,
    ) -> Result<Arc<ImageRgba>, GraphError> {
        let (w, h) = sane_resolution(width, height)?;
        let outs = self.eval_node(id, w, h, &mut Vec::new())?;
        outs.into_iter().next().ok_or(GraphError::MissingOutput {
            node: id,
            output: 0,
        })
    }

    /// Renders a specific output index of a node.
    pub fn render_output(
        &mut self,
        id: GraphNodeId,
        output: u32,
        width: u32,
        height: u32,
    ) -> Result<Arc<ImageRgba>, GraphError> {
        let (w, h) = sane_resolution(width, height)?;
        let outputs = self.eval_node(id, w, h, &mut Vec::new())?;
        outputs
            .get(output as usize)
            .cloned()
            .ok_or(GraphError::MissingOutput { node: id, output })
    }

    /// Memoized topological evaluation. `path` carries the current DFS chain
    /// for cycle diagnostics.
    fn eval_node(
        &mut self,
        id: GraphNodeId,
        width: u32,
        height: u32,
        path: &mut Vec<GraphNodeId>,
    ) -> Result<Vec<Arc<ImageRgba>>, GraphError> {
        if path.contains(&id) {
            let mut cycle = path.clone();
            cycle.push(id);
            return Err(GraphError::CycleDetected(cycle));
        }
        let node = self.graph.node(id)?.clone();
        // Fast path: every output of this node is already memoized.
        let mut hits = Vec::with_capacity(node.body.output_count() as usize);
        for out in 0..node.body.output_count() {
            match self.cache.get(&(OutputRef(id, out as u32), width, height)) {
                Some(buf) => hits.push(buf.clone()),
                None => {
                    hits.clear();
                    break;
                }
            }
        }
        if hits.len() == node.body.output_count() as usize {
            return Ok(hits);
        }
        path.push(id);
        let result = self.eval_uncached(&node, width, height, path);
        path.pop();
        let buffers = result?;
        let mut outs = Vec::with_capacity(buffers.len());
        for (out, buf) in buffers.into_iter().enumerate() {
            let key = (OutputRef(id, out as u32), width, height);
            let arc = Arc::new(buf);
            self.cache.insert(key, arc.clone());
            outs.push(arc);
        }
        Ok(outs)
    }

    /// Fresh evaluation (no cache): resolve slots, dispatch the body.
    fn eval_uncached(
        &mut self,
        node: &GraphNode,
        width: u32,
        height: u32,
        path: &mut Vec<GraphNodeId>,
    ) -> Result<Vec<ImageRgba>, GraphError> {
        if !node.enabled {
            // Disabled nodes pass their first slot through (gray if slot-less).
            let pass = if node.inputs.is_empty() {
                Arc::new(solid(GRAY, width, height))
            } else {
                let slot = node.inputs[0];
                self.resolve_slot(&slot, node.body.slot_default(0), width, height, path)?
            };
            return Ok(vec![(*pass).clone()]);
        }
        let mut inputs = Vec::with_capacity(node.inputs.len());
        for (slot, input) in node.inputs.iter().enumerate() {
            let def = node.body.slot_default(slot);
            inputs.push((*self.resolve_slot(input, def, width, height, path)?).clone());
        }
        node.body
            .eval(&self.context.clone(), width, height, &inputs)
    }

    /// Materializes one slot: constant value, built-in default, or an
    /// upstream node's output.
    fn resolve_slot(
        &mut self,
        slot: &GraphSlot,
        default: [f32; 4],
        width: u32,
        height: u32,
        path: &mut Vec<GraphNodeId>,
    ) -> Result<Arc<ImageRgba>, GraphError> {
        match slot {
            GraphSlot::Default => Ok(Arc::new(solid(default, width, height))),
            GraphSlot::Value(color) => Ok(Arc::new(solid(*color, width, height))),
            GraphSlot::Link(src) => {
                let outs = self.eval_node(*src, width, height, path)?;
                outs.into_iter().next().ok_or(GraphError::MissingOutput {
                    node: *src,
                    output: 0,
                })
            }
        }
    }

    // ── invalidation / dirty-flag API ────────────────────────────────────────

    /// Drops memoized buffers for `id` and every transitive dependent. Returns
    /// how many buffers were dropped (callers can assert progress in tests).
    pub fn invalidate_node(&mut self, id: GraphNodeId) -> usize {
        let mut targets = self.graph.dependents(id);
        targets.push(id);
        let before = self.cache.len();
        self.cache
            .retain(|(output, _, _), _| !targets.contains(&output.0));
        before - self.cache.len()
    }

    /// Full domain invalidation (context seed or resolution semantics change).
    pub fn invalidate_all(&mut self) {
        self.cache.clear();
    }

    /// Coarse-grained edit helper for early UI wiring: run any sequence of
    /// graph mutations, then invalidate everything touched by them. Fine
    /// grained parking spot: [`GraphEngine::invalidate_node`] once slots are
    /// wired per node.
    pub fn edit<G>(&mut self, f: G)
    where
        G: FnOnce(&mut Graph),
    {
        f(&mut self.graph);
        self.invalidate_all();
    }

    /// Replaces the context entirely; results from previous contexts never
    /// survive (seed/resolution are cache-key inputs, samples are not, so be
    /// explicit).
    pub fn set_context(&mut self, context: GraphContext) {
        self.context = context;
        self.invalidate_all();
    }

    /// Registers or replaces a named sample; samplers reading it must be
    /// invalidated manually (see [`GraphEngine::invalidate_node`]).
    pub fn insert_sample(&mut self, name: impl Into<String>, image: SampleImage) {
        self.context.samples.insert(name.into(), image);
    }
}

/// Clamps a resolution request into the engine's supported envelope.
fn sane_resolution(width: u32, height: u32) -> Result<(u32, u32), GraphError> {
    if width == 0 || height == 0 {
        return Err(GraphError::InvalidParams(
            "resolution must be at least 1x1".into(),
        ));
    }
    if width as u64 * height as u64 > MAX_PIXELS {
        return Err(GraphError::InvalidParams(format!(
            "resolution {}x{} exceeds the {}-pixel budget",
            width, height, MAX_PIXELS
        )));
    }
    Ok((width, height))
}

/// Uniform RGBA buffer at a requested size.
fn solid(color: [f32; 4], width: u32, height: u32) -> ImageRgba {
    vec![color; area(width, height)]
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests (kept small: 64x64 or smaller, whole suite targets < 1s)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const W: u32 = 64;
    const H: u32 = 64;

    fn ctx(seed: u32) -> GraphContext {
        GraphContext::new(W, H, seed)
    }

    fn engine_of(graph: Graph, seed: u32) -> GraphEngine {
        GraphEngine::new(graph, ctx(seed))
    }

    fn small_graph(seed: u32, output_seed: u32) -> (Graph, GraphNodeId, GraphNodeId) {
        let mut g = Graph::new();
        let a = g
            .add_node(GraphNodeBody::FbmNoise {
                seed,
                octaves: 3,
                scale: 8.0,
                gain: 0.55,
                contrast: 1.0,
                color_a: BLACK,
                color_b: WHITE,
            })
            .unwrap();
        let b = g
            .add_node(GraphNodeBody::Levels {
                min_in: 0.0,
                max_in: 1.0,
                gamma: 1.0,
                min_out: 0.1,
                max_out: 0.9,
            })
            .unwrap();
        g.connect(a, b, 0).unwrap();
        g.set_output(b).unwrap();
        let _ = output_seed;
        (g, a, b)
    }

    #[test]
    fn fbm_noise_is_deterministic_and_seed_sensitive() {
        let img1 = {
            let (g, a, _) = small_graph(11, 0);
            engine_of(g, 7).render_node(a, W, H).unwrap()
        };
        // Deterministic across recomputations at the same resolution.
        let again = {
            let (g, a, _) = small_graph(11, 0);
            engine_of(g, 7).render_node(a, W, H).unwrap()
        };
        assert_eq!(img1[..], again[..]);

        // A different node seed changes the field, but stays in [0, 1].
        let other = {
            let (g, a, _) = small_graph(12, 0);
            engine_of(g, 7).render_node(a, W, H).unwrap()
        };
        assert_ne!(img1[0], other[0]);
        for px in other.iter() {
            assert!(px.iter().all(|&c| (0.0..=1.0).contains(&c)));
        }

        // Context-level seed mixes into every node seed.
        let rescoped = {
            let (g, a, _) = small_graph(11, 0);
            engine_of(g, 8).render_node(a, W, H).unwrap()
        };
        assert_ne!(img1[0], rescoped[0]);
    }

    #[test]
    fn constant_color_and_output() {
        let mut g = Graph::new();
        let n = g
            .add_node(GraphNodeBody::constant([0.25, 0.5, 0.75, 1.0]))
            .unwrap();
        let out = g.add_node(GraphNodeBody::output()).unwrap();
        g.connect(n, out, 0).unwrap();
        g.set_output(out).unwrap();
        let img = engine_of(g, 1).render().unwrap();
        assert_eq!(img[42], [0.25, 0.5, 0.75, 1.0]);
    }

    #[test]
    fn blend_math_matches_layer_module() {
        // Multiply: red x blue = black, opacity 1.
        let img = blend_result(BlendMode::Multiply, 1.0);
        assert!((img[0][0]).abs() < 1e-6);
        assert!((img[0][2]).abs() < 1e-6);
        assert!((img[0][3] - 1.0).abs() < 1e-6);

        // Normal @ 0.5 opacity mixes base and top evenly.
        let img = blend_result(BlendMode::Normal, 0.5);
        assert!((img[0][0] - 0.5).abs() < 1e-6);
        assert!((img[0][2] - 0.5).abs() < 1e-6);

        // Multiply @ 0 opacity leaves the base untouched.
        let img = blend_result(BlendMode::Multiply, 0.0);
        assert!((img[0][0] - 1.0).abs() < 1e-6);
        assert!((img[0][2] - 0.0).abs() < 1e-6);
    }

    fn blend_result(mode: BlendMode, opacity: f32) -> Vec<[f32; 4]> {
        let mut g = Graph::new();
        let base = g
            .add_node(GraphNodeBody::constant([1.0, 0.0, 0.0, 1.0]))
            .unwrap();
        let top = g
            .add_node(GraphNodeBody::constant([0.0, 0.0, 1.0, 1.0]))
            .unwrap();
        let blend = g.add_node(GraphNodeBody::Blend { mode, opacity }).unwrap();
        let out = g.add_node(GraphNodeBody::output()).unwrap();
        g.connect(base, blend, 0).unwrap();
        g.connect(top, blend, 1).unwrap();
        g.connect(blend, out, 0).unwrap();
        g.set_output(out).unwrap();
        (*engine_of(g, 0).render().unwrap()).clone()
    }

    #[test]
    fn invert_and_levels_spot_checks() {
        let mut g = Graph::new();
        let src = g
            .add_node(GraphNodeBody::constant([1.0, 0.75, 0.25, 0.6]))
            .unwrap();
        let inv = g.add_node(GraphNodeBody::Invert).unwrap();
        g.connect(src, inv, 0).unwrap();
        g.set_output(inv).unwrap();
        let img = engine_of(g, 0).render().unwrap();
        assert_eq!(img[0], [0.0, 0.25, 0.75, 0.6]);

        // gamma 2: norm 0.5 -> sqrt(0.5)
        let mut g = Graph::new();
        let src = g
            .add_node(GraphNodeBody::constant([0.5, 0.5, 0.5, 1.0]))
            .unwrap();
        let lvl = g
            .add_node(GraphNodeBody::Levels {
                min_in: 0.0,
                max_in: 1.0,
                gamma: 2.0,
                min_out: 0.0,
                max_out: 1.0,
            })
            .unwrap();
        g.connect(src, lvl, 0).unwrap();
        g.set_output(lvl).unwrap();
        let img = engine_of(g, 0).render().unwrap();
        assert!((img[0][0] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-4);
    }

    #[test]
    fn gaussian_blur_keeps_constants_and_softens_edges() {
        // Constant fields survive the blur (sum of kernel weights is 1).
        let img = blur_of_gradient(0.35, 0.35, 3.0);
        assert!((img[7 * W as usize + 7][0] - 0.35).abs() < 1e-4);

        // A black->white horizontal ramp blurred across its midpoint lands
        // near 0.5 and stays monotonic.
        let img = blur_of_gradient(0.0, 1.0, 3.0);
        let w = W as usize;
        assert!((img[32 * w + 32][0] - 0.5).abs() < 0.06);
        assert!(img[16 * w + 16][0] < img[32 * w + 32][0]);
        assert!(img[32 * w + 32][0] < img[48 * w + 48][0]);
    }

    /// Renders `GaussianBlur` over a linear gradient between the two colors.
    fn blur_of_gradient(from_val: f32, to_val: f32, radius: f32) -> Vec<[f32; 4]> {
        let mut g = Graph::new();
        let ramp = g
            .add_node(GraphNodeBody::Gradient {
                kind: GradientKind::Linear,
                angle_deg: 0.0,
                from: gray(from_val),
                to: gray(to_val),
            })
            .unwrap();
        let blur = g.add_node(GraphNodeBody::GaussianBlur { radius }).unwrap();
        g.connect(ramp, blur, 0).unwrap();
        g.set_output(blur).unwrap();
        (*engine_of(g, 0).render_node(blur, W, H).unwrap()).clone()
    }

    #[test]
    fn cycle_detection_is_diagnostic() {
        let mut g = Graph::new();
        let a = g
            .add_node(GraphNodeBody::Blend {
                mode: BlendMode::Normal,
                opacity: 1.0,
            })
            .unwrap();
        let b = g
            .add_node(GraphNodeBody::Blend {
                mode: BlendMode::Normal,
                opacity: 1.0,
            })
            .unwrap();
        g.connect(b, a, 0).unwrap();
        g.connect(a, b, 1).unwrap();
        g.set_output(a).unwrap();
        g.validate().unwrap(); // structurally fine, cyclic at eval time
        let mut engine = engine_of(g, 0);
        match engine.render() {
            Err(GraphError::CycleDetected(path)) => {
                assert!(path.len() >= 2, "cycle path should carry diagnostics");
                assert!(path.contains(&a) && path.contains(&b));
            }
            Err(e) => panic!("expected cycle detection, got {}", e),
            Ok(_) => panic!("cycle was evaluated"),
        }
    }

    #[test]
    fn serialization_roundtrip_preserves_values_and_links() {
        let mut g = Graph::new();
        let spots = g
            .add_node(GraphNodeBody::BnWSpots {
                seed: 42,
                cells: 6,
                radius: 0.35,
                falloff: 1.0,
                invert: true,
            })
            .unwrap();
        let img = g
            .add_node(GraphNodeBody::ExternalImage {
                name: "patina".into(),
            })
            .unwrap();
        let blend = g
            .add_node(GraphNodeBody::Blend {
                mode: BlendMode::Overlay,
                opacity: 0.8,
            })
            .unwrap();
        let gmap = g
            .add_node(GraphNodeBody::GradientMap {
                stops: vec![
                    GradientStop::BLACK_WHITE_START,
                    GradientStop::BLACK_WHITE_END,
                ],
                mix: 0.9,
            })
            .unwrap();
        g.connect(spots, blend, 0).unwrap();
        g.connect(img, blend, 1).unwrap();
        g.connect(blend, gmap, 0).unwrap();
        g.set_output(gmap).unwrap();
        g.validate().unwrap();

        let json = serde_json::to_string(&g).unwrap();
        let restored: Graph = serde_json::from_str(&json).unwrap();
        assert_eq!(g, restored);
        assert_eq!(restored.output(), Some(gmap));
        match &restored.node(gmap).unwrap().inputs[0] {
            GraphSlot::Link(src) => assert_eq!(*src, blend),
            other => panic!("link lost in roundtrip: {:?}", other),
        }
        // Bodies round-trip even nested enums.
        match &restored.node(blend).unwrap().body {
            GraphNodeBody::Blend { mode, opacity } => {
                assert_eq!(*mode, BlendMode::Overlay);
                assert!((opacity - 0.8).abs() < 1e-6);
            }
            other => panic!("blend node changed: {:?}", other),
        }
    }

    #[test]
    fn memoization_and_invalidation() {
        let (g, base, out) = small_graph(3, 0);
        let mut engine = engine_of(g, 0);
        engine.render_node(out, W, H).unwrap();
        assert!(engine.cache_len() >= 2);

        // Changing only `base` must reach the output: dependents invalidated.
        engine.edit(|graph| {
            graph
                .set_body(base, GraphNodeBody::constant(BLACK))
                .unwrap();
        });
        engine.invalidate_node(base);
        assert_eq!(engine.cache_len(), 0);
        let img = engine.render_node(out, W, H).unwrap();
        // Levels maps 0 -> min_out = 0.1 (per channel, alpha included).
        assert_eq!(img[(31 * W + 31) as usize], [0.1, 0.1, 0.1, 0.9]);

        engine.invalidate_all();
        assert_eq!(engine.cache_len(), 0);
    }

    #[test]
    fn missing_inputs_are_errors_not_panics() {
        let mut g = Graph::new();
        let ext = g
            .add_node(GraphNodeBody::ExternalImage {
                name: "nope".into(),
            })
            .unwrap();
        g.set_output(ext).unwrap();
        let mut engine = engine_of(g, 0);
        assert!(matches!(engine.render(), Err(GraphError::MissingSample(_))));

        let mut g = Graph::new();
        let ghost = GraphNodeId(1234);
        let out = g.add_node(GraphNodeBody::output()).unwrap();
        g.connect(ghost, out, 0).unwrap();
        g.set_output(out).unwrap();
        let mut engine = engine_of(g, 0);
        assert!(matches!(engine.render(), Err(GraphError::MissingNode(_))));
    }

    #[test]
    fn gradient_map_colorizes_luminance() {
        let mut g = Graph::new();
        let src = g
            .add_node(GraphNodeBody::constant([0.25, 0.25, 0.25, 1.0]))
            .unwrap();
        let gmap = g
            .add_node(GraphNodeBody::GradientMap {
                stops: vec![
                    GradientStop::BLACK_WHITE_START,
                    GradientStop::BLACK_WHITE_END,
                ],
                mix: 1.0,
            })
            .unwrap();
        g.connect(src, gmap, 0).unwrap();
        g.set_output(gmap).unwrap();
        let img = engine_of(g, 0).render().unwrap();
        assert!((img[0][0] - 0.25).abs() < 1e-4);
        assert!((img[0][3] - 1.0).abs() < 1e-6);

        // Empty gradients are an error, never a panic.
        let mut g = Graph::new();
        let src = g.add_node(GraphNodeBody::constant(GRAY)).unwrap();
        let gmap = g
            .add_node(GraphNodeBody::GradientMap {
                stops: vec![],
                mix: 1.0,
            })
            .unwrap();
        g.connect(src, gmap, 0).unwrap();
        g.set_output(gmap).unwrap();
        assert!(matches!(
            engine_of(g, 0).render(),
            Err(GraphError::InvalidParams(_))
        ));
    }

    #[test]
    fn tile_generator_identity_when_params_are_neutral() {
        let mut g = Graph::new();
        let src = g
            .add_node(GraphNodeBody::constant([0.3, 0.6, 0.9, 1.0]))
            .unwrap();
        let tile = g
            .add_node(GraphNodeBody::TileGenerator {
                seed: 5,
                cols: 2,
                rows: 2,
                gap: 0.0,
                jitter: 0.0,
                zoom: 1.0,
                mortar: BLACK,
            })
            .unwrap();
        g.connect(src, tile, 0).unwrap();
        g.set_output(tile).unwrap();
        let img = engine_of(g, 0).render().unwrap();
        for px in img.iter() {
            assert!((px[0] - 0.3).abs() < 1e-5);
            assert!((px[2] - 0.9).abs() < 1e-5);
        }
    }

    #[test]
    fn render_node_convenience_is_per_resolution() {
        let (g, a, _) = small_graph(9, 0);
        let mut engine = engine_of(g, 0);
        let small = engine.render_node(a, 32, 32).unwrap();
        assert_eq!(small.len(), 32 * 32);
        let big = engine.render_node(a, 128, 64).unwrap();
        assert_eq!(big.len(), 128 * 64);
        // Separate cache lines: one resolution did not evict the other.
        assert!(engine.cache_len() >= 2);
        // Requests outside the budget are rejected, not panicked.
        assert!(matches!(
            engine.render_node(a, 9000, 9000),
            Err(GraphError::InvalidParams(_))
        ));
        assert!(matches!(
            engine.render_node(a, 0, 32),
            Err(GraphError::InvalidParams(_))
        ));
    }
}
