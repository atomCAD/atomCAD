# Windows-specific script to run atomCAD with OpenFF/OpenMM conda environment
# This script sets up the Python environment for pyo3 runtime

<# Relax node support and python support are not fully implemented yet,
so temporarily disabled in atomCAD

param(
    [switch]$Build,
    [switch]$Release
)

# Set conda environment path
$condaEnv = "C:\ProgramData\miniforge3\envs\openff-py311"

# Verify conda environment exists
if (-not (Test-Path "$condaEnv\python.exe")) {
    Write-Error "Conda environment not found at: $condaEnv"
    Write-Host "Please create the environment with:"
    Write-Host "mamba create -n openff-py311 -c conda-forge python=3.11 openff-toolkit-base rdkit openmm packaging -y"
    exit 1
}

# Set environment variables for pyo3
$env:PYTHON_SYS_EXECUTABLE = "$condaEnv\python.exe"
$env:PYTHONHOME = $condaEnv
$env:PYTHONPATH = "$condaEnv\Lib\site-packages"
$env:PATH = "$condaEnv;$condaEnv\Scripts;$condaEnv\Library\bin;$env:PATH"

Write-Host "=== atomCAD Windows Runtime Setup ==="
Write-Host "Using conda environment: $condaEnv"
Write-Host "PYTHON_SYS_EXECUTABLE = $env:PYTHON_SYS_EXECUTABLE"
Write-Host "PYTHONHOME = $env:PYTHONHOME"
Write-Host "PYTHONPATH = $env:PYTHONPATH"
Write-Host ""

# Build Rust library if requested
if ($Build) {
    Write-Host "Building Rust library..."
    Push-Location rust
    
    if ($Release) {
        cargo build --release
    } else {
        cargo build
    }
    
    $buildResult = $LASTEXITCODE
    Pop-Location
    
    if ($buildResult -ne 0) {
        Write-Error "Rust build failed"
        exit $buildResult
    }
    
    Write-Host "Rust build completed successfully"
    Write-Host ""
}

# Clean up any existing processes
$existingProcess = Get-Process -Name "flutter_cad" -ErrorAction SilentlyContinue
if ($existingProcess) {
    Write-Host "Stopping existing flutter_cad process..."
    Stop-Process -Name "flutter_cad" -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 2
}

# Run Flutter application
Write-Host "Starting atomCAD..."
flutter run

#>