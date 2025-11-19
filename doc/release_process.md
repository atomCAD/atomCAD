# atomCAD Release Process

## Automated Release with GitHub Actions

The atomCAD release process is fully automated using GitHub Actions. This builds the application for Windows, Linux, and macOS in parallel and creates a GitHub release with all platform builds attached.

## How to Create a New Release

### Step 1: Navigate to GitHub Actions

1. Go to the atomCAD repository: https://github.com/atomCAD/atomCAD
2. Click on the **"Actions"** tab (top navigation bar)

### Step 2: Start the Release Workflow

1. In the left sidebar, click on **"Release atomCAD"**
2. On the right side, click the **"Run workflow"** dropdown button
3. Make sure the branch is set to **"main"** (or your release branch)
4. Enter the release version in the input field (e.g., `0.2.0`)
   - Use semantic versioning: `MAJOR.MINOR.PATCH`
   - **Do NOT include the "v" prefix** - the workflow adds it automatically
5. Click the green **"Run workflow"** button

### Step 3: Monitor the Build

The workflow will start running and show 4 jobs:
- **Build Windows Release** (~15-25 minutes)
- **Build Linux Release** (~15-25 minutes)
- **Build macOS Release** (~15-25 minutes)
- **Create GitHub Release** (runs after all builds complete, ~1 minute)

You can click on any job to see detailed logs. The jobs run in parallel, so total time is ~20-30 minutes.

### Step 4: Review the Draft Release

Once the workflow completes successfully:

1. Go to the **"Releases"** section of the repository (right sidebar on main page)
2. You'll see a new **draft release** named `v0.2.0` (or your version)
3. The release will have 3 files attached:
   - `atomCAD-windows-v0.2.0.zip`
   - `atomCAD-linux-v0.2.0.tar.gz`
   - `atomCAD-macos-v0.2.0.zip`

### Step 5: Add Release Notes and Publish

1. Click **"Edit"** on the draft release
2. The release body contains basic installation instructions
3. **Add your release notes** at the bottom where it says "*Add your release notes here before publishing*"
   - What's new in this version
   - Bug fixes
   - Breaking changes (if any)
   - Known issues (if any)
4. Review everything carefully:
   - Check that all 3 platform files are attached
   - Verify the version number is correct
   - Ensure release notes are complete
5. Click **"Publish release"** to make it public

## Example Release Notes

```markdown
### What's New
- Added impostor rendering for improved performance with large atomic structures
- Implemented space-filling visualization mode
- Added transitive dependency resolution for node network imports

### Bug Fixes
- Fixed z-fighting issues in space-filling mode
- Corrected bond rendering in impostor mode

### Known Issues
- macOS users may need to right-click and select "Open" on first launch (Gatekeeper)

### Full Changelog
See all changes: https://github.com/atomCAD/atomCAD/compare/v0.1.0...v0.2.0
```

## Troubleshooting

### Build Fails

If any build job fails:
1. Click on the failed job to see the error logs
2. Common issues:
   - **Compilation errors**: Fix the code issue and run the workflow again
   - **Missing dependencies**: Usually auto-handled, but may need workflow updates
   - **Script execution errors**: Check build script permissions or syntax

### Release Already Exists

If you try to create a release for a version that already exists:
1. Either delete the existing release/tag first
2. Or use a different version number

### Artifacts Not Uploaded

If the workflow completes but files are missing:
1. Check the build job logs for errors in the build scripts
2. Verify the `dist/` folder is created correctly
3. Check artifact upload steps for errors

## Manual Release (Fallback)

If you need to build manually (e.g., for testing):

### Windows
```powershell
.\build_windows_release.ps1 -Version "0.2.0"
```

### Linux
```bash
./build_linux_release.sh --version "0.2.0"
```

### macOS
```bash
./build_macos_release.sh --version "0.2.0"
```

Then upload the files from `dist/` folder manually to a GitHub release.

## Workflow Details

### What the Workflow Does

1. **Parallel Builds**: Runs 3 build jobs simultaneously on GitHub-hosted runners
   - Windows: `windows-latest` runner
   - Linux: `ubuntu-latest` runner
   - macOS: `macos-latest` runner

2. **Environment Setup** (per platform):
   - Checks out code
   - Installs Flutter SDK (with caching for speed)
   - Installs platform-specific dependencies (Linux only)
   - Sets up Rust build cache
   - Runs platform-specific build script

3. **Create Release**:
   - Downloads all build artifacts
   - Creates a draft release with tag `vX.Y.Z`
   - Uploads all 3 platform archives
   - Adds basic installation instructions

### Performance Optimizations

- **Caching**: Flutter SDK and Rust builds are cached between runs
  - First run: ~25-30 minutes total
  - Subsequent runs: ~15-20 minutes total (with warm cache)

- **Parallel Execution**: All 3 platforms build simultaneously
  - Sequential would take ~60-90 minutes
  - Parallel takes ~20-30 minutes

### Workflow File Location

The workflow is defined in: `.github/workflows/release.yml`

To modify the workflow, edit this file and commit changes to the repository.

## Security Notes

- The workflow uses `GITHUB_TOKEN` which is automatically provided by GitHub
- No manual secrets or credentials needed
- The workflow has `contents: write` permission to create releases
- All builds run in isolated, ephemeral VMs (no state between runs)

## Best Practices

1. **Always test before releasing**: Build and test locally first when possible
2. **Use semantic versioning**: Follow `MAJOR.MINOR.PATCH` convention
3. **Review draft releases**: Always check files and add release notes before publishing
4. **Keep changelogs**: Document what changed in each release
5. **Tag properly**: The workflow creates tags automatically (v0.2.0, etc.)
