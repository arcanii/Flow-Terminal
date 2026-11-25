#Requires -Version 7
# Write THIRD-PARTY-NOTICES.txt for a Flow release: WinSparkle.dll and the
# libraries built into it, busybox.exe (GPLv2), then every crate compiled into
# flow.exe and the programs beside it, flow-help.exe, flow-settings.exe and
# flowapp.exe, with the license files from its published package. From 0.1.34
# the games and the demos come from flowapp, not with Flow.
# windows\scripts\release.ps1 runs this.
#
#   pwsh windows\scripts\third-party-notices.ps1 -Out target\dist\stage\THIRD-PARTY-NOTICES.txt
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Out,
    [string]$Target = 'x86_64-pc-windows-msvc'
)
$ErrorActionPreference = 'Stop'
# This script is in windows\scripts; the repo root is two levels up.
$root = Split-Path -Path (Split-Path -Path $PSScriptRoot -Parent) -Parent

$json = cargo metadata --format-version 1 --locked --filter-platform $Target `
    --manifest-path (Join-Path $root 'Cargo.toml')
if ($LASTEXITCODE) { throw "cargo metadata failed (exit $LASTEXITCODE)" }
$meta = $json | ConvertFrom-Json
$packages = @{}
foreach ($p in $meta.packages) { $packages[$p.id] = $p }
$nodes = @{}
foreach ($n in $meta.resolve.nodes) { $nodes[$n.id] = $n }

# Walk normal dependencies from flow-terminal, whose programs are all Flow
# carries. Build and dev dependencies don't ship, and neither do proc-macro
# crates or what only they pull in.
$starts = @($meta.resolve.root)
$shipped = [System.Collections.Generic.HashSet[string]]::new()
$queue = [System.Collections.Generic.Queue[string]]::new()
foreach ($id in $starts) { $queue.Enqueue($id) }
while ($queue.Count) {
    $id = $queue.Dequeue()
    if (-not $shipped.Add($id)) { continue }
    foreach ($dep in $nodes[$id].deps) {
        if (-not ($dep.dep_kinds | Where-Object { $null -eq $_.kind })) { continue }
        if ($packages[$dep.pkg].targets | Where-Object { $_.kind -contains 'proc-macro' }) { continue }
        $queue.Enqueue($dep.pkg)
    }
}
foreach ($id in $starts) { $null = $shipped.Remove($id) }
# Our own workspace crates (flow-core, flow-windows) have no source registry: leave them out.
$crates = $shipped | ForEach-Object { $packages[$_] } | Where-Object { $_.source } |
    Sort-Object name, { [string]$_.version }

# Identical license texts (most Apache-2.0 copies) are printed once and numbered.
$textIndex = [System.Collections.Generic.Dictionary[string, int]]::new()
$texts = [System.Collections.Generic.List[string]]::new()
$usedBy = [System.Collections.Generic.List[object]]::new()
$list = foreach ($p in $crates) {
    $dir = Split-Path -Path $p.manifest_path -Parent
    # A crate that builds a C library from its source (libwebp-sys) carries
    # that library's own notices in vendor\, before its own, as
    # examples\third_party_notices.rs reads them for the Mac and Linux.
    $files = foreach ($d in (Join-Path $dir 'vendor'), $dir) {
        if (Test-Path -LiteralPath $d -PathType Container) { Get-ChildItem -LiteralPath $d -File | Sort-Object Name }
    }
    $refs = foreach ($f in $files) {
        if ($f.Name -notmatch '^(LICEN[CS]E|COPYING|COPYRIGHT|NOTICE|UNLICENSE|PATENTS)') { continue }
        $text = ((Get-Content -LiteralPath $f.FullName -Raw) -replace "`r`n", "`n" -replace '[ \t]+(?=\n)', '').Trim()
        if (-not $text) { continue }
        if (-not $textIndex.ContainsKey($text)) {
            $texts.Add($text)
            $usedBy.Add([System.Collections.Generic.List[string]]::new())
            $textIndex[$text] = $texts.Count
        }
        $n = $textIndex[$text]
        $usedBy[$n - 1].Add("$($p.name) $($p.version)")
        "[$n]"
    }
    $source = if ($p.repository) { $p.repository } else { "https://crates.io/crates/$($p.name)/$($p.version)" }
    $license = if ($p.license) { $p.license } else { "see $($p.license_file)" }
    $textLine = if ($refs) { ($refs | Select-Object -Unique) -join ' ' } else { 'not in the published crate; see Source' }
    "$($p.name) $($p.version)"
    "    License: $license"
    "    Source:  $source"
    "    Text:    $textLine"
    ''
}

