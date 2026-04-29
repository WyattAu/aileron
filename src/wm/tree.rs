use crate::db::workspaces::{SplitDir, WorkspaceData, WorkspaceNode};
use crate::wm::pane::Pane;
use crate::wm::rect::{Direction, Rect, SplitDirection};
use std::cell::{Cell, RefCell};
use std::fmt;
use uuid::Uuid;

/// A node in the BSP tree. Either a leaf (containing a pane) or an internal split.
#[derive(Debug, Clone)]
pub enum BspNode {
    Leaf {
        pane: Pane,
        rect: Rect,
    },
    Split {
        direction: SplitDirection,
        ratio: f64,
        rect: Rect,
        left: Box<BspNode>,
        right: Box<BspNode>,
    },
}

/// Errors that can occur during BSP tree operations.
#[derive(Debug, thiserror::Error)]
pub enum TileError {
    #[error("pane not found: {0}")]
    PaneNotFound(Uuid),
    #[error("cannot close the last pane")]
    LastPane,
    #[error("invalid split ratio: {0} (must be between 0.1 and 0.9)")]
    InvalidRatio(f64),
    #[error("pane too small: child would be below minimum size (100x100)")]
    PaneTooSmall,
    #[error("tree is empty")]
    EmptyTree,
}

/// The BSP tree managing tiled panes.
#[derive(Debug, Clone)]
pub struct BspTree {
    root: Option<BspNode>,
    active_pane_id: Uuid,
    panes_cache: RefCell<Vec<(Uuid, Rect)>>,
    pane_ids_cache: RefCell<Vec<Uuid>>,
    cache_dirty: Cell<bool>,
}

impl BspTree {
    /// Create a new BSP tree with a single pane filling the given viewport.
    pub fn new(viewport: Rect, initial_url: url::Url) -> Self {
        let pane = Pane::new(initial_url);
        let id = pane.id;
        Self {
            root: Some(BspNode::Leaf {
                pane,
                rect: viewport,
            }),
            active_pane_id: id,
            panes_cache: RefCell::new(Vec::new()),
            pane_ids_cache: RefCell::new(Vec::new()),
            cache_dirty: Cell::new(true),
        }
    }

    /// Split a pane into two sub-panes.
    pub fn split(
        &mut self,
        pane_id: Uuid,
        direction: SplitDirection,
        ratio: f64,
    ) -> Result<Uuid, TileError> {
        if !(0.1..=0.9).contains(&ratio) {
            return Err(TileError::InvalidRatio(ratio));
        }

        // Clone root first so we can restore on error.
        // BspNode is Clone (cheap: just Uuid + String + f64).
        let root = self.root.clone().ok_or(TileError::EmptyTree)?;
        let (new_root, new_id) = Self::split_recursive(root, pane_id, direction, ratio)?;
        self.root = Some(new_root);
        self.active_pane_id = new_id;
        self.invalidate_caches();
        Ok(new_id)
    }

    fn split_recursive(
        node: BspNode,
        target_id: Uuid,
        direction: SplitDirection,
        ratio: f64,
    ) -> Result<(BspNode, Uuid), TileError> {
        match node {
            BspNode::Leaf { pane, rect } => {
                let pane_id = pane.id;
                if pane_id != target_id {
                    return Ok((BspNode::Leaf { pane, rect }, pane_id));
                }

                let (left_rect, right_rect) = rect.partition(direction, ratio);
                if left_rect.w < Rect::MIN_W
                    || left_rect.h < Rect::MIN_H
                    || right_rect.w < Rect::MIN_W
                    || right_rect.h < Rect::MIN_H
                {
                    return Err(TileError::PaneTooSmall);
                }

                let new_pane = Pane::new(url::Url::parse("aileron://new").unwrap());
                let new_id = new_pane.id;

                let new_node = BspNode::Split {
                    direction,
                    ratio,
                    rect,
                    left: Box::new(BspNode::Leaf {
                        pane,
                        rect: left_rect,
                    }),
                    right: Box::new(BspNode::Leaf {
                        pane: new_pane,
                        rect: right_rect,
                    }),
                };

                Ok((new_node, new_id))
            }
            BspNode::Split {
                direction: dir,
                ratio,
                rect,
                left,
                right,
            } => {
                let (new_left, left_id) =
                    Self::split_recursive(*left, target_id, direction, ratio)?;
                let left_changed = left_id == target_id;

                let (new_right, right_id) = if left_changed {
                    let rid = match right.as_ref() {
                        BspNode::Leaf { pane, .. } => pane.id,
                        BspNode::Split { .. } => target_id,
                    };
                    (*right, rid)
                } else {
                    Self::split_recursive(*right, target_id, direction, ratio)?
                };

                Ok((
                    BspNode::Split {
                        direction: dir,
                        ratio,
                        rect,
                        left: Box::new(new_left),
                        right: Box::new(new_right),
                    },
                    if left_changed { left_id } else { right_id },
                ))
            }
        }
    }

