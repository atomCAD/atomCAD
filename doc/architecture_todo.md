# Architecture TODO

This document tracks architectural improvements needed to achieve a fully clean dependency graph. These are not urgent and will be addressed incrementally.

## Renderer Module Dependencies (Non-Util)

The renderer module should ideally only depend on `util`, but currently has the following additional dependencies:

### 1. API Dependencies

**Files:** `renderer.rs`, `camera.rs`

```rust
use crate::api::common_api_types::APICameraCanonicalView;
use crate::api::structure_designer::structure_designer_preferences::{StructureDesignerPreferences, BackgroundPreferences};
```

**Issue:** Renderer depends on API types for camera views and preferences.

**Impact:** Medium - Creates coupling between renderer and API layer.

**Possible Solutions:**
- Abstract camera view types into renderer module or util
- Create preference types in renderer and have API layer convert to/from them
- Accept this dependency as reasonable since API is the interface layer

### 2. Crystolecule Dependency

**File:** `renderer.rs`

```rust
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
```

**Usage:** Line 725 in `refresh_background()` method - used to get default unit cell for background coordinate system visualization.

```rust
let unit_cell_to_use = unit_cell.cloned().unwrap_or_else(|| UnitCellStruct::cubic_diamond());
```

**Issue:** Renderer depends on domain-specific crystallography data structure.

**Impact:** Low - Only used for background grid, data-only dependency (no complex logic).

**Possible Solutions:**
- Abstract coordinate system configuration into generic struct in util or renderer
- Pass required grid parameters directly instead of full UnitCellStruct
- Move background grid rendering to display module
- Accept minor dependency as pragmatic (unit cells are fundamental to the application)

## Notes

- Current status: Renderer is **95% independent** - only minor dependencies remain
- All tessellation logic successfully moved to display module ✅
- Renderer no longer depends on structure_designer, geo_tree, or most of crystolecule ✅
- These remaining dependencies are relatively benign and can be addressed incrementally
