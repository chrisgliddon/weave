//! Deterministic Weave graph model, spatial index, and `gpui-flow` adapter.

use std::collections::{BTreeSet, HashMap};

use gpui_flow::{
    Connection, EdgeType, FlowEdge, FlowNode, FlowPoint, HandleDef, HandlePosition, Viewport,
};
use weave_core::ast::{Document, Item, Span, Spanned, Statement};

/// Interactive frame budget used by graph performance tests and diagnostics.
pub const INTERACTIVE_FRAME_BUDGET_MS: f64 = 16.67;

const CELL_SIZE: f32 = 320.0;
const NODE_WIDTH: f32 = 220.0;
const NODE_HEIGHT: f32 = 92.0;

/// Stable node categories exposed by Weave source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GraphNodeKind {
    Knot,
    Choice,
    Grammar,
    Pattern,
    Variable,
}

impl GraphNodeKind {
    /// Identifier used to route a node to a `gpui-flow` renderer.
    #[must_use]
    pub const fn renderer_key(self) -> &'static str {
        match self {
            Self::Knot => "knot",
            Self::Choice => "choice",
            Self::Grammar => "grammar",
            Self::Pattern => "pattern",
            Self::Variable => "variable",
        }
    }
}

/// Stable edge categories exposed by Weave source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GraphEdgeKind {
    Choice,
    Divert,
    Thread,
}

/// Position in graph coordinates.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GraphPoint {
    pub x: f32,
    pub y: f32,
}

impl GraphPoint {
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// Axis-aligned graph-space rectangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GraphRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl GraphRect {
    fn intersects(self, other: Self) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }

    fn contains(self, point: GraphPoint) -> bool {
        point.x >= self.x
            && point.x <= self.x + self.width
            && point.y >= self.y
            && point.y <= self.y + self.height
    }
}

/// One graph node backed by a source construct.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphNode {
    pub id: String,
    pub kind: GraphNodeKind,
    pub label: String,
    pub preview: String,
    pub position: GraphPoint,
    pub width: f32,
    pub height: f32,
    pub source_span: Option<Span>,
    pub validation: Option<String>,
}

impl GraphNode {
    fn bounds(&self) -> GraphRect {
        GraphRect {
            x: self.position.x,
            y: self.position.y,
            width: self.width,
            height: self.height,
        }
    }
}

/// One semantic connection between source constructs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphEdge {
    pub id: String,
    pub kind: GraphEdgeKind,
    pub source: String,
    pub target: String,
    pub label: Option<String>,
}

/// Pan and zoom state independent of a platform window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GraphViewport {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
}

impl Default for GraphViewport {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
        }
    }
}

impl GraphViewport {
    /// Convert graph coordinates to screen coordinates.
    #[must_use]
    pub fn graph_to_screen(self, point: GraphPoint) -> GraphPoint {
        GraphPoint::new(point.x * self.zoom + self.x, point.y * self.zoom + self.y)
    }

    /// Convert screen coordinates to graph coordinates.
    #[must_use]
    pub fn screen_to_graph(self, point: GraphPoint) -> GraphPoint {
        GraphPoint::new(
            (point.x - self.x) / self.zoom,
            (point.y - self.y) / self.zoom,
        )
    }

    /// Pan by a logical-pixel delta.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.x += dx;
        self.y += dy;
    }

    /// Zoom around one screen-space anchor while keeping its graph point fixed.
    pub fn zoom_at(&mut self, factor: f32, anchor: GraphPoint) {
        let previous = self.zoom;
        self.zoom = (self.zoom * factor).clamp(0.2, 4.0);
        self.x = anchor.x - (anchor.x - self.x) * (self.zoom / previous);
        self.y = anchor.y - (anchor.y - self.y) * (self.zoom / previous);
    }
}

/// Cardinal direction used for keyboard navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationDirection {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone)]
enum GraphMutation {
    Move {
        ids: Vec<String>,
        before: Vec<GraphPoint>,
        after: Vec<GraphPoint>,
    },
    AddNode(GraphNode),
    Remove {
        nodes: Vec<GraphNode>,
        edges: Vec<GraphEdge>,
    },
    AddEdge(GraphEdge),
}

