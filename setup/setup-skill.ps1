#!/usr/bin/env pwsh
# atomCAD Skill Setup for Claude Code
#
# Usage:
#   .\setup-skill.ps1 --global          # Install skill globally for all projects
#   .\setup-skill.ps1 --project <path>  # Install skill to specific project
#   .\setup-skill.ps1 --add-to-path     # Add CLI to user PATH

param(
    [switch]$Global,
    [string]$Project,
    [switch]$AddToPath,
    [switch]$Help
)

$ErrorActionPreference = "Stop"

# Find atomCAD installation directory (where this script is located)
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$AtomCADDir = Split-Path -Parent $ScriptDir
$CLIPath = Join-Path $AtomCADDir "cli"
$SkillSourceDir = Join-Path $AtomCADDir "claude-skill\atomcad"

if ($Help -or (-not $Global -and -not $Project -and -not $AddToPath)) {
    Write-Host "atomCAD Skill Setup for Claude Code" -ForegroundColor Green
    Write-Host ""
    Write-Host "This script configures your system to use atomCAD with Claude Code."
    Write-Host ""
    Write-Host "Options:"
    Write-Host "  --global        Install skill globally (~/.claude/skills/atomcad/)"
    Write-Host "  --project PATH  Install skill to a specific project"
    Write-Host "  --add-to-path   Add atomcad-cli to user PATH environment variable"
    Write-Host "  --help          Show this help"
    Write-Host ""
    Write-Host "Examples:"
    Write-Host "  .\setup-skill.ps1 --global --add-to-path"
    Write-Host "  .\setup-skill.ps1 --project C:\Users\me\my-project"
    Write-Host ""
    exit 0
}

# Verify CLI and skill directory exist
if (-not (Test-Path (Join-Path $CLIPath "atomcad-cli.exe"))) {
    Write-Host "Error: atomcad-cli.exe not found in $CLIPath" -ForegroundColor Red
    exit 1
}
if (-not (Test-Path (Join-Path $SkillSourceDir "skill.md"))) {
    Write-Host "Error: skill.md not found in $SkillSourceDir" -ForegroundColor Red
    exit 1
}

# Add CLI to PATH
if ($AddToPath) {
    Write-Host "Adding atomcad-cli to PATH..." -ForegroundColor Yellow
    $CurrentPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($CurrentPath -notlike "*$CLIPath*") {
        [Environment]::SetEnvironmentVariable("Path", "$CurrentPath;$CLIPath", "User")
        Write-Host "  Added $CLIPath to user PATH" -ForegroundColor Green
        Write-Host "  NOTE: Restart your terminal for changes to take effect" -ForegroundColor Cyan
    } else {
        Write-Host "  CLI path already in PATH" -ForegroundColor Yellow
    }
}

# Install skill globally (copy entire directory structure)
if ($Global) {
    Write-Host "Installing skill globally..." -ForegroundColor Yellow
    $GlobalSkillDir = Join-Path $env:USERPROFILE ".claude\skills"
    New-Item -ItemType Directory -Path $GlobalSkillDir -Force | Out-Null
    # Remove existing atomcad skill if present, then copy new one
    $TargetDir = Join-Path $GlobalSkillDir "atomcad"
    if (Test-Path $TargetDir) {
        Remove-Item $TargetDir -Recurse -Force
    }
    Copy-Item $SkillSourceDir $GlobalSkillDir -Recurse
    Write-Host "  Installed skill to $TargetDir" -ForegroundColor Green
    Write-Host "  (includes skill.md and references/ subdirectory)" -ForegroundColor Gray
}

# Install skill to specific project (copy entire directory structure)
if ($Project) {
    Write-Host "Installing skill to project..." -ForegroundColor Yellow
    if (-not (Test-Path $Project)) {
        Write-Host "Error: Project directory not found: $Project" -ForegroundColor Red
        exit 1
    }
    $ProjectSkillDir = Join-Path $Project ".claude\skills"
    New-Item -ItemType Directory -Path $ProjectSkillDir -Force | Out-Null
    # Remove existing atomcad skill if present, then copy new one
    $TargetDir = Join-Path $ProjectSkillDir "atomcad"
    if (Test-Path $TargetDir) {
        Remove-Item $TargetDir -Recurse -Force
    }
    Copy-Item $SkillSourceDir $ProjectSkillDir -Recurse
    Write-Host "  Installed skill to $TargetDir" -ForegroundColor Green
    Write-Host "  (includes skill.md and references/ subdirectory)" -ForegroundColor Gray
}

Write-Host ""
Write-Host "Setup complete!" -ForegroundColor Green
Write-Host ""
Write-Host "Next steps:" -ForegroundColor Cyan
Write-Host "  1. Start atomCAD"
Write-Host "  2. Open Claude Code in your project"
Write-Host "  3. Use /atomcad or ask Claude to create atomic structures"
Write-Host ""
