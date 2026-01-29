#!/usr/bin/env pwsh
# atomCAD Skill Setup for Claude Code
# Installs the skill globally and adds CLI to PATH

$ErrorActionPreference = "Stop"

# Find atomCAD installation directory (where this script is located)
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$AtomCADDir = Split-Path -Parent $ScriptDir
$CLIPath = Join-Path $AtomCADDir "cli"
$SkillSourceDir = Join-Path $AtomCADDir "claude-skill\atomcad"

Write-Host "atomCAD Skill Setup for Claude Code" -ForegroundColor Green
Write-Host ""

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
Write-Host "Adding atomcad-cli to PATH..." -ForegroundColor Yellow
$CurrentPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($CurrentPath -notlike "*$CLIPath*") {
    [Environment]::SetEnvironmentVariable("Path", "$CurrentPath;$CLIPath", "User")
    Write-Host "  Added $CLIPath to user PATH" -ForegroundColor Green
} else {
    Write-Host "  CLI path already in PATH" -ForegroundColor Yellow
}

# Install skill globally
Write-Host "Installing skill globally..." -ForegroundColor Yellow
$GlobalSkillDir = Join-Path $env:USERPROFILE ".claude\skills"
New-Item -ItemType Directory -Path $GlobalSkillDir -Force | Out-Null
$TargetDir = Join-Path $GlobalSkillDir "atomcad"
if (Test-Path $TargetDir) {
    Remove-Item $TargetDir -Recurse -Force
}
Copy-Item $SkillSourceDir $GlobalSkillDir -Recurse
Write-Host "  Installed skill to $TargetDir" -ForegroundColor Green

Write-Host ""
Write-Host "Setup complete!" -ForegroundColor Green
Write-Host ""
Write-Host "Next steps:" -ForegroundColor Cyan
Write-Host "  1. Restart your terminal"
Write-Host "  2. Start atomCAD"
Write-Host "  3. Open Claude Code in your project"
Write-Host "  4. Use /atomcad or ask Claude to create atomic structures"
Write-Host ""
