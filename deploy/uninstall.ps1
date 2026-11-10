# =============================================================================
#  massmailer - uninstall (Windows)
#
#  Usage:
#    irm https://raw.githubusercontent.com/schlingensiepen/massmailer/main/deploy/uninstall.ps1 | iex
#
#  Deletes %LOCALAPPDATA%\Programs\massmailer and removes it from the user PATH.
#  Run folders and config files are left untouched.
# =============================================================================
& {
    $ErrorActionPreference = 'Continue'
    $dir = Join-Path $env:LOCALAPPDATA 'Programs\massmailer'
    if (Test-Path $dir) { Remove-Item -Recurse -Force $dir; Write-Host "Removed: $dir" }
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ($userPath) {
        $new = (($userPath -split ';') | Where-Object { $_ -and $_ -ne $dir }) -join ';'
        [Environment]::SetEnvironmentVariable('Path', $new, 'User')
    }
    Write-Host 'massmailer has been removed.'
}
