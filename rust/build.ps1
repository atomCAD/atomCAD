param(
    [switch]$Release,
    [switch]$Clean
)

# path to the venv python
$python = "$env:USERPROFILE\venvs\openff\Scripts\python.exe"
if (-not (Test-Path $python)) {
    Write-Error "Python not found at $python"
    exit 1
}

$env:PYTHON_SYS_EXECUTABLE = $python
Write-Host "Using PYTHON_SYS_EXECUTABLE = $python"

if ($Clean) {
    Write-Host "Running cargo clean..."
    cargo clean
}

if ($Release) {
    Write-Host "Building release..."
    cargo build --release
} else {
    Write-Host "Building debug..."
    cargo build
}