#[derive(Debug, Clone, Default)]
struct SpatialIndex {
    cells: HashMap<(i32, i32), Vec<usize>>,
}

impl SpatialIndex {
    fn rebuild(nodes: &[GraphNode]) -> Self {
        let mut index = Self::default();
        for (node_index, node) in nodes.iter().enumerate() {
            let bounds = node.bounds();
            let min_x = cell(bounds.x);
            let max_x = cell(bounds.x + bounds.width);
            let min_y = cell(bounds.y);
            let max_y = cell(bounds.y + bounds.height);
            for x in min_x..=max_x {
                for y in min_y..=max_y {
                    index.cells.entry((x, y)).or_default().push(node_index);
                }
            }
        }
        index
    }

    fn candidates(&self, area: GraphRect) -> Vec<usize> {
        let min_x = cell(area.x);
        let max_x = cell(area.x + area.width);
        let min_y = cell(area.y);
        let max_y = cell(area.y + area.height);
        let mut candidates = BTreeSet::new();
        for x in min_x..=max_x {
            for y in min_y..=max_y {
                if let Some(indices) = self.cells.get(&(x, y)) {
                    candidates.extend(indices.iter().copied());
                }
            }
        }
        candidates.into_iter().collect()
    }
}

fn cell(value: f32) -> i32 {
    (value / CELL_SIZE).floor() as i32
}

/// Canonical editor graph state derived from the source AST.
#[derive(Debug, Clone)]
pub struct GraphDocument {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    node_lookup: HashMap<String, usize>,
    spatial: SpatialIndex,
    selection: BTreeSet<String>,
    viewport: GraphViewport,
    undo: Vec<GraphMutation>,
    redo: Vec<GraphMutation>,
    next_user_node: u64,
}

impl GraphDocument {
    /// Build a deterministic graph from a parsed source document.
    #[must_use]
    pub fn from_ast(document: &Document) -> Self {
        let mut builder = GraphBuilder::default();
        builder.add_top_level_nodes(document);
        builder.add_statement_nodes_and_edges(document);
        let mut graph = Self::new(builder.nodes, builder.edges);
        graph.next_user_node = graph.nodes.len() as u64;
        graph
    }

    /// Build graph state from explicit nodes and edges.
    #[must_use]
    pub fn new(nodes: Vec<GraphNode>, edges: Vec<GraphEdge>) -> Self {
        let mut graph = Self {
            nodes,
            edges,
            node_lookup: HashMap::new(),
            spatial: SpatialIndex::default(),
            selection: BTreeSet::new(),
            viewport: GraphViewport::default(),
            undo: Vec::new(),
            redo: Vec::new(),
            next_user_node: 0,
        };
        graph.reindex();
        graph
    }

    #[must_use]
    pub fn nodes(&self) -> &[GraphNode] {
        &self.nodes
    }

    #[must_use]
    pub fn edges(&self) -> &[GraphEdge] {
        &self.edges
    }

    #[must_use]
    pub const fn viewport(&self) -> GraphViewport {
        self.viewport
    }

    pub fn set_viewport(&mut self, viewport: GraphViewport) {
        self.viewport = GraphViewport {
            zoom: viewport.zoom.clamp(0.2, 4.0),
            ..viewport
        };
    }

    #[must_use]
    pub fn selected(&self) -> &BTreeSet<String> {
        &self.selection
    }

    /// Replace selection, ignoring unknown or non-selectable identifiers.
    pub fn select<I, S>(&mut self, ids: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.selection = ids
            .into_iter()
            .filter_map(|id| {
                let id = id.as_ref();
                self.node_lookup.contains_key(id).then(|| id.to_owned())
            })
            .collect();
    }

    /// Select all nodes that intersect a graph-space box.
    pub fn box_select(&mut self, area: GraphRect, additive: bool) {
        if !additive {
            self.selection.clear();
        }
        for index in self.spatial.candidates(area) {
            let node = &self.nodes[index];
            if node.bounds().intersects(area) {
                self.selection.insert(node.id.clone());
            }
        }
    }

