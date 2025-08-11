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
# Add both venv site-packages and base Python paths to PYTHONPATH
$venvSitePackages = Join-Path $venvPath "Lib\site-packages"

# Try to detect base Python installation automatically
$basePythonPath = $null
$possiblePaths = @("C:\Python311\Lib\site-packages", "C:\Python312\Lib\site-packages", "C:\Python310\Lib\site-packages")
foreach ($path in $possiblePaths) {
    if (Test-Path $path) {
        $basePythonPath = $path
        break
    }
}

# Create PYTHONPATH with venv first, then base Python if found
if ($basePythonPath) {
    $env:PYTHONPATH = "$venvSitePackages;$basePythonPath"
    Write-Host "Runtime will use PYTHONPATH = $env:PYTHONPATH"
} else {
    $env:PYTHONPATH = $venvSitePackages
    Write-Host "Runtime will use PYTHONPATH = $env:PYTHONPATH (base Python not found)"
}
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
