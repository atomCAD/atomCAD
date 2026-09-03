#!/usr/bin/env pwsh
# See atomcad-cli: bypass Flutter's dart.bat, which cannot pass a newline inside an argument.
$dart = 'dart'
$resolved = Get-Command dart -ErrorAction SilentlyContinue
if ($resolved -and $resolved.Source) {
    $sdkDart = Join-Path (Split-Path $resolved.Source) 'cache\dart-sdk\bin\dart.exe'
    if (Test-Path $sdkDart) { $dart = $sdkDart }
}
& $dart run bin/atomcad_cli.dart @args