    /// Find the topmost node at one graph-space point.
    #[must_use]
    pub fn hit_test(&self, point: GraphPoint) -> Option<&GraphNode> {
        let area = GraphRect {
            x: point.x,
            y: point.y,
            width: 0.01,
            height: 0.01,
        };
        self.spatial
            .candidates(area)
            .into_iter()
            .rev()
            .map(|index| &self.nodes[index])
            .find(|node| node.bounds().contains(point))
    }

    /// Query only nodes whose bounds intersect the visible viewport.
    #[must_use]
    pub fn visible_nodes(
        &self,
        screen_width: f32,
        screen_height: f32,
        margin: f32,
    ) -> Vec<&GraphNode> {
        let top_left = self
            .viewport
            .screen_to_graph(GraphPoint::new(-margin, -margin));
        let bottom_right = self.viewport.screen_to_graph(GraphPoint::new(
            screen_width + margin,
            screen_height + margin,
        ));
        let area = GraphRect {
            x: top_left.x,
            y: top_left.y,
            width: bottom_right.x - top_left.x,
            height: bottom_right.y - top_left.y,
        };
        self.spatial
            .candidates(area)
            .into_iter()
            .filter_map(|index| {
                let node = &self.nodes[index];
                node.bounds().intersects(area).then_some(node)
            })
            .collect()
    }

    /// Move nodes as one undoable operation.
    pub fn move_nodes(&mut self, ids: &[String], delta: GraphPoint) {
        let mut changed_ids = Vec::new();
        let mut before = Vec::new();
        let mut after = Vec::new();
        for id in ids {
            let Some(index) = self.node_lookup.get(id).copied() else {
                continue;
            };
            let previous = self.nodes[index].position;
            let next = GraphPoint::new(previous.x + delta.x, previous.y + delta.y);
            self.nodes[index].position = next;
            changed_ids.push(id.clone());
            before.push(previous);
            after.push(next);
        }
        if !changed_ids.is_empty() {
            self.record(GraphMutation::Move {
                ids: changed_ids,
                before,
                after,
            });
            self.reindex();
        }
    }

    /// Add one user-authored knot at the supplied graph coordinate.
    pub fn add_knot(&mut self, position: GraphPoint) -> String {
        let id = format!("knot:new_{}", self.next_user_node);
        self.next_user_node += 1;
        let node = GraphNode {
            id: id.clone(),
            kind: GraphNodeKind::Knot,
            label: "New knot".to_owned(),
            preview: "Empty narrative block".to_owned(),
            position,
            width: NODE_WIDTH,
            height: NODE_HEIGHT,
            source_span: None,
            validation: Some("Name this knot before saving".to_owned()),
        };
        self.nodes.push(node.clone());
        self.record(GraphMutation::AddNode(node));
        self.reindex();
        id
    }

    /// Add a validated semantic edge.
    pub fn connect(
        &mut self,
        kind: GraphEdgeKind,
        source: &str,
        target: &str,
    ) -> Result<String, GraphConnectionError> {
        self.validate_connection(kind, source, target)?;
        let id = format!("edge:{kind:?}:{source}:{target}:{}", self.edges.len());
        let edge = GraphEdge {
            id: id.clone(),
            kind,
            source: source.to_owned(),
            target: target.to_owned(),
            label: None,
        };
        self.edges.push(edge.clone());
        self.record(GraphMutation::AddEdge(edge));
        Ok(id)
    }

    /// Validate a connection using Weave control-flow rules.
    pub fn validate_connection(
        &self,
        kind: GraphEdgeKind,
        source: &str,
        target: &str,
    ) -> Result<(), GraphConnectionError> {
        if source == target {
            return Err(GraphConnectionError::SelfConnection);
        }
        let source_node = self
            .node(source)
            .ok_or_else(|| GraphConnectionError::UnknownNode(source.to_owned()))?;
        let target_node = self
            .node(target)
            .ok_or_else(|| GraphConnectionError::UnknownNode(target.to_owned()))?;
        if !matches!(
            source_node.kind,
            GraphNodeKind::Knot | GraphNodeKind::Choice
        ) || target_node.kind != GraphNodeKind::Knot
        {
            return Err(GraphConnectionError::IncompatibleKinds);
        }
        if kind == GraphEdgeKind::Choice && source_node.kind != GraphNodeKind::Choice {
            return Err(GraphConnectionError::IncompatibleKinds);
        }
        if self
            .edges
            .iter()
            .any(|edge| edge.kind == kind && edge.source == source && edge.target == target)
        {
            return Err(GraphConnectionError::Duplicate);
        }
        Ok(())
    }

