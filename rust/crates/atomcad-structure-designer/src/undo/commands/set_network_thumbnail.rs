use crate::network_thumbnail::NetworkThumbnail;
use crate::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// *Set current view as thumbnail* and *Reset to automatic thumbnail*
/// (`doc/design_network_thumbnails.md` D6): the only command that changes a
/// thumbnail. Automatic capture is not an undo step (D8).
///
/// Each side is the whole stored thumbnail — PNG bytes plus the user-set flag.
/// Applying a side takes a fresh thumbnail revision, so the UI's image cache
/// never shows the image the other side had.
#[derive(Debug)]
pub struct SetNetworkThumbnailCommand {
    pub network_name: String,
    pub before: Option<NetworkThumbnail>,
    pub after: Option<NetworkThumbnail>,
}

impl SetNetworkThumbnailCommand {
    fn apply(&self, ctx: &mut UndoContext, thumbnail: &Option<NetworkThumbnail>) {
        if let Some(network) = ctx.network_mut(&self.network_name) {
            network.set_thumbnail(thumbnail.clone());
        }
    }
}

impl UndoCommand for SetNetworkThumbnailCommand {
    fn description(&self) -> &str {
        "Set network thumbnail"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        self.apply(ctx, &self.before);
    }

    fn redo(&self, ctx: &mut UndoContext) {
        self.apply(ctx, &self.after);
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        // A thumbnail is not evaluated; nothing in the scene changes.
        UndoRefreshMode::Lightweight
    }
}
