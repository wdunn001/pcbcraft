//! The board is a free-form 2D shape: its extent comes from the outline and
//! every main (copper) layer may have a shape of its own.
//!
//! `Board.outline` is the default shape. A copper layer follows it until it
//! gets an entry in `Board.layer_shapes`. Layer shapes are independent: they
//! may differ completely and overlap, so there is no containment rule.
//! Silkscreen, solder mask and paste have no shape of their own; they follow
//! the shape of their parent copper layer. `Board.width` and `Board.height`
//! are a cache of the union extent, kept in step by `Board::refresh_extent`.
//!
//! # Dependencies
//! - crates/doc/src/outline.rs
//! - crates/doc/src/lib.rs

use crate::{Board, outline_extent};
use pcbcraft_geom::Point;
use serde::{Deserialize, Serialize};

/// The own shape of one main (copper) layer, in millimetres.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct LayerShape {
    /// Closed polygon vertices, ordered around the perimeter.
    pub outline: Vec<Point>,
    /// Arc midpoints, one entry per segment, the same form as `Board.outline_arcs`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arcs: Vec<Option<Point>>,
}

/// What a layer id names: a main layer, or a sub-layer of one.
#[derive(Clone, Debug, PartialEq)]
pub enum LayerRef {
    /// A copper layer, by its position-based key (`F.Cu`, `In1.Cu`, `B.Cu`).
    Copper(String),
    /// A sub-layer that follows `parent`; `label` is its plain name.
    Sub {
        /// Plain name of the kind of sub-layer, for messages.
        label: &'static str,
        /// Key of the copper layer it follows.
        parent: String,
    },
}

impl Board {
    /// Number of copper layers; two when no stack rows are recorded.
    pub fn copper_count(&self) -> u8 {
        let n = self
            .layer_records
            .iter()
            .filter(|r| r.kind == "copper")
            .count();
        if n == 0 {
            2
        } else {
            n.min(crate::MAX_COPPER_LAYERS as usize) as u8
        }
    }

    /// Position-based key of copper layer `index` (`F.Cu`, `In<i>.Cu`, `B.Cu`).
    pub fn copper_key(&self, index: u8) -> String {
        if index == 0 {
            "F.Cu".into()
        } else if index + 1 >= self.copper_count() {
            "B.Cu".into()
        } else {
            format!("In{index}.Cu")
        }
    }

    /// Index of the copper layer a name or key refers to.
    pub fn resolve_copper_index(&self, s: &str) -> Result<u8, String> {
        let s = s.trim();
        if s.is_empty() {
            return Err("copper layer name is empty".into());
        }
        let count = self.copper_count();
        if s == "F.Cu" {
            return Ok(0);
        }
        if s == "B.Cu" {
            return Ok(if count <= 2 { 1 } else { count - 1 });
        }
        if let Some(rest) = s.strip_prefix('L')
            && let Ok(n) = rest.parse::<u32>()
            && n >= 1
            && n <= count as u32
        {
            return Ok((n - 1) as u8);
        }
        if let Some(rest) = s.strip_prefix("In").and_then(|r| r.strip_suffix(".Cu"))
            && let Ok(n) = rest.parse::<u8>()
            && n < count
        {
            return Ok(n);
        }
        for (i, rec) in self
            .layer_records
            .iter()
            .filter(|r| r.kind == "copper")
            .enumerate()
        {
            if rec.name == s {
                return Ok(i as u8);
            }
        }
        Err(format!("unknown copper layer {s:?}"))
    }