    /// Remove selected nodes and their incident edges as one undoable operation.
    pub fn delete_selection(&mut self) {
        let nodes = self
            .nodes
            .iter()
            .filter(|node| self.selection.contains(&node.id))
            .cloned()
            .collect::<Vec<_>>();
        if nodes.is_empty() {
            return;
        }
        let ids = nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<BTreeSet<_>>();
        let edges = self
            .edges
            .iter()
            .filter(|edge| ids.contains(edge.source.as_str()) || ids.contains(edge.target.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        self.nodes.retain(|node| !ids.contains(node.id.as_str()));
        self.edges.retain(|edge| {
            !ids.contains(edge.source.as_str()) && !ids.contains(edge.target.as_str())
        });
        self.selection.clear();
        self.record(GraphMutation::Remove { nodes, edges });
        self.reindex();
    }

    /// Move keyboard focus/selection to the nearest node in a direction.
    pub fn navigate(&mut self, direction: NavigationDirection) -> Option<&str> {
        let origin = self
            .selection
            .iter()
            .next()
            .and_then(|id| self.node(id))
            .or_else(|| self.nodes.first())?;
        let origin_id = origin.id.clone();
        let origin_position = origin.position;
        let next = self
            .nodes
            .iter()
            .filter(|node| node.id != origin_id)
            .filter_map(|node| {
                directional_score(origin_position, node.position, direction)
                    .map(|score| (score, node.id.clone()))
            })
            .min_by(|left, right| left.0.total_cmp(&right.0))?
            .1;
        self.selection.clear();
        self.selection.insert(next);
        self.selection.iter().next().map(String::as_str)
    }

    /// Undo the last graph mutation.
    pub fn undo(&mut self) -> bool {
        let Some(mutation) = self.undo.pop() else {
            return false;
        };
        self.apply_inverse(&mutation);
        self.redo.push(mutation);
        self.reindex();
        true
    }

    /// Redo the last undone graph mutation.
    pub fn redo(&mut self) -> bool {
        let Some(mutation) = self.redo.pop() else {
            return false;
        };
        self.apply_forward(&mutation);
        self.undo.push(mutation);
        self.reindex();
        true
    }

    /// Convert to `gpui-flow` nodes and edges without duplicating source-domain data.
    #[must_use]
    pub fn flow_parts(&self) -> (Vec<FlowNode>, Vec<FlowEdge>) {
        let nodes = self
            .nodes
            .iter()
            .map(|node| {
                let handles = match node.kind {
                    GraphNodeKind::Knot => vec![
                        HandleDef::target(HandlePosition::Left).id("in"),
                        HandleDef::source(HandlePosition::Right).id("out"),
                    ],
                    GraphNodeKind::Choice => vec![
                        HandleDef::target(HandlePosition::Left).id("in"),
                        HandleDef::source(HandlePosition::Right).id("branch"),
                    ],
                    GraphNodeKind::Grammar | GraphNodeKind::Pattern | GraphNodeKind::Variable => {
                        vec![
                            HandleDef::source(HandlePosition::Right)
                                .id("value")
                                .connectable(false),
                        ]
                    }
                };
                let mut flow = FlowNode::new(&node.id, node.position.x, node.position.y)
                    .label(&node.label)
                    .node_type(node.kind.renderer_key())
                    .size(node.width, node.height)
                    .handles(handles);
                flow.selected = self.selection.contains(&node.id);
                flow
            })
            .collect();
        let edges = self
            .edges
            .iter()
            .map(|edge| {
                let (color, edge_type) = match edge.kind {
                    GraphEdgeKind::Choice => (
                        0x7c9cff,
                        EdgeType::SmoothStep {
                            border_radius: 8.0,
                            offset: 24.0,
                        },
                    ),
                    GraphEdgeKind::Divert => (0x99a3b5, EdgeType::Bezier { curvature: 0.25 }),
                    GraphEdgeKind::Thread => (0xb481ff, EdgeType::Straight),
                };
                let mut flow = FlowEdge::new(&edge.id, &edge.source, &edge.target)
                    .color(color)
                    .stroke_width(if edge.kind == GraphEdgeKind::Thread {
                        1.0
                    } else {
                        2.0
                    })
                    .edge_type(edge_type);
                if let Some(label) = &edge.label {
                    flow = flow.label(label);
                }
                flow
            })
            .collect();
        (nodes, edges)
    }

    /// Apply the viewport currently owned by `gpui-flow`.
    pub fn import_flow_viewport(&mut self, viewport: Viewport) {
        self.set_viewport(GraphViewport {
            x: viewport.x,
            y: viewport.y,
            zoom: viewport.zoom,
        });
    }

    #[must_use]
    pub fn node(&self, id: &str) -> Option<&GraphNode> {
        self.node_lookup.get(id).map(|index| &self.nodes[*index])
    }

    fn record(&mut self, mutation: GraphMutation) {
        self.undo.push(mutation);
        self.redo.clear();
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
    }

    fn apply_inverse(&mut self, mutation: &GraphMutation) {
        match mutation {
            GraphMutation::Move { ids, before, .. } => self.set_positions(ids, before),
            GraphMutation::AddNode(node) => self.nodes.retain(|candidate| candidate.id != node.id),
            GraphMutation::Remove { nodes, edges } => {
                self.nodes.extend(nodes.iter().cloned());
                self.edges.extend(edges.iter().cloned());
            }
            GraphMutation::AddEdge(edge) => {
                self.edges.retain(|candidate| candidate.id != edge.id);
            }
        }
    }

    fn apply_forward(&mut self, mutation: &GraphMutation) {
        match mutation {
            GraphMutation::Move { ids, after, .. } => self.set_positions(ids, after),
            GraphMutation::AddNode(node) => self.nodes.push(node.clone()),
            GraphMutation::Remove { nodes, .. } => {
                let ids = nodes
                    .iter()
                    .map(|node| node.id.as_str())
                    .collect::<BTreeSet<_>>();
                self.nodes.retain(|node| !ids.contains(node.id.as_str()));
                self.edges.retain(|edge| {
                    !ids.contains(edge.source.as_str()) && !ids.contains(edge.target.as_str())
                });
            }
            GraphMutation::AddEdge(edge) => self.edges.push(edge.clone()),
        }
    }

    fn set_positions(&mut self, ids: &[String], positions: &[GraphPoint]) {
        for (id, position) in ids.iter().zip(positions) {
            if let Some(index) = self.node_lookup.get(id).copied() {
                self.nodes[index].position = *position;
            }
        }
    }

    fn reindex(&mut self) {
        self.node_lookup = self
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (node.id.clone(), index))
            .collect();
        self.spatial = SpatialIndex::rebuild(&self.nodes);
    }
}

/// Invalid user-authored graph connection.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GraphConnectionError {
    #[error("unknown graph node `{0}`")]
    UnknownNode(String),
    #[error("a node cannot connect to itself")]
    SelfConnection,
    #[error("these node kinds cannot form that connection")]
    IncompatibleKinds,
    #[error("that connection already exists")]
    Duplicate,
}

