@echo off
REM atomCAD Skill Setup for Claude Code
REM This wrapper handles PowerShell execution policy automatically
REM
REM Usage:
REM   setup-skill.bat -Global           Install skill globally
REM   setup-skill.bat -AddToPath        Add CLI to user PATH
REM   setup-skill.bat -Global -AddToPath Both

powershell -ExecutionPolicy Bypass -File "%~dp0setup-skill.ps1" %*
