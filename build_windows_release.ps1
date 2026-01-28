# atomCAD Windows Release Build Script
# 
# This script builds atomCAD for Windows and creates a distributable zip file.
# Output: dist\atomCAD-windows-v[VERSION].zip containing all necessary files for distribution.
#
# Usage Examples:
#   .\build_windows_release.ps1                           # Build with default version (1.0.0)
#   .\build_windows_release.ps1 -Version "1.2.3"         # Build with specific version
#   .\build_windows_release.ps1 -Version "2.0.0" -SkipFlutterClean  # Skip clean for faster builds
#   .\build_windows_release.ps1 -SkipRustBuild            # Skip Rust build if already built
#
# The created zip file contains the complete Windows application ready for distribution.

param(
    [string]$Version = "1.0.0",
    [switch]$SkipRustBuild,
    [switch]$SkipFlutterClean
)

$ErrorActionPreference = "Stop"

Write-Host "=== atomCAD Windows Release Build ===" -ForegroundColor Green
Write-Host "Version: $Version" -ForegroundColor Cyan
Write-Host ""

# Get project root directory
$ProjectRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
Push-Location $ProjectRoot

try {
    # Step 1: Build Rust backend in release mode
    if (-not $SkipRustBuild) {
        Write-Host "Building Rust backend (release mode)..." -ForegroundColor Yellow
        Push-Location rust
        cargo build --release
        if ($LASTEXITCODE -ne 0) {
            throw "Rust build failed with exit code $LASTEXITCODE"
        }
        Pop-Location
        Write-Host "Rust build completed successfully" -ForegroundColor Green
        Write-Host ""
    } else {
        Write-Host "Skipping Rust build (SkipRustBuild specified)" -ForegroundColor Yellow
        Write-Host ""
    }

    # Step 2: Clean Flutter build (optional)
    if (-not $SkipFlutterClean) {
        Write-Host "Cleaning Flutter build cache..." -ForegroundColor Yellow
        flutter clean
        Write-Host "Flutter clean completed" -ForegroundColor Green
        Write-Host ""
    } else {
        Write-Host "Skipping Flutter clean (SkipFlutterClean specified)" -ForegroundColor Yellow
        Write-Host ""
    }

    # Step 3: Get Flutter dependencies
    Write-Host "Getting Flutter dependencies..." -ForegroundColor Yellow
    flutter pub get
    if ($LASTEXITCODE -ne 0) {
        throw "Flutter pub get failed with exit code $LASTEXITCODE"
    }
    Write-Host "Flutter dependencies updated" -ForegroundColor Green
    Write-Host ""

    # Step 4: Build Flutter Windows release
    Write-Host "Building Flutter Windows release..." -ForegroundColor Yellow
    flutter build windows --release
    if ($LASTEXITCODE -ne 0) {
        throw "Flutter Windows build failed with exit code $LASTEXITCODE"
    }
    Write-Host "Flutter Windows build completed" -ForegroundColor Green
    Write-Host ""

    # Step 5: Verify release files exist
    $ReleasePath = "build\windows\x64\runner\Release"
    $ExePath = "$ReleasePath\atomCAD.exe"
    
    if (-not (Test-Path $ReleasePath)) {
        throw "Release directory not found: $ReleasePath"
    }
    
    if (-not (Test-Path $ExePath)) {
        throw "Executable not found: $ExePath"
    }
    
    Write-Host "Release files verified" -ForegroundColor Green
    Write-Host ""

    # Step 5.5: Compile CLI to native executable
    Write-Host "Compiling atomcad-cli..." -ForegroundColor Yellow
    $CLIDir = "$ReleasePath\cli"
    New-Item -ItemType Directory -Path $CLIDir -Force | Out-Null
    dart compile exe bin/atomcad_cli.dart -o "$CLIDir\atomcad-cli.exe"
    if ($LASTEXITCODE -ne 0) {
        throw "CLI compilation failed with exit code $LASTEXITCODE"
    }
    Write-Host "CLI compiled successfully" -ForegroundColor Green
    Write-Host ""

    # Step 5.6: Copy skill directory (entire structure)
    Write-Host "Copying Claude skill directory..." -ForegroundColor Yellow
    $SkillDir = "$ReleasePath\claude-skill"
    New-Item -ItemType Directory -Path $SkillDir -Force | Out-Null
    Copy-Item ".claude\skills\atomcad" "$SkillDir\" -Recurse
    Write-Host "Skill directory copied (including references/)" -ForegroundColor Green
    Write-Host ""

    # Step 5.7: Copy setup scripts
    Write-Host "Copying setup scripts..." -ForegroundColor Yellow
    $SetupDir = "$ReleasePath\setup"
    New-Item -ItemType Directory -Path $SetupDir -Force | Out-Null
    Copy-Item "setup\setup-skill.ps1" "$SetupDir\"
    Copy-Item "setup\setup-skill.sh" "$SetupDir\"
    Write-Host "Setup scripts copied" -ForegroundColor Green
    Write-Host ""

    # Step 6: Create zip archive
    $DistDir = Join-Path $ProjectRoot "dist"
    if (-not (Test-Path $DistDir)) {
        New-Item -ItemType Directory -Path $DistDir -Force | Out-Null
        Write-Host "Created dist directory: $DistDir" -ForegroundColor Yellow
    }
    
    $ZipFileName = "atomCAD-windows-v$Version.zip"
    $ZipPath = Join-Path $DistDir $ZipFileName
    
    # Remove existing zip if it exists
    if (Test-Path $ZipPath) {
        Write-Host "Removing existing zip file: $ZipFileName" -ForegroundColor Yellow
        Remove-Item $ZipPath -Force
    }
    
    Write-Host "Creating zip archive: $ZipFileName" -ForegroundColor Yellow
    
    # Create zip from Release folder contents
    $SourcePath = Join-Path $ProjectRoot $ReleasePath
    Compress-Archive -Path "$SourcePath\*" -DestinationPath $ZipPath -CompressionLevel Optimal
    
    if (-not (Test-Path $ZipPath)) {
        throw "Failed to create zip archive: $ZipPath"
    }
    
    # Get zip file size for display
    $ZipSize = [math]::Round((Get-Item $ZipPath).Length / 1MB, 2)
    
    Write-Host "Zip archive created successfully" -ForegroundColor Green
    Write-Host ""
    
    # Step 7: Display results
    Write-Host "=== Build Complete ===" -ForegroundColor Green
    Write-Host "Release executable: $ExePath" -ForegroundColor Cyan
    Write-Host "Zip archive: dist\$ZipFileName ($ZipSize MB)" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "The zip file contains all necessary files for distribution." -ForegroundColor White
    Write-Host "Extract and run flutter_cad.exe on the target Windows machine." -ForegroundColor White

} catch {
    Write-Host ""
    Write-Host "=== Build Failed ===" -ForegroundColor Red
    Write-Host "Error: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
} finally {
    Pop-Location
}