fn directional_score(
    origin: GraphPoint,
    candidate: GraphPoint,
    direction: NavigationDirection,
) -> Option<f32> {
    let dx = candidate.x - origin.x;
    let dy = candidate.y - origin.y;
    let (primary, secondary) = match direction {
        NavigationDirection::Left if dx < 0.0 => (-dx, dy.abs()),
        NavigationDirection::Right if dx > 0.0 => (dx, dy.abs()),
        NavigationDirection::Up if dy < 0.0 => (-dy, dx.abs()),
        NavigationDirection::Down if dy > 0.0 => (dy, dx.abs()),
        _ => return None,
    };
    Some(primary + secondary * 2.0)
}

/// Score one `gpui-flow` node for directional keyboard navigation.
#[must_use]
pub fn directional_flow_node(
    origin: FlowPoint,
    candidate: FlowPoint,
    direction: NavigationDirection,
) -> Option<f32> {
    directional_score(
        GraphPoint::new(origin.x, origin.y),
        GraphPoint::new(candidate.x, candidate.y),
        direction,
    )
}

#[derive(Default)]
struct GraphBuilder {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    edge_ordinal: usize,
    declaration_row: usize,
    knot_row: usize,
}

impl GraphBuilder {
    fn add_top_level_nodes(&mut self, document: &Document) {
        for item in &document.items {
            match &item.node {
                Item::Grammar(grammar) => {
                    let preview = grammar
                        .node
                        .entries
                        .iter()
                        .find_map(|entry| match &entry.node {
                            weave_core::ast::GrammarEntry::Rule(rule) => Some(format!(
                                "{} · {} alternative{}",
                                rule.name,
                                rule.alternatives.len(),
                                if rule.alternatives.len() == 1 {
                                    ""
                                } else {
                                    "s"
                                }
                            )),
                            _ => None,
                        })
                        .unwrap_or_else(|| "No grammar rules".to_owned());
                    self.add_declaration_node(
                        format!("grammar:{}", grammar.node.name),
                        GraphNodeKind::Grammar,
                        grammar.node.name.clone(),
                        preview,
                        item.span,
                    );
                }
                Item::Pattern(pattern) => {
                    self.add_declaration_node(
                        format!("pattern:{}", pattern.node.name),
                        GraphNodeKind::Pattern,
                        pattern.node.name.clone(),
                        format!("{} pattern settings", pattern.node.entries.len()),
                        item.span,
                    );
                }
                Item::Global(statement) => {
                    if let Statement::Declare(declaration) = &statement.node {
                        self.add_variable_node(
                            "global",
                            &declaration.name,
                            item.span,
                            "Story global",
                        );
                    }
                }
                Item::Knot(knot) => {
                    let preview = first_text(&knot.node.body)
                        .unwrap_or_else(|| format!("{} statements", knot.node.body.len()));
                    self.nodes.push(GraphNode {
                        id: knot_id(&knot.node.name),
                        kind: GraphNodeKind::Knot,
                        label: knot.node.name.clone(),
                        preview,
                        position: GraphPoint::new(420.0, 72.0 + self.knot_row as f32 * 180.0),
                        width: NODE_WIDTH,
                        height: NODE_HEIGHT,
                        source_span: Some(item.span),
                        validation: None,
                    });
                    self.knot_row += 1;
                }
                Item::Comment(_) | Item::Blank => {}
            }
        }
    }