$rule = '-' * 79
$head = @'
Flow - third-party notices
==========================

Flow and what it carries, flow-help, flow-settings and flowapp, are built
from the open-source Rust crates listed below. Flow ships
WinSparkle.dll for automatic updates, busybox.exe as its built-in bash, and
the fonts for its themes. Their copyright notices and licenses follow.

-------------------------------------------------------------------------------
WinSparkle.dll
-------------------------------------------------------------------------------

WinSparkle 0.9.3, the automatic-update library
    Copyright (C) 2009-2026 Vaclav Slavik
    License: MIT (text below)
    Source:  https://github.com/vslavik/winsparkle

WinSparkle.dll has these libraries built in:

wxWidgets (user interface)
    License: wxWindows Library Licence, Version 3.1
    Source:  https://www.wxwidgets.org/about/licence/

Expat (XML parser)
    Copyright (c) Thai Open Source Software Center Ltd, Clark Cooper and the
    Expat maintainers
    License: MIT (text below)
    Source:  https://libexpat.github.io/

OpenSSL (cryptography)
    License: OpenSSL License and the original SSLeay License
    Source:  https://www.openssl.org/source/license-openssl-ssleay.txt
    This product includes software developed by the OpenSSL Project for use
    in the OpenSSL Toolkit (http://www.openssl.org/).
    This product includes cryptographic software written by Eric Young
    (eay@cryptsoft.com).

MIT License (WinSparkle and Expat)

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
'@

# The built-in bash: BusyBox, GPLv2. Its version comes from the source tarball
# vendored next to it, and the license text from that source's LICENSE.
$bbDir = Join-Path $root 'windows\third_party\busybox'
$bbSource = Get-ChildItem -LiteralPath $bbDir -Filter 'busybox-w32-FRP-*.tgz' | Select-Object -First 1
if (-not $bbSource) { throw "No busybox-w32-FRP-*.tgz in $bbDir" }
$bbVersion = $bbSource.Name -replace '^busybox-w32-' -replace '\.tgz$'
$bbLicense = ((Get-Content -LiteralPath (Join-Path $bbDir 'LICENSE') -Raw) -replace "`r`n", "`n").Trim()
$busybox = @"
$rule
busybox.exe
$rule

BusyBox for Windows (busybox-w32 $bbVersion), the built-in bash and Unix tools
    Copyright: the BusyBox and busybox-w32 authors (listed in AUTHORS in the source)
    License: GNU General Public License, version 2 (text below)
    Source:  $($bbSource.Name), published with each Flow release that
             includes busybox.exe at https://github.com/arcanii/Flow-releases/releases,
             and upstream at https://frippery.org/files/busybox/

$bbLicense
"@

$body = [System.Collections.Generic.List[string]]::new()
$body.Add($head)
$body.Add('')
$body.Add($busybox)
$body.Add('')
# The fonts built into flow.exe for the old machines' themes.
$body.Add(((Get-Content -LiteralPath (Join-Path $root 'assets\fonts\NOTICES.txt') -Raw) -replace "`r`n", "`n").TrimEnd())
$body.Add('')
$body.Add($rule)
$body.Add("Rust crates compiled into flow.exe, flow-help.exe, flow-settings.exe and flowapp.exe ($($crates.Count))")
$body.Add($rule)
$body.Add('')
$body.AddRange([string[]]$list)
$body.Add($rule)
$body.Add('License texts (numbered as in the Text: lines above)')
$body.Add($rule)
for ($i = 0; $i -lt $texts.Count; $i++) {
    $body.Add('')
    $body.Add("[$($i + 1)] $($usedBy[$i] -join ', ')")
    $body.Add('')
    $body.Add($texts[$i])
}

$doc = (($body -join "`n") -replace "`r`n", "`n" -replace "`n", "`r`n") + "`r`n"
[System.IO.File]::WriteAllText($Out, $doc, [System.Text.UTF8Encoding]::new($false))
Write-Host "Wrote $Out ($($crates.Count) crates, $($texts.Count) license texts)"
