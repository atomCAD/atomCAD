# run.ps1
param(
    [switch]$Release
)

# Path to the venv python
$venvPath = "$env:USERPROFILE\venvs\openff"
$pythonExe = Join-Path $venvPath "Scripts\python.exe"

# --- Build-time configuration (pyo3) ---
$env:PYTHON_SYS_EXECUTABLE = $pythonExe
Write-Host "Build will use PYTHON_SYS_EXECUTABLE = $pythonExe"

# --- Runtime configuration (embedded Python) ---
# Don't set PYTHONHOME for virtual environments - let Python find the base installation
# Instead, just add the venv site-packages to PYTHONPATH
$env:PYTHONPATH = Join-Path $venvPath "Lib\site-packages"
Write-Host "Runtime will use PYTHONPATH = $env:PYTHONPATH"
Write-Host "PYTHONHOME not set - Python will use default base installation"

# Remember starting directory (Flutter project root)
$projectRoot = Get-Location

# --- Build Rust code ---
Set-Location "$projectRoot\rust"
if ($Release) {
    cargo build --release
} else {
    cargo build
}

# Go back to Flutter root
Set-Location $projectRoot

# --- Run Flutter app ---
flutter run
