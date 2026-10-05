param(
    [string]$ReleaseNotes = 'Atualização do MangoDisk para Windows x64, com as alterações desta versão integradas.'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repositoryRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
Set-Location -LiteralPath $repositoryRoot

$config = Get-Content -LiteralPath 'src-tauri/tauri.conf.json' -Raw | ConvertFrom-Json
$version = [string]$config.version
if ($version -notmatch '^\d+\.\d+\.\d+$') {
    throw "The application version must be a stable SemVer version: $version"
}
if ((Get-Content -LiteralPath 'package.json' -Raw | ConvertFrom-Json).version -ne $version) {
    throw 'The package and Tauri versions differ.'
}

$tag = "v$version"
$updatesRepo = 'Yeake0/MangoDisk-updates'
$sourceRepo = 'Yeake0/MangoDisk'
$expectedEndpoint = "https://github.com/$updatesRepo/releases/latest/download/latest.json"
if (@($config.plugins.updater.endpoints).Count -ne 1 -or $config.plugins.updater.endpoints[0] -ne $expectedEndpoint) {
    throw 'The updater endpoint does not point to the public release channel.'
}
$publicKeyPath = Join-Path $repositoryRoot '.local/updater.key.pub'
if (-not (Test-Path -LiteralPath $publicKeyPath -PathType Leaf) -or
    $config.plugins.updater.pubkey -ne (Get-Content -LiteralPath $publicKeyPath -Raw).Trim()) {
    throw 'The updater public key does not match the local signing key.'
}
$installerName = "MangoDisk_${version}_x64-setup.exe"
$installerPath = Join-Path $repositoryRoot "target/release/bundle/nsis/$installerName"
$signaturePath = "$installerPath.sig"
foreach ($path in @($installerPath, $signaturePath)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Missing signed update artifact: $path"
    }
}

$signature = (Get-Content -LiteralPath $signaturePath -Raw).Trim()
if ($signature -notmatch '^[A-Za-z0-9+/=]+$') {
    throw 'The updater signature is invalid or empty.'
}

$changes = @(git status --porcelain)
if ($changes.Length -ne 0) {
    throw 'Commit or discard worktree changes before publishing a release.'
}
$localCommit = (git rev-parse HEAD).Trim()
$remoteCommit = (gh api "repos/$sourceRepo/commits/main" --jq .sha).Trim()
if ($LASTEXITCODE -ne 0 -or $localCommit -ne $remoteCommit) {
    throw 'Push the exact source commit to the private repository before publishing.'
}

$existingReleases = gh release list --repo $updatesRepo --json tagName --limit 100
if ($LASTEXITCODE -ne 0) {
    throw 'Could not inspect the public update releases.'
}
if (@($existingReleases | ConvertFrom-Json | Where-Object { $_.tagName -eq $tag }).Count -ne 0) {
    throw "The public release $tag already exists. Increase the application version."
}

$assetUrl = "https://github.com/$updatesRepo/releases/download/$tag/$installerName"
$manifest = @{
    version = $version
    notes = $ReleaseNotes
    pub_date = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    platforms = @{
        'windows-x86_64' = @{
            url = $assetUrl
            signature = $signature
        }
    }
}
$staging = Join-Path $repositoryRoot '.local'
New-Item -ItemType Directory -Path $staging -Force | Out-Null
$manifestPath = Join-Path $staging 'latest.json'
$notesPath = Join-Path $staging 'release-notes.md'
$manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $manifestPath -Encoding utf8
@"
$ReleaseNotes

Compilado a partir do commit $localCommit do código privado.
O instalador e a assinatura de atualização são publicados juntos. O instalador Windows não tem assinatura Authenticode.
"@ | Set-Content -LiteralPath $notesPath -Encoding utf8

gh release create $tag $installerPath $signaturePath $manifestPath --repo $updatesRepo --title "MangoDisk $version for Windows x64" --notes-file $notesPath --draft
if ($LASTEXITCODE -ne 0) {
    throw 'Could not create the draft update release.'
}
gh release edit $tag --repo $updatesRepo --draft=false --latest
if ($LASTEXITCODE -ne 0) {
    throw 'The update assets are uploaded, but the release remains a draft.'
}
Write-Output "Published signed update: https://github.com/$updatesRepo/releases/tag/$tag"
