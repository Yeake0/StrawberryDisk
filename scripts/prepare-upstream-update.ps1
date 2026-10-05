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

$remote = git remote get-url upstream 2>$null
if ($LASTEXITCODE -ne 0) {
    git remote add upstream $UpstreamUrl
    if ($LASTEXITCODE -ne 0) { throw 'Could not add the upstream remote.' }
} elseif ($remote.Trim() -ne $UpstreamUrl) {
    throw "The upstream remote points to a different repository: $remote"
}

git fetch upstream main
if ($LASTEXITCODE -ne 0) { throw 'Could not fetch the original repository.' }
git merge-base --is-ancestor upstream/main HEAD
if ($LASTEXITCODE -eq 0) {
    Write-Output 'The private main branch already contains the latest upstream main.'
    exit 0
}
if ($LASTEXITCODE -ne 1) { throw 'Could not compare the upstream and private histories.' }

$shortCommit = (git rev-parse --short=8 upstream/main).Trim()
$branch = "codex/upstream-$shortCommit"
git switch -c $branch
if ($LASTEXITCODE -ne 0) { throw "Could not create branch $branch" }
git merge --no-ff upstream/main -m "Merge upstream MangoDisk $shortCommit"
if ($LASTEXITCODE -ne 0) {
    Write-Warning 'The merge has conflicts. Resolve them on this branch, preserving the private changes, then run the project checks.'
    exit 1
}
Write-Output "Prepared upstream merge on branch $branch. Review the diff and run pnpm check plus the Rust core tests before merging into main."
