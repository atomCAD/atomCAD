//! A node network's thumbnail (`doc/design_network_thumbnails.md`): a small
//! PNG of what the network builds, stored inside the network in the `.cnnd`.
//!
//! This crate stores the image as opaque PNG bytes and knows nothing about
//! rendering: the capture, the framing and the "did it really change"
//! comparison live in the renderer crate and the API layer (D2).

use std::sync::atomic::{AtomicU64, Ordering};

/// The stored picture of one network (D7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkThumbnail {
    /// PNG bytes, 128×128.
    pub png: Vec<u8>,
    /// Set by *Set current view as thumbnail* (D6). Automatic capture leaves
    /// a user-set thumbnail alone.
    pub user_set: bool,
}

/// One process-wide counter that never goes backwards (D7). A network takes a
/// fresh value whenever its thumbnail is set or the network is created or
/// deserialized, so two different images never share a revision — which is
/// what lets the Flutter image cache key on the revision alone. A per-network
/// counter would restart on every rebuild of the network.
static NEXT_THUMBNAIL_REVISION: AtomicU64 = AtomicU64::new(1);

/// A revision no network has had before in this process.
pub fn next_thumbnail_revision() -> u64 {
    NEXT_THUMBNAIL_REVISION.fetch_add(1, Ordering::Relaxed)
}
