use std::cell::RefCell;
use std::rc::{Rc, Weak};

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
/// This wraps the internally mutable cache state in `Rc<RefCell<...>>`
/// so that GeoNodes can hold `Weak` references back to it in later steps.
#[derive(Clone)]
pub struct GeoTreeCache {
    pub(crate) inner: Rc<RefCell<GeoTreeCacheInner>>, // visibility may be tightened later
}

impl GeoTreeCache {
    pub fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(GeoTreeCacheInner::new())),
        }
    }

    /// Allocate a new GeoNode with the given kind.
    ///
    /// This wires up a unique id and a weak back-reference to the cache so
    /// that the node can notify the cache when it is dropped.
    pub fn alloc_node_with_kind(&self, kind: GeoNodeKind) -> Rc<GeoNode> {
        let weak_cache: Weak<RefCell<GeoTreeCacheInner>> = Rc::downgrade(&self.inner);
        let id = {
            let mut inner = self.inner.borrow_mut();
            inner.allocate_id()
        };

        Rc::new(GeoNode {
            id,
            kind,
            cache: weak_cache,
        })
    }
}