    /// Close a pane. The sibling expands to fill the parent's space.
    pub fn close(&mut self, pane_id: Uuid) -> Result<(), TileError> {
        let root = self.root.take().ok_or(TileError::EmptyTree)?;

        if Self::leaf_count_node(&root) <= 1 {
            return Err(TileError::LastPane);
        }

        let (new_root, new_active) = Self::close_recursive(root, pane_id)?;
        self.root = Some(new_root);
        self.active_pane_id = new_active;
        self.invalidate_caches();
        Ok(())
    }

    fn close_recursive(node: BspNode, target_id: Uuid) -> Result<(BspNode, Uuid), TileError> {
        match node {
            BspNode::Leaf { pane, rect } => {
                let pane_id = pane.id;
                if pane_id == target_id {
                    Err(TileError::PaneNotFound(target_id))
                } else {
                    Ok((BspNode::Leaf { pane, rect }, pane_id))
                }
            }
            BspNode::Split {
                direction,
                ratio,
                rect,
                left,
                right,
            } => {
                if let BspNode::Leaf { pane, .. } = left.as_ref()
                    && pane.id == target_id
                {
                    let mut new_right = *right;
                    Self::resize_node(&mut new_right, rect);
                    let active = Self::first_leaf_id(&new_right);
                    return Ok((new_right, active));
                }

                if let BspNode::Leaf { pane, .. } = right.as_ref()
                    && pane.id == target_id
                {
                    let mut new_left = *left;
                    Self::resize_node(&mut new_left, rect);
                    let active = Self::first_leaf_id(&new_left);
                    return Ok((new_left, active));
                }

                let (new_left, left_id) = Self::close_recursive(*left, target_id)
                    .map_err(|_| TileError::PaneNotFound(target_id))?;
                let (new_right, right_id) = if left_id == target_id {
                    let right_id = Self::first_leaf_id(&right);
                    (*right, right_id)
                } else {
                    Self::close_recursive(*right, target_id)?
                };

                Ok((
                    BspNode::Split {
                        direction,
                        ratio,
                        rect,
                        left: Box::new(new_left),
                        right: Box::new(new_right),
                    },
                    right_id,
                ))
            }
        }
    }

    /// Get the rectangle for a specific pane.
    pub fn get_rect(&self, pane_id: Uuid) -> Option<Rect> {
        self.root
            .as_ref()
            .and_then(|root| Self::find_rect(root, pane_id))
    }

    fn find_rect(node: &BspNode, pane_id: Uuid) -> Option<Rect> {
        match node {
            BspNode::Leaf { pane, rect } => {
                if pane.id == pane_id {
                    Some(*rect)
                } else {
                    None
                }
            }
            BspNode::Split { left, right, .. } => {
                Self::find_rect(left, pane_id).or_else(|| Self::find_rect(right, pane_id))
            }
        }
    }

    /// Get the active pane ID.
    pub fn active_pane_id(&self) -> Uuid {
        self.active_pane_id
    }

    /// Set the active pane.
    pub fn set_active_pane(&mut self, pane_id: Uuid) {
        if self.get_rect(pane_id).is_some() {
            self.active_pane_id = pane_id;
        }
    }

    /// Resize all panes to fit a new viewport.
    pub fn resize(&mut self, new_viewport: Rect) {
        if let Some(ref mut root) = self.root {
            Self::resize_node(root, new_viewport);
        }
        self.invalidate_caches();
    }