    /// Read a layer id as a main layer or as a sub-layer of one.
    ///
    /// # Rules
    /// - Sub-layers are silkscreen, solder mask and paste; stack rows of those
    ///   kinds and the `F.`/`B.` names both count.
    /// - A sub-layer resolves to the copper layer on its face.
    pub fn layer_ref(&self, id: &str) -> Result<LayerRef, String> {
        let id = id.trim();
        let top = self.copper_key(0);
        let bottom = self.copper_key(self.copper_count().saturating_sub(1));
        let label_of = |text: &str| -> Option<&'static str> {
            let t = text.to_ascii_lowercase();
            if t.contains("silk") {
                Some("Silkscreen")
            } else if t.contains("mask") {
                Some("Solder mask")
            } else if t.contains("paste") {
                Some("Paste")
            } else {
                None
            }
        };
        let rows = &self.layer_records;
        if let Some(i) = rows.iter().position(|r| r.name == id && r.kind != "copper") {
            let kind_label = match rows[i].kind.as_str() {
                "silkscreen" => Some("Silkscreen"),
                "soldermask" => Some("Solder mask"),
                "paste" => Some("Paste"),
                _ => None,
            };
            if let Some(label) = kind_label {
                let first = rows.iter().position(|r| r.kind == "copper");
                let on_top = match rows[i].side.as_deref() {
                    Some("top") => true,
                    Some("bottom") => false,
                    _ => first.is_none_or(|f| i < f),
                };
                let parent = if on_top { top } else { bottom };
                return Ok(LayerRef::Sub { label, parent });
            }
        }
        if let Some(label) = label_of(id)
            && !id.ends_with(".Cu")
        {
            let parent = if id.starts_with("B.") { bottom } else { top };
            return Ok(LayerRef::Sub { label, parent });
        }
        let index = self.resolve_copper_index(id)?;
        Ok(LayerRef::Copper(self.copper_key(index)))
    }

    /// The shape a layer has: its own when one is set, else the board outline.
    /// A sub-layer id gives its parent copper layer's shape. An unknown layer
    /// follows the board. Empty slices mean the board has no traced outline.
    ///
    /// # Rules
    /// - Read this, never `outline` directly, where the layer matters.
    pub fn shape_of(&self, layer: &str) -> (&[Point], &[Option<Point>]) {
        let key = match self.layer_ref(layer) {
            Ok(LayerRef::Copper(k)) | Ok(LayerRef::Sub { parent: k, .. }) => Some(k),
            Err(_) => None,
        };
        if let Some(own) = key.as_ref().and_then(|k| self.layer_shapes.get(k)) {
            return (&own.outline, &own.arcs);
        }
        match &self.outline {
            Some(points) => (points, &self.outline_arcs),
            None => (&[], &[]),
        }
    }

    /// Smallest and largest corner of the union of the board outline and
    /// every layer's own shape, arcs included. A board with no traced outline
    /// and no layer shape reaches from the origin to `width` by `height`.
    pub fn extent(&self) -> (Point, Point) {
        let mut found: Option<(Point, Point)> = None;
        let mut take = |lo: Point, hi: Point| {
            found = Some(match found {
                None => (lo, hi),
                Some((a, b)) => (
                    Point::new(a.x.min(lo.x), a.y.min(lo.y)),
                    Point::new(b.x.max(hi.x), b.y.max(hi.y)),
                ),
            });
        };
        if let Some(points) = &self.outline
            && let Some((lo, hi)) = outline_extent(points, &self.outline_arcs)
        {
            take(lo, hi);
        }
        for shape in self.layer_shapes.values() {
            if let Some((lo, hi)) = outline_extent(&shape.outline, &shape.arcs) {
                take(lo, hi);
            }
        }
        found.unwrap_or((Point::new(0.0, 0.0), Point::new(self.width, self.height)))
    }

    /// Width and height of the extent, read only.
    pub fn extent_size(&self) -> (f64, f64) {
        let (lo, hi) = self.extent();
        (hi.x - lo.x, hi.y - lo.y)
    }

    /// Bring the `width` and `height` cache in step with the shape.
    ///
    /// # Rules
    /// - The only place that writes `width` and `height`; every command that
    ///   changes a shape calls it.
    /// - The board sits on the origin, so the cache is the far corner of the extent.
    /// - A board with no traced outline and no layer shape keeps its numbers.
    pub fn refresh_extent(&mut self) {
        if self.outline.is_none() && self.layer_shapes.is_empty() {
            return;
        }
        let (_, hi) = self.extent();
        self.width = hi.x;
        self.height = hi.y;
    }

    /// Give a board with no outline a rectangle outline of its width and height.
    pub fn ensure_outline(&mut self) {
        if self.outline.is_none() {
            self.outline = Some(rectangle_outline(self.width, self.height));
            self.outline_arcs.clear();
        }
    }

    /// One row per copper layer in stack order, top first: the layer key,
    /// whether it follows the board outline, and its corner count.
    pub fn layer_shapes_in_order(&self) -> Vec<(String, bool, usize)> {
        (0..self.copper_count())
            .map(|i| {
                let key = self.copper_key(i);
                let own = self.layer_shapes.get(&key);
                let corners = match own {
                    Some(s) => s.outline.len(),
                    None => self.outline.as_ref().map_or(0, Vec::len),
                };
                (key, own.is_none(), corners)
            })
            .collect()
    }
}

/// The four corners of a rectangle on the origin, counter-clockwise.
pub fn rectangle_outline(width: f64, height: f64) -> Vec<Point> {
    vec![
        Point::new(0.0, 0.0),
        Point::new(width, 0.0),
        Point::new(width, height),
        Point::new(0.0, height),
    ]
}
