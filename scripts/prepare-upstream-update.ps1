param(
    [string]$UpstreamUrl = 'https://github.com/harry0703/MangoDisk.git'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repositoryRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
Set-Location -LiteralPath $repositoryRoot

if (@(git status --porcelain).Length -ne 0) {
    throw 'Commit or save local changes before preparing an upstream update.'
}
if ((git branch --show-current).Trim() -ne 'main') {
    throw 'Run this script from the private repository main branch.'
}

$baselinePath = Join-Path $repositoryRoot '.upstream-source'
if (-not (Test-Path -LiteralPath $baselinePath)) {
    throw 'The recorded upstream source commit is missing.'
}
$baseline = (Get-Content -LiteralPath $baselinePath -Raw).Trim()
if ($baseline -notmatch '^[0-9a-f]{40}$') {
    throw 'The recorded upstream source commit is invalid.'
}

$remote = git remote get-url upstream 2>$null
if ($LASTEXITCODE -ne 0) {
    git remote add upstream $UpstreamUrl
    if ($LASTEXITCODE -ne 0) { throw 'Could not add the upstream remote.' }
} elseif ($remote.Trim() -ne $UpstreamUrl) {
    throw "The upstream remote points to a different repository: $remote"
}

git fetch --no-tags upstream main
if ($LASTEXITCODE -ne 0) { throw 'Could not fetch the original repository.' }
$sourceCommit = (git rev-parse upstream/main).Trim()
if ($LASTEXITCODE -ne 0 -or $sourceCommit -notmatch '^[0-9a-f]{40}$') {
    throw 'Could not read the upstream commit.'
}
if ($baseline -eq $sourceCommit) {
    Write-Output 'The private main branch already includes the latest upstream source changes.'
    exit 0
}

git merge-base --is-ancestor $baseline $sourceCommit
if ($LASTEXITCODE -ne 0) {
    throw 'Upstream history no longer descends from the recorded source commit. Review it manually before importing.'
}

$shortCommit = $sourceCommit.Substring(0, 8)
$branch = "codex/upstream-$shortCommit"
git show-ref --verify --quiet "refs/heads/$branch"
if ($LASTEXITCODE -eq 0) { throw "The integration branch already exists: $branch" }
if ($LASTEXITCODE -ne 1) { throw "Could not check for branch $branch" }

git switch -c $branch
if ($LASTEXITCODE -ne 0) { throw "Could not create branch $branch" }

$patchDirectory = Join-Path $repositoryRoot 'target'
[System.IO.Directory]::CreateDirectory($patchDirectory) | Out-Null
$patchPath = Join-Path $patchDirectory "upstream-$shortCommit-$([guid]::NewGuid().ToString('N')).patch"
try {
    # Write bytes through Git so binary changes survive Windows PowerShell pipes.
    git diff --binary --full-index --no-ext-diff "--output=$patchPath" $baseline $sourceCommit --
    if ($LASTEXITCODE -ne 0) { throw 'Could not prepare the upstream change patch.' }
    if ((Get-Item -LiteralPath $patchPath).Length -gt 0) {
        git apply --3way --index $patchPath
        if ($LASTEXITCODE -ne 0) {
            throw 'The upstream patch has conflicts. Resolve them on this branch, update .upstream-source to the new commit, and commit the reviewed result.'
        }
    }
    [System.IO.File]::WriteAllText(
        $baselinePath,
        "$sourceCommit`n",
        [System.Text.UTF8Encoding]::new($false)
    )
    git add -- .upstream-source
    if ($LASTEXITCODE -ne 0) { throw 'Could not stage the upstream source marker.' }
    git commit -m "Integrate upstream source through $shortCommit" -m "Upstream-Base: $baseline`nUpstream-Source: $sourceCommit"
    if ($LASTEXITCODE -ne 0) { throw 'Could not commit the upstream integration.' }
} finally {
    if (Test-Path -LiteralPath $patchPath) {
        Remove-Item -LiteralPath $patchPath
    }
}

Write-Output "Prepared upstream changes as one commit on $branch. Review the diff, run pnpm check and the Rust core tests, then fast-forward main."