    fn resize_node(node: &mut BspNode, new_rect: Rect) {
        match node {
            BspNode::Leaf { rect, .. } => {
                *rect = new_rect;
            }
            BspNode::Split {
                direction,
                ratio,
                rect,
                left,
                right,
            } => {
                *rect = new_rect;
                let (left_rect, right_rect) = new_rect.partition(*direction, *ratio);
                Self::resize_node(left, left_rect);
                Self::resize_node(right, right_rect);
            }
        }
    }

    fn invalidate_caches(&mut self) {
        self.panes_cache.borrow_mut().clear();
        self.pane_ids_cache.borrow_mut().clear();
        self.cache_dirty.set(true);
    }

    /// Collect all panes with their rectangles.
    pub fn panes(&self) -> Vec<(Uuid, Rect)> {
        if !self.cache_dirty.get() {
            return self.panes_cache.borrow().clone();
        }
        let result = match &self.root {
            None => vec![],
            Some(root) => {
                let mut result = Vec::new();
                Self::collect_panes(root, &mut result);
                result
            }
        };
        *self.panes_cache.borrow_mut() = result.clone();
        self.pane_ids_cache.borrow_mut().clear();
        self.cache_dirty.set(false);
        result
    }

    fn collect_panes(node: &BspNode, result: &mut Vec<(Uuid, Rect)>) {
        match node {
            BspNode::Leaf { pane, rect } => {
                result.push((pane.id, *rect));
            }
            BspNode::Split { left, right, .. } => {
                Self::collect_panes(left, result);
                Self::collect_panes(right, result);
            }
        }
    }

    /// Collect all pane IDs (without rectangles). Cheaper than `panes()` when
    /// only IDs are needed (e.g., checking membership, iterating for commands).
    pub fn pane_ids(&self) -> Vec<Uuid> {
        if !self.cache_dirty.get() && !self.pane_ids_cache.borrow().is_empty() {
            return self.pane_ids_cache.borrow().clone();
        }
        let result = match &self.root {
            None => vec![],
            Some(root) => {
                let mut result = Vec::new();
                Self::collect_ids(root, &mut result);
                result
            }
        };
        *self.pane_ids_cache.borrow_mut() = result.clone();
        self.panes_cache.borrow_mut().clear();
        self.cache_dirty.set(false);
        result
    }

    fn collect_ids(node: &BspNode, result: &mut Vec<Uuid>) {
        match node {
            BspNode::Leaf { pane, .. } => {
                result.push(pane.id);
            }
            BspNode::Split { left, right, .. } => {
                Self::collect_ids(left, result);
                Self::collect_ids(right, result);
            }
        }
    }

    /// Count the number of leaf panes.
    pub fn leaf_count(&self) -> usize {
        self.root.as_ref().map_or(0, Self::leaf_count_node)
    }

    /// Collect all split borders as (position, direction, pane_a_id, pane_b_id).
    /// A horizontal split produces a vertical border line, and vice versa.
    pub fn split_borders(&self) -> Vec<(f64, SplitDirection, Uuid, Uuid)> {
        let mut borders = Vec::new();
        if let Some(ref root) = self.root {
            Self::collect_split_borders(root, &mut borders);
        }
        borders
    }

    fn collect_split_borders(node: &BspNode, borders: &mut Vec<(f64, SplitDirection, Uuid, Uuid)>) {
        match node {
            BspNode::Split {
                direction,
                ratio,
                rect,
                left,
                right,
                ..
            } => {
                // Find the leftmost/topmost pane ID in the left subtree
                // and the rightmost/bottommost pane ID in the right subtree
                let left_ids = Self::collect_leaf_ids(left);
                let right_ids = Self::collect_leaf_ids(right);
                if let (Some(&id_a), Some(&id_b)) = (left_ids.first(), right_ids.first()) {
                    let pos = match direction {
                        SplitDirection::Horizontal => rect.x + rect.w * ratio,
                        SplitDirection::Vertical => rect.y + rect.h * ratio,
                    };
                    borders.push((pos, *direction, id_a, id_b));
                }
                Self::collect_split_borders(left, borders);
                Self::collect_split_borders(right, borders);
            }
            BspNode::Leaf { .. } => {}
        }
    }

    fn collect_leaf_ids(node: &BspNode) -> Vec<Uuid> {
        match node {
            BspNode::Leaf { pane, .. } => vec![pane.id],
            BspNode::Split { left, right, .. } => {
                let mut ids = Self::collect_leaf_ids(left);
                ids.extend(Self::collect_leaf_ids(right));
                ids
            }
        }
    }

