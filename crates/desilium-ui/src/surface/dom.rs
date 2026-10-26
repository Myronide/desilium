//! Scripted-free DOM updates: the shell state changes, Rust patches the document.
//!
//! Selectors are limited to `#id`, `.class`, `[data-key]` and `[data-key=value]`
//! (see [`crate::selector`]). Every operation applies to ALL matching elements
//! and returns [`UiError::NoMatch`] when nothing matches. Nothing is restyled
//! until the next render, so any number of changes cost one style/layout pass;
//! [`Updates`] additionally saves repeated lookups and reports misses together.

use blitz_dom::{DocumentMutator, LocalName, NodeId, QualName, local_name, ns};

use super::UiSurface;
use crate::{UiError, selector::Selector};

#[derive(Debug, Clone)]
enum Op {
    Text(String, String),
    Attr(String, String, String),
    Class(String, String, bool),
    StyleVar(String, String, String),
    Children(String, String),
}

/// A batch of DOM changes applied together with [`UiSurface::apply`].
#[derive(Debug, Clone, Default)]
pub struct Updates {
    ops: Vec<Op>,
}

impl Updates {
    /// Empty batch.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the text content of the matches.
    pub fn text(mut self, selector: &str, text: &str) -> Self {
        self.ops.push(Op::Text(selector.into(), text.into()));
        self
    }

    /// Sets an attribute on the matches.
    pub fn attr(mut self, selector: &str, name: &str, value: &str) -> Self {
        self.ops
            .push(Op::Attr(selector.into(), name.into(), value.into()));
        self
    }

    /// Adds or removes a class on the matches.
    pub fn class(mut self, selector: &str, class: &str, on: bool) -> Self {
        self.ops.push(Op::Class(selector.into(), class.into(), on));
        self
    }

    /// Sets a CSS custom property (`--name`) on the matches.
    pub fn style_var(mut self, selector: &str, name: &str, value: &str) -> Self {
        self.ops
            .push(Op::StyleVar(selector.into(), name.into(), value.into()));
        self
    }

    /// Replaces the children of the matches with parsed `html`.
    pub fn children(mut self, selector: &str, html: &str) -> Self {
        self.ops.push(Op::Children(selector.into(), html.into()));
        self
    }

    /// Number of queued changes.
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    /// True when nothing is queued.
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

fn attr_name(name: &str) -> QualName {
    QualName::new(None, ns!(), LocalName::from(name))
}

fn valid_attr_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':'))
}

fn valid_var_name(name: &str) -> bool {
    name.starts_with("--") && valid_attr_name(name)
}

fn class_list(current: Option<&str>) -> Vec<String> {
    current
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

impl UiSurface {
    pub(super) fn find_all(&self, selector: &str) -> Result<Vec<NodeId>, UiError> {
        let parsed = Selector::parse(selector)?;
        let found = match &parsed {
            Selector::Id(id) => self.doc.get_element_by_id(id).into_iter().collect(),
            other => self
                .doc
                .query_selector_all(&other.to_css())
                .map(|ids| ids.to_vec())
                .map_err(|_| UiError::InvalidSelector(selector.to_owned()))?,
        };
        if found.is_empty() {
            return Err(UiError::NoMatch(selector.to_owned()));
        }
        Ok(found)
    }

    /// Replaces the text content of every match.
    pub fn set_text(&mut self, selector: &str, text: &str) -> Result<(), UiError> {
        self.apply(Updates::new().text(selector, text))
    }

    /// Sets an attribute on every match.
    pub fn set_attr(&mut self, selector: &str, name: &str, value: &str) -> Result<(), UiError> {
        self.apply(Updates::new().attr(selector, name, value))
    }

    /// Adds (`on`) or removes `class` on every match.
    pub fn set_class(&mut self, selector: &str, class: &str, on: bool) -> Result<(), UiError> {
        self.apply(Updates::new().class(selector, class, on))
    }

    /// Sets the CSS custom property `name` (including the leading `--`) on every match.
    pub fn set_style_var(
        &mut self,
        selector: &str,
        name: &str,
        value: &str,
    ) -> Result<(), UiError> {
        self.apply(Updates::new().style_var(selector, name, value))
    }

    /// Sets a custom property on the root element, e.g. to retint a whole surface.
    pub fn set_root_style_var(&mut self, name: &str, value: &str) {
        let Some(root) = self.doc.try_root_element().map(|node| node.id) else {
            return;
        };
        if valid_var_name(name) {
            self.doc.mutate().set_style_property(root, name, value);
            self.mark_dirty();
        }
    }

    /// Replaces the children of every match with the parsed `html`.
    pub fn replace_children(&mut self, selector: &str, html: &str) -> Result<(), UiError> {
        self.apply(Updates::new().children(selector, html))
    }

    /// Applies a batch. All changes are made even when some selectors miss; the
    /// first error is returned afterwards.
    pub fn apply(&mut self, updates: Updates) -> Result<(), UiError> {
        let mut first_error = None;
        for op in updates.ops {
            if let Err(error) = self.apply_op(&op) {
                tracing::warn!(%error, "dom update skipped");
                first_error.get_or_insert(error);
            }
        }
        self.mark_dirty();
        first_error.map_or(Ok(()), Err)
    }

    fn apply_op(&mut self, op: &Op) -> Result<(), UiError> {
        let selector = match op {
            Op::Text(s, _)
            | Op::Attr(s, ..)
            | Op::Class(s, ..)
            | Op::StyleVar(s, ..)
            | Op::Children(s, _) => s,
        };
        let ids = self.find_all(selector)?;
        let mut mutator = self.doc.mutate();
        for id in ids {
            apply_to_node(&mut mutator, id, op)?;
        }
        Ok(())
    }
}

fn apply_to_node(mutator: &mut DocumentMutator<'_>, id: NodeId, op: &Op) -> Result<(), UiError> {
    match op {
        Op::Text(_, text) => replace_text(mutator, id, text),
        Op::Attr(_, name, value) => {
            if !valid_attr_name(name) {
                return Err(UiError::InvalidSelector(name.clone()));
            }
            mutator.set_attribute(id, attr_name(name), value);
        }
        Op::Class(_, class, on) => toggle_class(mutator, id, class, *on),
        Op::StyleVar(_, name, value) => {
            if !valid_var_name(name) {
                return Err(UiError::InvalidSelector(name.clone()));
            }
            mutator.set_style_property(id, name, value);
        }
        Op::Children(_, html) => mutator.set_inner_html(id, html),
    }
    Ok(())
}

fn replace_text(mutator: &mut DocumentMutator<'_>, id: NodeId, text: &str) {
    mutator.remove_and_drop_all_children(id);
    let child = mutator.create_text_node(text);
    mutator.append_children(id, &[child]);
}

fn toggle_class(mutator: &mut DocumentMutator<'_>, id: NodeId, class: &str, on: bool) {
    let current = mutator
        .doc
        .get_node(id)
        .and_then(|node| node.attr(local_name!("class")))
        .map(|value| class_list(Some(value)))
        .unwrap_or_default();
    let mut classes = current.clone();
    classes.retain(|existing| existing != class);
    if on {
        classes.push(class.to_owned());
    }
    if classes == current {
        return;
    }
    mutator.set_attribute(id, attr_name("class"), &classes.join(" "));
}
