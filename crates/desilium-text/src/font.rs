//! System font discovery through fontdb, with lazily loaded fontdue faces per face id.

use std::collections::HashMap;

use fontdb::{Database, Family, ID, Query, Stretch, Style};
use fontdue::{Font, FontSettings};

use crate::layout::fallback_metrics;
use crate::types::{FontStack, TextError, Weight};

const WEIGHTS: [Weight; 3] = [Weight::Regular, Weight::Medium, Weight::Bold];

/// Resolved fallback chains for every weight, plus the fontdue faces loaded so far.
pub struct FontSet {
    db: Database,
    chains: HashMap<Weight, Vec<ID>>,
    /// `None` marks a face that exists but could not be parsed, so it is not retried.
    faces: HashMap<ID, Option<Font>>,
}

impl FontSet {
    /// Scans system fonts and resolves the stack. Fails when no regular-weight face is usable.
    pub fn load(stack: &FontStack) -> Result<Self, TextError> {
        let mut db = Database::new();
        db.load_system_fonts();
        let chains = WEIGHTS
            .iter()
            .map(|&weight| (weight, resolve_chain(&db, &stack.families, weight)))
            .collect();
        let mut set = Self {
            db,
            chains,
            faces: HashMap::new(),
        };
        let regular = set.chain(Weight::Regular).to_vec();
        if !regular.into_iter().any(|id| set.font(id).is_some()) {
            return Err(TextError::NoFont);
        }
        Ok(set)
    }

    /// Fallback chain for a weight; falls back to the regular chain when a weight has none.
    fn chain(&self, weight: Weight) -> &[ID] {
        self.chains
            .get(&weight)
            .or_else(|| self.chains.get(&Weight::Regular))
            .map_or(&[], Vec::as_slice)
    }

    /// Loaded fontdue face for `id`, parsing it on first use.
    pub fn font(&mut self, id: ID) -> Option<&Font> {
        let db = &self.db;
        self.faces
            .entry(id)
            .or_insert_with(|| load_face(db, id))
            .as_ref()
    }

    /// First face in the chain that contains `ch`, with the glyph index inside that face.
    pub fn glyph_for_char(&mut self, weight: Weight, ch: char) -> Option<(ID, u16)> {
        let count = self.chain(weight).len();
        (0..count).find_map(|position| {
            let id = *self.chain(weight).get(position)?;
            let index = self.font(id)?.lookup_glyph_index(ch);
            (index != 0).then_some((id, index))
        })
    }

    /// Horizontal advance of a glyph in pixels at `px`.
    pub fn advance(&mut self, id: ID, glyph: u16, px: f32) -> f32 {
        self.font(id)
            .map_or(0.0, |font| font.metrics_indexed(glyph, px).advance_width)
    }

    /// Kerning between two adjacent glyphs of the same face, in pixels at `px`.
    pub fn kern(&mut self, id: ID, left: u16, right: u16, px: f32) -> f32 {
        self.font(id)
            .and_then(|font| font.horizontal_kern_indexed(left, right, px))
            .unwrap_or(0.0)
    }

    /// Ascent and descent of the primary face for `weight`, in pixels at `px`.
    pub fn line_metrics(&mut self, weight: Weight, px: f32) -> (f32, f32) {
        let primary = self.chain(weight).first().copied();
        let metrics = primary
            .and_then(|id| self.font(id))
            .and_then(|font| font.horizontal_line_metrics(px));
        metrics.map_or_else(|| fallback_metrics(px), |m| (m.ascent, m.descent))
    }
}

/// Builds the ordered, de-duplicated fallback chain: named families, then one generic sans face.
fn resolve_chain(db: &Database, families: &[String], weight: Weight) -> Vec<ID> {
    let face_weight = fontdb::Weight(weight.css_value());
    let named = families
        .iter()
        .filter_map(|name| query(db, Family::Name(name.as_str()), face_weight));
    let generic = query(db, Family::SansSerif, face_weight).or_else(|| any_upright_face(db));
    let mut chain: Vec<ID> = Vec::new();
    for id in named.chain(generic) {
        if !chain.contains(&id) {
            chain.push(id);
        }
    }
    chain
}

fn query(db: &Database, family: Family<'_>, weight: fontdb::Weight) -> Option<ID> {
    db.query(&Query {
        families: &[family],
        weight,
        stretch: Stretch::Normal,
        style: Style::Normal,
    })
}

/// Last resort when no generic sans family is configured: any upright, proportional face.
fn any_upright_face(db: &Database) -> Option<ID> {
    db.faces()
        .find(|face| face.style == Style::Normal && !face.monospaced)
        .map(|face| face.id)
}

fn load_face(db: &Database, id: ID) -> Option<Font> {
    db.with_face_data(id, |data, index| {
        let settings = FontSettings {
            collection_index: index,
            ..FontSettings::default()
        };
        Font::from_bytes(data, settings).ok()
    })
    .flatten()
}