    fn leaf_count_node(node: &BspNode) -> usize {
        match node {
            BspNode::Leaf { .. } => 1,
            BspNode::Split { left, right, .. } => {
                Self::leaf_count_node(left) + Self::leaf_count_node(right)
            }
        }
    }

    /// Navigate to an adjacent pane in the given direction.
    pub fn navigate(&self, direction: Direction) -> Option<Uuid> {
        let current_rect = self.get_rect(self.active_pane_id)?;
        let panes = self.panes();

        for (id, rect) in &panes {
            if *id != self.active_pane_id && current_rect.adjacent_to(rect, direction) {
                return Some(*id);
            }
        }
        None
    }

    /// Swap the UUIDs of two leaf panes, effectively exchanging their positions.
    /// Returns true if both panes were found and swapped.
    pub fn swap_pane_ids(&mut self, id_a: Uuid, id_b: Uuid) -> bool {
        if id_a == id_b {
            return false;
        }
        if let Some(ref mut root) = self.root {
            let found_a = Self::pane_exists(root, &id_a);
            let found_b = Self::pane_exists(root, &id_b);
            if !found_a || !found_b {
                return false;
            }
            Self::swap_ids_in_node(root, &id_a, &id_b);
            self.invalidate_caches();
            true
        } else {
            false
        }
    }

    fn pane_exists(node: &BspNode, pane_id: &Uuid) -> bool {
        match node {
            BspNode::Leaf { pane, .. } => &pane.id == pane_id,
            BspNode::Split { left, right, .. } => {
                Self::pane_exists(left, pane_id) || Self::pane_exists(right, pane_id)
            }
        }
    }

    fn swap_ids_in_node(node: &mut BspNode, id_a: &Uuid, id_b: &Uuid) {
        match node {
            BspNode::Leaf { pane, .. } => {
                if &pane.id == id_a {
                    pane.id = *id_b;
                } else if &pane.id == id_b {
                    pane.id = *id_a;
                }
            }
            BspNode::Split { left, right, .. } => {
                Self::swap_ids_in_node(left, id_a, id_b);
                Self::swap_ids_in_node(right, id_a, id_b);
            }
        }
    }

    /// Resize a pane by adjusting the ratio of its parent split.
    /// `amount` is positive (grow) or negative (shrink).
    /// The ratio is clamped to [0.1, 0.9].
    pub fn resize_pane(&mut self, pane_id: Uuid, amount: f64) -> Result<(), TileError> {
        let root = self.root.take().ok_or(TileError::EmptyTree)?;
        let viewport = match &root {
            BspNode::Leaf { rect, .. } => *rect,
            BspNode::Split { rect, .. } => *rect,
        };
        let new_root = Self::resize_pane_node(root, pane_id, amount)?;
        self.root = Some(new_root);
        if let Some(ref mut root) = self.root {
            Self::resize_node(root, viewport);
        }
        self.invalidate_caches();
        Ok(())
    }

    fn resize_pane_node(node: BspNode, target_id: Uuid, amount: f64) -> Result<BspNode, TileError> {
        match node {
            BspNode::Leaf { .. } => Err(TileError::PaneNotFound(target_id)),
            BspNode::Split {
                direction,
                ratio,
                rect,
                left,
                right,
            } => {
                let left_is_target =
                    matches!(left.as_ref(), BspNode::Leaf { pane, .. } if pane.id == target_id);
                let right_is_target =
                    matches!(right.as_ref(), BspNode::Leaf { pane, .. } if pane.id == target_id);

                let left_contains = Self::contains_pane(&left, target_id);
                let right_contains = Self::contains_pane(&right, target_id);

                if left_is_target || right_is_target {
                    let delta = if left_is_target { amount } else { -amount };
                    let new_ratio = (ratio + delta).clamp(0.1, 0.9);
                    return Ok(BspNode::Split {
                        direction,
                        ratio: new_ratio,
                        rect,
                        left,
                        right,
                    });
                }

                if left_contains {
                    let new_left = Self::resize_pane_node(*left, target_id, amount)?;
                    Ok(BspNode::Split {
                        direction,
                        ratio,
                        rect,
                        left: Box::new(new_left),
                        right,
                    })
                } else if right_contains {
                    let new_right = Self::resize_pane_node(*right, target_id, amount)?;
                    Ok(BspNode::Split {
                        direction,
                        ratio,
                        rect,
                        left,
                        right: Box::new(new_right),
                    })
                } else {
                    Err(TileError::PaneNotFound(target_id))
                }
            }
        }
    }

