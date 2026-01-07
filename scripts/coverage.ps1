# atomCAD Test Coverage Script
# Generates HTML coverage reports using cargo-llvm-cov
#
# Prerequisites:
#   cargo install cargo-llvm-cov
#
# Usage:
#   .\scripts\coverage.ps1           # Generate HTML report
#   .\scripts\coverage.ps1 -Open     # Generate and open in browser
#   .\scripts\coverage.ps1 -Summary  # Show summary only (no HTML)

param(
    [switch]$Open,
    [switch]$Summary
)

$ErrorActionPreference = "Stop"

Push-Location "$PSScriptRoot\..\rust"

try {
    # Check if cargo-llvm-cov is installed
    if (-not (Get-Command cargo-llvm-cov -ErrorAction SilentlyContinue)) {
        Write-Host "cargo-llvm-cov not found. Installing..." -ForegroundColor Yellow
        cargo install cargo-llvm-cov
    }

    # Common args: run all tests (not just lib), ignore external csgrs dependency
    $commonArgs = @("--ignore-filename-regex", "csgrs")

    if ($Summary) {
        Write-Host "Generating coverage summary..." -ForegroundColor Cyan
        cargo llvm-cov @commonArgs
    } else {
        Write-Host "Generating HTML coverage report..." -ForegroundColor Cyan
        cargo llvm-cov @commonArgs --html

        $reportPath = "target/llvm-cov/html/index.html"
        if (Test-Path $reportPath) {
            Write-Host "Coverage report generated: rust/$reportPath" -ForegroundColor Green
            if ($Open) {
                Start-Process $reportPath
            }
        } else {
            Write-Error "Coverage report not found at $reportPath"
        }
    }
} finally {
    Pop-Location
}
