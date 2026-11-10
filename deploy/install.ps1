# =============================================================================
#  massmailer - install / update (Windows)
#
#  Usage (PowerShell, no admin rights required):
#    irm https://raw.githubusercontent.com/schlingensiepen/massmailer/main/deploy/install.ps1 | iex
#
#  Downloads massmailer.exe from the latest GitHub release to
#  %LOCALAPPDATA%\Programs\massmailer, verifies the SHA-256 checksum and adds
#  the folder to the user PATH. Running it again updates an existing
#  installation.
#  Optional: $env:MASSMAILER_INSTALL_VERSION = 'v0.1.0' selects a version.
# =============================================================================
& {
    $ErrorActionPreference = 'Stop'
    $ProgressPreference    = 'SilentlyContinue'
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

    $repo  = 'schlingensiepen/massmailer'
    $asset = 'massmailer-x86_64-windows.exe'
    $tag   = $env:MASSMAILER_INSTALL_VERSION
    $base  = if ($tag) { "https://github.com/$repo/releases/download/$tag" } else { "https://github.com/$repo/releases/latest/download" }
    $dir   = Join-Path $env:LOCALAPPDATA 'Programs\massmailer'
    $exe   = Join-Path $dir 'massmailer.exe'
    $tmp   = "$exe.download"

    Write-Host "massmailer: downloading $base/$asset ..."
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    Invoke-WebRequest -UseBasicParsing -Uri "$base/$asset" -OutFile $tmp
    $expected = (Invoke-WebRequest -UseBasicParsing -Uri "$base/$asset.sha256").Content
    if ($expected -is [byte[]]) { $expected = [Text.Encoding]::ASCII.GetString($expected) }
    $expected = ($expected -split '\s+')[0].Trim().ToLowerInvariant()
    $actual   = (Get-FileHash -Algorithm SHA256 -Path $tmp).Hash.ToLowerInvariant()
    if ($expected -ne $actual) {
        Remove-Item -Force $tmp
        throw "Checksum mismatch (expected $expected, got $actual). Aborting."
    }
    Move-Item -Force -Path $tmp -Destination $exe
    Unblock-File -Path $exe

    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (-not (($userPath -split ';') -contains $dir)) {
        [Environment]::SetEnvironmentVariable('Path', ($(if ($userPath) { "$userPath;" } else { '' }) + $dir), 'User')
        Write-Host "Added $dir to your user PATH (open a new terminal to use it)."
    }
    $env:Path = "$env:Path;$dir"
    & $exe --version
    Write-Host "massmailer installed to $dir. Try: massmailer --help"
}