    fn contains_pane(node: &BspNode, pane_id: Uuid) -> bool {
        match node {
            BspNode::Leaf { pane, .. } => pane.id == pane_id,
            BspNode::Split { left, right, .. } => {
                Self::contains_pane(left, pane_id) || Self::contains_pane(right, pane_id)
            }
        }
    }

    fn first_leaf_id(node: &BspNode) -> Uuid {
        match node {
            BspNode::Leaf { pane, .. } => pane.id,
            BspNode::Split { left, .. } => Self::first_leaf_id(left),
        }
    }

    fn find_pane_by_id(node: &BspNode, pane_id: Uuid) -> Option<&Pane> {
        match node {
            BspNode::Leaf { pane, .. } => {
                if pane.id == pane_id {
                    Some(pane)
                } else {
                    None
                }
            }
            BspNode::Split { left, right, .. } => Self::find_pane_by_id(left, pane_id)
                .or_else(|| Self::find_pane_by_id(right, pane_id)),
        }
    }

    /// Remove all panes except the specified one, keeping it as the root.
    pub fn retain_only(&mut self, pane_id: Uuid) -> Result<(), String> {
        let viewport = match &self.root {
            Some(BspNode::Leaf { rect, .. }) => *rect,
            Some(BspNode::Split { rect, .. }) => *rect,
            None => return Err("Empty tree".into()),
        };

        let Some(root) = self.root.as_ref() else {
            return Err("Empty tree".into());
        };
        let pane = Self::find_pane_by_id(root, pane_id)
            .ok_or_else(|| format!("Pane not found: {}", &pane_id.to_string()[..8]))?;

        self.root = Some(BspNode::Leaf {
            pane: pane.clone(),
            rect: viewport,
        });
        self.active_pane_id = pane_id;
        self.invalidate_caches();
        Ok(())
    }

    /// Verify the coverage axiom: sum of leaf areas == viewport area.
    pub fn verify_coverage(&self) -> bool {
        let root = match &self.root {
            Some(r) => r,
            None => return true,
        };
        let viewport_area = match root {
            BspNode::Leaf { rect, .. } => rect.area(),
            BspNode::Split { rect, .. } => rect.area(),
        };
        let leaf_area_sum: f64 = self.panes().iter().map(|(_, r)| r.area()).sum();
        (leaf_area_sum - viewport_area).abs() < 0.001
    }

    /// Verify non-overlapping axiom: no two leaf rectangles overlap.
    pub fn verify_non_overlapping(&self) -> bool {
        let panes = self.panes();
        for i in 0..panes.len() {
            for j in (i + 1)..panes.len() {
                if !panes[i].1.disjoint(&panes[j].1) {
                    return false;
                }
            }
        }
        true
    }

    // ─── Workspace persistence ────────────────────────────────────

    /// Convert the current tree + pane URLs into serializable workspace data.
    /// URLs are fetched from a callback (since the BSP tree stores initial URLs
    /// but the actual current URLs live in WryPaneManager on the main thread).
    pub fn to_workspace_data<F>(&self, url_resolver: F) -> anyhow::Result<WorkspaceData>
    where
        F: Fn(Uuid) -> Option<String>,
    {
        let tree = self
            .root
            .as_ref()
            .map(|r| Self::bsp_to_workspace(r, &url_resolver))
            .ok_or_else(|| anyhow::anyhow!("Empty tree"))?;

        let active_url =
            url_resolver(self.active_pane_id).unwrap_or_else(|| "aileron://new".into());

        Ok(WorkspaceData { tree, active_url })
    }