    fn add_statement_nodes_and_edges(&mut self, document: &Document) {
        for knot in document.knots() {
            let owner = knot_id(&knot.node.name);
            let mut path = Vec::new();
            self.walk_statements(&knot.node.name, &owner, &knot.node.body, &mut path, 0);
        }
    }

    fn walk_statements(
        &mut self,
        knot: &str,
        owner: &str,
        statements: &[Spanned<Statement>],
        path: &mut Vec<usize>,
        depth: usize,
    ) {
        for (index, statement) in statements.iter().enumerate() {
            path.push(index);
            match &statement.node {
                Statement::Choice(choice) => {
                    let choice_id = format!(
                        "choice:{knot}:{}",
                        path.iter()
                            .map(usize::to_string)
                            .collect::<Vec<_>>()
                            .join(".")
                    );
                    self.nodes.push(GraphNode {
                        id: choice_id.clone(),
                        kind: GraphNodeKind::Choice,
                        label: choice.text.clone(),
                        preview: choice.condition.as_ref().map_or_else(
                            || "Always available".to_owned(),
                            |_| "Conditional".to_owned(),
                        ),
                        position: GraphPoint::new(
                            720.0 + depth as f32 * 260.0,
                            72.0 + self.nodes.len() as f32 * 118.0,
                        ),
                        width: NODE_WIDTH,
                        height: 78.0,
                        source_span: Some(statement.span),
                        validation: None,
                    });
                    self.add_edge(
                        GraphEdgeKind::Choice,
                        owner,
                        &choice_id,
                        Some(choice.text.clone()),
                    );
                    if let Some(target) = &choice.divert {
                        self.add_target_edge(GraphEdgeKind::Divert, &choice_id, target, None);
                    }
                    self.walk_statements(knot, &choice_id, &choice.body, path, depth + 1);
                }
                Statement::Divert(target) => {
                    self.add_target_edge(GraphEdgeKind::Divert, owner, target, None);
                }
                Statement::Thread(target) => {
                    self.add_target_edge(
                        GraphEdgeKind::Thread,
                        owner,
                        target,
                        Some("thread".to_owned()),
                    );
                }
                Statement::Declare(declaration) => {
                    self.add_variable_node(
                        knot,
                        &declaration.name,
                        statement.span,
                        "Local declaration",
                    );
                }
                Statement::Conditional(conditional) => {
                    for branch in &conditional.branches {
                        self.walk_statements(knot, owner, &branch.body, path, depth + 1);
                    }
                    if let Some(fallback) = &conditional.fallback {
                        self.walk_statements(knot, owner, fallback, path, depth + 1);
                    }
                }
                Statement::Text(_)
                | Statement::Assign { .. }
                | Statement::MutateList { .. }
                | Statement::End
                | Statement::Comment(_)
                | Statement::Blank => {}
            }
            path.pop();
        }
    }

