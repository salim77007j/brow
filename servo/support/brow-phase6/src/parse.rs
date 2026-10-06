/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HTML5 parsing statistics through html5ever — the same parser crate the
//! vendored engine uses for tree construction.
//!
//! The sink is an arena-based counting tree builder: it tracks node count,
//! tree depth, element count and text volume without materializing a full
//! DOM. Element names ARE tracked (the tree builder's spec algorithm queries
//! `elem_name` on the stack of open elements, so name fidelity is required
//! for identical tree-construction behavior); interned atoms make cloning
//! them out of the arena cheap.

use std::cell::RefCell;
use std::io::Read;
use std::time::Instant;

use html5ever::interface::{ElemName, ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::tendril::{StrTendril, TendrilSink};
use html5ever::{parse_document, Attribute, LocalName, Namespace, ParseOpts, QualName};

use crate::metrics::ParseStats;

/// Owned (atom-backed) element name handed back to the tree builder.
#[derive(Clone, Debug)]
struct OwnedElemName {
    ns: Namespace,
    local: LocalName,
}

impl ElemName for OwnedElemName {
    fn ns(&self) -> &Namespace {
        &self.ns
    }
    fn local_name(&self) -> &LocalName {
        &self.local
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct Counters {
    /// element + comment + PI + doctype + document nodes
    nodes: u64,
    elements: u64,
    text_chars: u64,
    max_depth: u32,
}

#[derive(Debug, Default)]
struct NodeData {
    parent: Option<usize>,
    name: Option<(Namespace, LocalName)>,
    children: Vec<usize>,
    depth: u32,
}

/// Arena-based counting sink. All TreeSink methods take `&self` in
/// html5ever 0.39, so mutable state lives behind RefCells.
struct CountingSink {
    arena: RefCell<Vec<NodeData>>,
    counters: RefCell<Counters>,
    document: usize,
}

impl CountingSink {
    fn new() -> Self {
        let document = 0;
        CountingSink {
            arena: RefCell::new(vec![NodeData::default()]),
            counters: RefCell::new(Counters::default()),
            document,
        }
    }

    fn alloc(&self, name: Option<(Namespace, LocalName)>) -> usize {
        let id = {
            let mut arena = self.arena.borrow_mut();
            arena.push(NodeData {
                name,
                ..Default::default()
            });
            arena.len() - 1
        };
        self.counters.borrow_mut().nodes += 1;
        id
    }

    fn attach(&self, parent: usize, id: usize) {
        let mut arena = self.arena.borrow_mut();
        arena[id].parent = Some(parent);
        arena[id].depth = arena[parent].depth + 1;
        arena[parent].children.push(id);
        let d = arena[id].depth;
        drop(arena);
        let cur = self.counters.borrow().max_depth;
        self.counters.borrow_mut().max_depth = cur.max(d);
    }

    /// Recompute depths for the subtree rooted at `root` (after reparenting).
    fn recompute_subtree_depths(&self, root: usize) {
        let mut arena = self.arena.borrow_mut();
        let base = arena[root].depth;
        let mut stack: Vec<usize> = Vec::new();
        for c in arena[root].children.clone() {
            stack.push(c);
        }
        while let Some(id) = stack.pop() {
            let d = arena[id].parent.map(|p| arena[p].depth + 1).unwrap_or(base + 1);
            arena[id].depth = d;
            let kids = arena[id].children.clone();
            for k in kids {
                stack.push(k);
            }
        }
        let max = arena.iter().map(|n| n.depth).max().unwrap_or(0);
        drop(arena);
        let cur = self.counters.borrow().max_depth;
        self.counters.borrow_mut().max_depth = cur.max(max);
    }
}

impl TreeSink for CountingSink {
    type Handle = usize;
    type Output = Counters;
    type ElemName<'a> = OwnedElemName
    where
        Self: 'a;

    fn finish(self) -> Counters {
        self.counters.into_inner()
    }

    fn parse_error(&self, _msg: std::borrow::Cow<'static, str>) {}

    fn get_document(&self) -> usize {
        self.document
    }

    fn elem_name<'a>(&'a self, target: &'a usize) -> OwnedElemName {
        let arena = self.arena.borrow();
        let (ns, local) = arena[*target]
            .name
            .clone()
            .expect("elem_name called on non-element node");
        OwnedElemName { ns, local }
    }

    fn create_element(
        &self,
        name: QualName,
        _attrs: Vec<Attribute>,
        _flags: ElementFlags,
    ) -> usize {
        let id = self.alloc(Some((name.ns, name.local)));
        self.counters.borrow_mut().elements += 1;
        id
    }

    fn create_comment(&self, _text: StrTendril) -> usize {
        self.alloc(None)
    }

    fn create_pi(&self, _target: StrTendril, _data: StrTendril) -> usize {
        self.alloc(None)
    }

    fn append(&self, parent: &usize, child: NodeOrText<usize>) {
        match child {
            NodeOrText::AppendNode(id) => {
                // fresh nodes only, per the trait contract
                self.attach(*parent, id);
            }
            NodeOrText::AppendText(tendril) => {
                self.counters.borrow_mut().text_chars += tendril.len() as u64;
                // text depth contributes to max_depth as a virtual node
                let arena = self.arena.borrow();
                let d = arena[*parent].depth + 1;
                drop(arena);
                let cur = self.counters.borrow().max_depth;
                self.counters.borrow_mut().max_depth = cur.max(d);
            }
        }
    }

    fn append_based_on_parent_node(
        &self,
        element: &usize,
        prev_element: &usize,
        child: NodeOrText<usize>,
    ) {
        // Foster-parenting insertion point: use `element`'s parent when it
        // has one, otherwise the provided fallback element.
        let parent = self.arena.borrow()[*element].parent.unwrap_or(*prev_element);
        self.append(&parent, child);
    }

    fn append_doctype_to_document(
        &self,
        _name: StrTendril,
        _public_id: StrTendril,
        _system_id: StrTendril,
    ) {
        // Counted without a handle: doctypes have no further tree role.
        self.counters.borrow_mut().nodes += 1;
    }

    fn get_template_contents(&self, target: &usize) -> usize {
        *target
    }

    fn same_node(&self, x: &usize, y: &usize) -> bool {
        x == y
    }

    fn set_quirks_mode(&self, _mode: QuirksMode) {}

    fn append_before_sibling(&self, sibling: &usize, new_node: NodeOrText<usize>) {
        let parent = self.arena.borrow()[*sibling].parent.unwrap_or(self.document);
        match new_node {
            NodeOrText::AppendNode(id) => self.attach(parent, id),
            NodeOrText::AppendText(tendril) => {
                self.counters.borrow_mut().text_chars += tendril.len() as u64;
            }
        }
    }

    fn add_attrs_if_missing(&self, _target: &usize, _attrs: Vec<Attribute>) {}

    fn remove_from_parent(&self, target: &usize) {
        let mut arena = self.arena.borrow_mut();
        if let Some(p) = arena[*target].parent.take() {
            arena[p].children.retain(|c| c != target);
        }
    }

    fn reparent_children(&self, node: &usize, new_parent: &usize) {
        let mut arena = self.arena.borrow_mut();
        let moved = std::mem::take(&mut arena[*node].children);
        for m in &moved {
            arena[*m].parent = Some(*new_parent);
        }
        arena[*new_parent].children.extend(moved);
        drop(arena);
        // depths may shift when the new parent sits at a different level
        self.recompute_subtree_depths(*new_parent);
    }

    fn is_mathml_annotation_xml_integration_point(&self, handle: &usize) -> bool {
        // The counting sink does not retain per-element flags; returning the
        // default (false) matches plain-HTML documents, which is what the
        // benchmark fetches. Recorded as a known simplification.
        let _ = handle;
        false
    }
}

/// Parse an HTML byte stream and return tree-construction statistics.
pub fn parse_html_stats<R: Read>(input: &mut R) -> Result<ParseStats, std::io::Error> {
    let start = Instant::now();
    let sink = CountingSink::new();
    let counters = parse_document(sink, ParseOpts::default())
        .from_utf8()
        .read_from(input)?;
    let elapsed_us = start.elapsed().as_micros() as u64;
    let nodes_per_sec = if elapsed_us > 0 {
        counters.nodes * 1_000_000 / elapsed_us
    } else {
        0
    };
    Ok(ParseStats {
        nodes: counters.nodes,
        max_depth: counters.max_depth,
        elements: counters.elements,
        text_chars: counters.text_chars,
        parse_us: elapsed_us,
        nodes_per_sec,
    })
}

/// Parse an in-memory HTML document.
pub fn parse_html_bytes(html: &[u8]) -> std::io::Result<ParseStats> {
    parse_html_stats(&mut std::io::Cursor::new(html))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_document_counts() {
        let html = b"<!doctype html><html><head><title>t</title></head>\
                      <body><p>hello</p><div><span>world</span></div></body></html>";
        let s = parse_html_bytes(html).unwrap();
        // document + doctype + html + head + title + text + body + p + text
        // + div + span + text = 11 tree nodes (text counted separately)
        assert!(s.nodes >= 8, "expected at least 8 nodes, got {}", s.nodes);
        assert_eq!(s.elements, 7, "html head title body p div span: {}", s.elements);
        assert!(s.text_chars >= 10, "expected >= 10 text chars, got {}", s.text_chars);
        assert!(s.max_depth >= 4, "html>body>div>span = depth 4+, got {}", s.max_depth);
    }

    #[test]
    fn deep_nesting_depth_accounting() {
        let mut html = String::from("<html><body>");
        for _ in 0..50 {
            html.push_str("<div>");
        }
        html.push_str("x");
        for _ in 0..50 {
            html.push_str("</div>");
        }
        html.push_str("</body></html>");
        let s = parse_html_bytes(html.as_bytes()).unwrap();
        assert!(s.max_depth >= 50, "expected depth >= 50, got {}", s.max_depth);
        assert!(s.nodes_per_sec > 0);
    }

    #[test]
    fn malformed_html_still_parses() {
        // html5ever is error-recovering; the sink must never panic.
        let html = b"<p>unclosed <b>bold <i>italic<div>weird";
        let s = parse_html_bytes(html).unwrap();
        assert!(s.nodes > 0);
    }

    #[test]
    fn tables_trigger_foster_paths_without_panic() {
        // Foster parenting exercises append_based_on_parent_node and
        // reparent_children in the tree builder.
        let html = b"<table><b>fostered</b><tr><td>cell</td></tr></table><form><input><form>";
        let s = parse_html_bytes(html).unwrap();
        assert!(s.elements >= 5, "elements: {}", s.elements);
    }

    #[test]
    fn template_contents_and_comments() {
        let html = b"<template><div>in template</div></template><!-- comment --><?pi data>";
        let s = parse_html_bytes(html).unwrap();
        assert!(s.elements >= 2);
        assert!(s.nodes >= s.elements + 2, "comment + template div: {}", s.nodes);
    }

    #[test]
    fn empty_and_binary_inputs_are_safe() {
        let s = parse_html_bytes(b"").unwrap();
        assert!(s.nodes >= 1, "document node always exists");
        let s = parse_html_bytes(&[0x00, 0xff, 0xfe, 0x01]).unwrap();
        assert!(s.nodes >= 1);
    }
}