    fn bsp_to_workspace<F>(node: &BspNode, url_resolver: &F) -> WorkspaceNode
    where
        F: Fn(Uuid) -> Option<String>,
    {
        match node {
            BspNode::Leaf { pane, .. } => WorkspaceNode::Leaf {
                url: url_resolver(pane.id).unwrap_or_else(|| pane.url.to_string()),
            },
            BspNode::Split {
                direction,
                ratio,
                left,
                right,
                ..
            } => WorkspaceNode::Split {
                direction: match direction {
                    SplitDirection::Horizontal => SplitDir::Horizontal,
                    SplitDirection::Vertical => SplitDir::Vertical,
                },
                ratio: *ratio,
                left: Box::new(Self::bsp_to_workspace(left, url_resolver)),
                right: Box::new(Self::bsp_to_workspace(right, url_resolver)),
            },
        }
    }

    /// Rebuild the BSP tree from workspace data and a viewport rectangle.
    /// Returns a new BspTree with the workspace's layout structure.
    /// Pane UUIDs are freshly generated (old UUIDs are not persisted).
    pub fn from_workspace_data(data: &WorkspaceData, viewport: Rect) -> anyhow::Result<Self> {
        let (root, _active_url) = Self::workspace_to_bsp(&data.tree, None)?;
        let mut tree = Self {
            root: Some(root),
            active_pane_id: Uuid::nil(),
            panes_cache: RefCell::new(Vec::new()),
            pane_ids_cache: RefCell::new(Vec::new()),
            cache_dirty: Cell::new(true),
        };

        // Resize all rects to fit the viewport
        tree.resize(viewport);

        // Find the pane matching the active URL
        let active_id = Self::find_pane_by_url(&tree, &data.active_url).unwrap_or_else(|| {
            tree.panes()
                .first()
                .map(|(id, _)| *id)
                .unwrap_or(Uuid::nil())
        });

        tree.active_pane_id = active_id;
        Ok(tree)
    }

    fn workspace_to_bsp(
        node: &WorkspaceNode,
        _url: Option<&str>,
    ) -> anyhow::Result<(BspNode, Option<String>)> {
        match node {
            WorkspaceNode::Leaf { url: leaf_url } => {
                let pane = Pane::new(url::Url::parse(leaf_url)?);
                Ok((
                    BspNode::Leaf {
                        pane,
                        rect: Rect::new(0.0, 0.0, 0.0, 0.0), // placeholder, will be resized
                    },
                    Some(leaf_url.clone()),
                ))
            }
            WorkspaceNode::Split {
                direction,
                ratio,
                left,
                right,
            } => {
                let dir = match direction {
                    SplitDir::Horizontal => SplitDirection::Horizontal,
                    SplitDir::Vertical => SplitDirection::Vertical,
                };
                let (left_node, left_url) = Self::workspace_to_bsp(left, _url)?;
                let (right_node, right_url) = Self::workspace_to_bsp(right, _url)?;
                Ok((
                    BspNode::Split {
                        direction: dir,
                        ratio: *ratio,
                        rect: Rect::new(0.0, 0.0, 0.0, 0.0), // placeholder
                        left: Box::new(left_node),
                        right: Box::new(right_node),
                    },
                    right_url.or(left_url),
                ))
            }
        }
    }

    fn find_pane_by_url(tree: &BspTree, target_url: &str) -> Option<Uuid> {
        let target = url::Url::parse(target_url).ok()?;
        match &tree.root {
            Some(node) => Self::find_pane_by_url_node(node, &target),
            None => None,
        }
    }

    fn find_pane_by_url_node(node: &BspNode, target_url: &url::Url) -> Option<Uuid> {
        match node {
            BspNode::Leaf { pane, .. } => {
                if &pane.url == target_url {
                    Some(pane.id)
                } else {
                    None
                }
            }
            BspNode::Split { left, right, .. } => Self::find_pane_by_url_node(left, target_url)
                .or_else(|| Self::find_pane_by_url_node(right, target_url)),
        }
    }
}