    fn add_declaration_node(
        &mut self,
        id: String,
        kind: GraphNodeKind,
        label: String,
        preview: String,
        span: Span,
    ) {
        self.nodes.push(GraphNode {
            id,
            kind,
            label,
            preview,
            position: GraphPoint::new(72.0, 72.0 + self.declaration_row as f32 * 136.0),
            width: NODE_WIDTH,
            height: NODE_HEIGHT,
            source_span: Some(span),
            validation: None,
        });
        self.declaration_row += 1;
    }

    fn add_variable_node(&mut self, scope: &str, name: &str, span: Span, preview: &str) {
        let id = format!("variable:{scope}:{name}");
        if self.nodes.iter().any(|node| node.id == id) {
            return;
        }
        self.add_declaration_node(
            id,
            GraphNodeKind::Variable,
            name.to_owned(),
            preview.to_owned(),
            span,
        );
    }

    fn add_target_edge(
        &mut self,
        kind: GraphEdgeKind,
        source: &str,
        target: &str,
        label: Option<String>,
    ) {
        if target != "END" {
            self.add_edge(kind, source, &knot_id(target), label);
        }
    }

    fn add_edge(&mut self, kind: GraphEdgeKind, source: &str, target: &str, label: Option<String>) {
        self.edges.push(GraphEdge {
            id: format!("edge:{:?}:{}", kind, self.edge_ordinal),
            kind,
            source: source.to_owned(),
            target: target.to_owned(),
            label,
        });
        self.edge_ordinal += 1;
    }
}

fn knot_id(name: &str) -> String {
    format!("knot:{name}")
}

fn first_text(statements: &[Spanned<Statement>]) -> Option<String> {
    statements
        .iter()
        .find_map(|statement| match &statement.node {
            Statement::Text(text) => Some(truncate(text, 72)),
            _ => None,
        })
}

fn truncate(value: &str, maximum_chars: usize) -> String {
    let mut characters = value.chars();
    let preview = characters.by_ref().take(maximum_chars).collect::<String>();
    if characters.next().is_some() {
        format!("{preview}…")
    } else {
        preview
    }
}

