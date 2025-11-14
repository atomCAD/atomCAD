use std::sync::{Arc, RwLock, Weak};

use super::{GeoNode, GeoNodeKind};

/// Internal state of the geometry tree cache.
///
/// For now this only manages ID allocation. In later steps it will also
/// hold per-node cached meshes and other derived data.
pub struct GeoTreeCacheInner {
    pub next_id: u64,
}

impl GeoTreeCacheInner {
    pub fn new() -> Self {
        Self { next_id: 1 }
    }

    /// Allocate a new unique id for a GeoNode.
    pub fn allocate_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Called when a GeoNode with the given id is dropped.
    ///
    /// This will later be used to clear cached meshes or other resources
    /// associated with the node. For now it is intentionally empty.
    pub fn node_deleted(&mut self, _id: u64) {
        // no-op for now
    }
}

/// Public handle for the geometry tree cache.
///
/// This wraps the internally mutable cache state in `Arc<RwLock<...>>`
/// so that GeoNodes can hold `Weak` references back to it.
#[derive(Clone)]
pub struct GeoTreeCache {
    pub(crate) inner: Arc<RwLock<GeoTreeCacheInner>>, // visibility may be tightened later
}

impl GeoTreeCache {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(GeoTreeCacheInner::new())),
        }
    }

    /// Allocate a new GeoNode with the given kind.
    ///
    /// This wires up a unique id and a weak back-reference to the cache so
    /// that the node can notify the cache when it is dropped.
    pub fn alloc_node_with_kind(&self, kind: GeoNodeKind) -> Arc<GeoNode> {
        let weak_cache: Weak<RwLock<GeoTreeCacheInner>> = Arc::downgrade(&self.inner);
        let id = {
            let mut inner = self.inner.write().unwrap();
            inner.allocate_id()
        };

        Arc::new(GeoNode {
            id,
            kind,
            cache: weak_cache,
        })
    }

    // --- Convenience constructors for common node variants ---

    pub fn half_space(&self, normal: glam::f64::DVec3, center: glam::f64::DVec3) -> Arc<GeoNode> {
        self.alloc_node_with_kind(GeoNodeKind::HalfSpace { normal, center })
    }

    pub fn half_plane(&self, point1: glam::f64::DVec2, point2: glam::f64::DVec2) -> Arc<GeoNode> {
        self.alloc_node_with_kind(GeoNodeKind::HalfPlane { point1, point2 })
    }

    pub fn circle(&self, center: glam::f64::DVec2, radius: f64) -> Arc<GeoNode> {
        self.alloc_node_with_kind(GeoNodeKind::Circle { center, radius })
    }

    pub fn sphere(&self, center: glam::f64::DVec3, radius: f64) -> Arc<GeoNode> {
        self.alloc_node_with_kind(GeoNodeKind::Sphere { center, radius })
    }

    pub fn polygon(&self, vertices: Vec<glam::f64::DVec2>) -> Arc<GeoNode> {
        self.alloc_node_with_kind(GeoNodeKind::Polygon { vertices })
    }

    pub fn extrude(&self, height: f64, direction: glam::f64::DVec3, shape: Arc<GeoNode>) -> Arc<GeoNode> {
        self.alloc_node_with_kind(GeoNodeKind::Extrude { height, direction, shape })
    }

    pub fn transform(&self, transform: crate::util::transform::Transform, shape: Arc<GeoNode>) -> Arc<GeoNode> {
        self.alloc_node_with_kind(GeoNodeKind::Transform { transform, shape })
    }

    pub fn union2d(&self, shapes: Vec<Arc<GeoNode>>) -> Arc<GeoNode> {
        self.alloc_node_with_kind(GeoNodeKind::Union2D { shapes })
    }

    pub fn union3d(&self, shapes: Vec<Arc<GeoNode>>) -> Arc<GeoNode> {
        self.alloc_node_with_kind(GeoNodeKind::Union3D { shapes })
    }

    pub fn intersection2d(&self, shapes: Vec<Arc<GeoNode>>) -> Arc<GeoNode> {
        self.alloc_node_with_kind(GeoNodeKind::Intersection2D { shapes })
    }

    pub fn intersection3d(&self, shapes: Vec<Arc<GeoNode>>) -> Arc<GeoNode> {
        self.alloc_node_with_kind(GeoNodeKind::Intersection3D { shapes })
    }

    pub fn difference2d(&self, base: Arc<GeoNode>, sub: Arc<GeoNode>) -> Arc<GeoNode> {
        self.alloc_node_with_kind(GeoNodeKind::Difference2D { base, sub })
    }

    pub fn difference3d(&self, base: Arc<GeoNode>, sub: Arc<GeoNode>) -> Arc<GeoNode> {
        self.alloc_node_with_kind(GeoNodeKind::Difference3D { base, sub })
    }
}