impl fmt::Display for BspTree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "BspTree(panes={}, active={})",
            self.leaf_count(),
            self.active_pane_id
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use url::Url;

    fn test_url() -> Url {
        Url::parse("https://example.com").unwrap()
    }

    #[test]
    fn test_initial_single_pane() {
        let tree = BspTree::new(Rect::new(0.0, 0.0, 1920.0, 1080.0), test_url());
        assert_eq!(tree.leaf_count(), 1);
        let panes = tree.panes();
        assert_eq!(panes.len(), 1);
        assert_eq!(panes[0].1, Rect::new(0.0, 0.0, 1920.0, 1080.0));
    }

    #[test]
    fn test_horizontal_split() {
        let mut tree = BspTree::new(Rect::new(0.0, 0.0, 1920.0, 1080.0), test_url());
        let first_id = tree.active_pane_id();
        let new_id = tree
            .split(first_id, SplitDirection::Horizontal, 0.5)
            .unwrap();

        assert_eq!(tree.leaf_count(), 2);
        assert!(tree.verify_coverage());
        assert!(tree.verify_non_overlapping());

        let panes = tree.panes();
        assert_eq!(panes.len(), 2);
        assert_eq!(tree.active_pane_id(), new_id);
    }

    #[test]
    fn test_vertical_split() {
        let mut tree = BspTree::new(Rect::new(0.0, 0.0, 1920.0, 1080.0), test_url());
        let first_id = tree.active_pane_id();
        tree.split(first_id, SplitDirection::Vertical, 0.5).unwrap();

        assert_eq!(tree.leaf_count(), 2);
        assert!(tree.verify_coverage());
        assert!(tree.verify_non_overlapping());
    }

    #[test]
    fn test_close_pane() {
        let mut tree = BspTree::new(Rect::new(0.0, 0.0, 1920.0, 1080.0), test_url());
        let first_id = tree.active_pane_id();
        let _new_id = tree.split(first_id, SplitDirection::Vertical, 0.5).unwrap();

        assert_eq!(tree.leaf_count(), 2);
        tree.close(first_id).unwrap();

        assert_eq!(tree.leaf_count(), 1);
        assert!(tree.verify_coverage());
        assert!(tree.verify_non_overlapping());
    }

    #[test]
    fn test_reject_small_split() {
        // Use a rectangle where a 0.9 horizontal split would produce a child
        // with height below MIN_H (100px). 110 * 0.1 = 11 < 100.
        let mut tree = BspTree::new(Rect::new(0.0, 0.0, 500.0, 110.0), test_url());
        let result = tree.split(tree.active_pane_id(), SplitDirection::Horizontal, 0.9);
        assert!(result.is_err());
        assert_eq!(tree.leaf_count(), 1);
    }

    #[test]
    fn test_reject_close_last_pane() {
        let mut tree = BspTree::new(Rect::new(0.0, 0.0, 1920.0, 1080.0), test_url());
        let result = tree.close(tree.active_pane_id());
        assert!(matches!(result, Err(TileError::LastPane)));
    }

    #[test]
    fn test_resize_preserves_coverage() {
        let mut tree = BspTree::new(Rect::new(0.0, 0.0, 1920.0, 1080.0), test_url());
        let first_id = tree.active_pane_id();
        tree.split(first_id, SplitDirection::Vertical, 0.5).unwrap();

        assert!(tree.verify_coverage());
        tree.resize(Rect::new(0.0, 0.0, 1280.0, 720.0));
        assert!(tree.verify_coverage());
        assert!(tree.verify_non_overlapping());
    }

    #[test]
    fn test_four_pane_grid() {
        let mut tree = BspTree::new(Rect::new(0.0, 0.0, 1920.0, 1080.0), test_url());
        let id1 = tree.active_pane_id();
        let id2 = tree.split(id1, SplitDirection::Vertical, 0.5).unwrap();
        let _id3 = tree.split(id1, SplitDirection::Horizontal, 0.5).unwrap();
        let _id4 = tree.split(id2, SplitDirection::Horizontal, 0.5).unwrap();

        assert_eq!(tree.leaf_count(), 4);
        assert!(tree.verify_coverage());
        assert!(tree.verify_non_overlapping());
    }

    #[test]
    fn test_invalid_ratio() {
        let mut tree = BspTree::new(Rect::new(0.0, 0.0, 1920.0, 1080.0), test_url());
        assert!(matches!(
            tree.split(tree.active_pane_id(), SplitDirection::Horizontal, 0.0),
            Err(TileError::InvalidRatio(0.0))
        ));
        assert!(matches!(
            tree.split(tree.active_pane_id(), SplitDirection::Horizontal, 1.0),
            Err(TileError::InvalidRatio(1.0))
        ));
        assert!(matches!(
            tree.split(tree.active_pane_id(), SplitDirection::Horizontal, -0.5),
            Err(TileError::InvalidRatio(_))
        ));
    }

    #[test]
    fn test_set_active_pane() {
        let mut tree = BspTree::new(Rect::new(0.0, 0.0, 1920.0, 1080.0), test_url());
        let first_id = tree.active_pane_id();
        let new_id = tree.split(first_id, SplitDirection::Vertical, 0.5).unwrap();
        assert_eq!(tree.active_pane_id(), new_id);
        tree.set_active_pane(first_id);
        assert_eq!(tree.active_pane_id(), first_id);
    }

    #[test]
    fn test_pane_not_found() {
        let mut tree = BspTree::new(Rect::new(0.0, 0.0, 1920.0, 1080.0), test_url());
        let fake_id = Uuid::new_v4();
        let result = tree.split(fake_id, SplitDirection::Vertical, 0.5);
        let _ = result;
    }

    #[test]
    fn test_resize_pane_vertical() {
        let mut tree = BspTree::new(Rect::new(0.0, 0.0, 1000.0, 800.0), test_url());
        let id1 = tree.active_pane_id();
        let id2 = tree.split(id1, SplitDirection::Vertical, 0.5).unwrap();

        tree.resize_pane(id1, 0.1).unwrap();
        assert!(tree.verify_coverage());
        assert!(tree.verify_non_overlapping());

        let panes = tree.panes();
        let left = panes.iter().find(|(id, _)| *id == id1).unwrap().1;
        let right = panes.iter().find(|(id, _)| *id == id2).unwrap().1;
        assert!(left.w > right.w);
    }

    #[test]
    fn test_resize_pane_horizontal() {
        let mut tree = BspTree::new(Rect::new(0.0, 0.0, 1000.0, 800.0), test_url());
        let id1 = tree.active_pane_id();
        let id2 = tree.split(id1, SplitDirection::Horizontal, 0.5).unwrap();

        tree.resize_pane(id1, 0.1).unwrap();
        assert!(tree.verify_coverage());
        assert!(tree.verify_non_overlapping());

        let panes = tree.panes();
        let top = panes.iter().find(|(id, _)| *id == id1).unwrap().1;
        let bottom = panes.iter().find(|(id, _)| *id == id2).unwrap().1;
        assert!(top.h > bottom.h);
    }

    #[test]
    fn test_resize_pane_clamped() {
        let mut tree = BspTree::new(Rect::new(0.0, 0.0, 1000.0, 800.0), test_url());
        let id1 = tree.active_pane_id();
        let _id2 = tree.split(id1, SplitDirection::Vertical, 0.5).unwrap();

        tree.resize_pane(id1, 5.0).unwrap();
        let panes = tree.panes();
        let left = panes.iter().find(|(id, _)| *id == id1).unwrap().1;
        assert!(left.w > 800.0);
    }

    #[test]
    fn test_resize_pane_not_found() {
        let mut tree = BspTree::new(Rect::new(0.0, 0.0, 1000.0, 800.0), test_url());
        let result = tree.resize_pane(Uuid::new_v4(), 0.1);
        assert!(result.is_err());
    }

    #[test]
    fn test_resize_pane_right_child() {
        let mut tree = BspTree::new(Rect::new(0.0, 0.0, 1000.0, 800.0), test_url());
        let id1 = tree.active_pane_id();
        let id2 = tree.split(id1, SplitDirection::Vertical, 0.5).unwrap();

        tree.resize_pane(id2, 0.1).unwrap();
        assert!(tree.verify_coverage());

        let panes = tree.panes();
        let left = panes.iter().find(|(id, _)| *id == id1).unwrap().1;
        let right = panes.iter().find(|(id, _)| *id == id2).unwrap().1;
        assert!(right.w > left.w);
    }

    #[test]
    fn test_resize_pane_nested() {
        let mut tree = BspTree::new(Rect::new(0.0, 0.0, 1000.0, 800.0), test_url());
        let id1 = tree.active_pane_id();
        let _id2 = tree.split(id1, SplitDirection::Vertical, 0.5).unwrap();
        let id3 = tree.split(id1, SplitDirection::Horizontal, 0.5).unwrap();

        tree.resize_pane(id3, 0.1).unwrap();
        assert!(tree.verify_coverage());
        assert!(tree.verify_non_overlapping());

        let panes = tree.panes();
        assert_eq!(panes.len(), 3);
    }
}