/// Validate a connection created by `gpui-flow` against a graph snapshot.
#[must_use]
pub fn is_valid_flow_connection(connection: &Connection, nodes: &[FlowNode]) -> bool {
    if connection.source == connection.target {
        return false;
    }
    let source = nodes.iter().find(|node| node.id == connection.source);
    let target = nodes.iter().find(|node| node.id == connection.target);
    let (Some(source), Some(target)) = (source, target) else {
        return false;
    };
    let source_kind = source.node_type.as_ref().map(ToString::to_string);
    let target_kind = target.node_type.as_ref().map(ToString::to_string);
    matches!(source_kind.as_deref(), Some("knot" | "choice"))
        && target_kind.as_deref() == Some("knot")
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    const SOURCE: &str = r#"grammar names {
    value: ["Mira", "Aldric"]
}
pattern cards {
    builtin: tarot
}
VAR score = 0
=== start ===
Welcome.
* [Continue] -> next
<- aside
=== next ===
-> END
=== aside ===
Aside.
"#;

    fn graph() -> GraphDocument {
        GraphDocument::from_ast(&weave_core::parse_document(SOURCE).expect("valid graph source"))
    }

    #[test]
    fn ast_projection_covers_source_constructs_and_control_flow() {
        let graph = graph();
        assert!(graph.node("grammar:names").is_some());
        assert!(graph.node("pattern:cards").is_some());
        assert!(graph.node("variable:global:score").is_some());
        assert!(graph.node("knot:start").is_some());
        assert!(
            graph
                .nodes()
                .iter()
                .any(|node| node.kind == GraphNodeKind::Choice)
        );
        assert!(
            graph
                .edges()
                .iter()
                .any(|edge| edge.kind == GraphEdgeKind::Thread)
        );
    }

    #[test]
    fn viewport_hit_testing_selection_and_navigation_are_deterministic() {
        let mut graph = graph();
        let knot = graph.node("knot:start").expect("start knot").clone();
        assert_eq!(
            graph.hit_test(GraphPoint::new(
                knot.position.x + 2.0,
                knot.position.y + 2.0
            )),
            Some(&knot)
        );
        graph.box_select(knot.bounds(), false);
        assert!(graph.selected().contains("knot:start"));
        let selected = graph.navigate(NavigationDirection::Down);
        assert!(selected.is_some());

        let point = GraphPoint::new(431.5, 98.25);
        let screen = graph.viewport().graph_to_screen(point);
        assert_eq!(graph.viewport().screen_to_graph(screen), point);
    }

    #[test]
    fn graph_mutations_are_undoable_and_connections_are_checked() {
        let mut graph = graph();
        let start = graph.node("knot:start").expect("start knot").position;
        graph.move_nodes(&["knot:start".to_owned()], GraphPoint::new(50.0, -10.0));
        assert_ne!(
            graph.node("knot:start").expect("start knot").position,
            start
        );
        assert!(graph.undo());
        assert_eq!(
            graph.node("knot:start").expect("start knot").position,
            start
        );
        assert!(graph.redo());

        assert_eq!(
            graph.connect(GraphEdgeKind::Divert, "grammar:names", "knot:start"),
            Err(GraphConnectionError::IncompatibleKinds)
        );
        assert!(
            graph
                .connect(GraphEdgeKind::Divert, "knot:next", "knot:start")
                .is_ok()
        );
    }

    #[test]
    fn spatial_queries_keep_large_graph_work_inside_the_frame_budget() {
        let nodes = (0..10_000)
            .map(|index| GraphNode {
                id: format!("knot:{index}"),
                kind: GraphNodeKind::Knot,
                label: format!("Knot {index}"),
                preview: String::new(),
                position: GraphPoint::new(
                    (index % 100) as f32 * 280.0,
                    (index / 100) as f32 * 140.0,
                ),
                width: NODE_WIDTH,
                height: NODE_HEIGHT,
                source_span: None,
                validation: None,
            })
            .collect();
        let graph = GraphDocument::new(nodes, Vec::new());
        let started = Instant::now();
        let mut largest_visible_set = 0;
        for _ in 0..200 {
            largest_visible_set =
                largest_visible_set.max(graph.visible_nodes(1_440.0, 900.0, 200.0).len());
        }
        let average_ms = started.elapsed().as_secs_f64() * 1_000.0 / 200.0;
        assert!(largest_visible_set < 100);
        assert!(
            average_ms < INTERACTIVE_FRAME_BUDGET_MS,
            "{average_ms:.3}ms"
        );
    }
}
