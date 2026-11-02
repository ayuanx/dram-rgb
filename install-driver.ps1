#Requires -Version 5.1
#Requires -RunAsAdministrator

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$expectedHash = '43599EBC5CA92A17DFCFAC6DEB129F8B8FDE91CACB385FFEEE2768113DA7E2EB'
$source = Join-Path $PSScriptRoot 'CtiIO64.sys'
$destination = Join-Path $env:WINDIR 'CtiIO64.sys'

if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
    throw "Bundled driver not found: $source"
}

$sourceHash = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash
if ($sourceHash -ne $expectedHash) {
    throw "Bundled driver hash mismatch: $sourceHash"
}

$signature = Get-AuthenticodeSignature -LiteralPath $source
if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid) {
    throw "Bundled driver signature is not valid: $($signature.Status)"
}

$service = Get-Service -Name 'CTIIO' -ErrorAction SilentlyContinue
if ($null -ne $service) {
    if (-not (Test-Path -LiteralPath $destination -PathType Leaf)) {
        throw 'CTIIO service exists, but its expected driver file is missing; refusing to modify it.'
    }

    $installedHash = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash
    if ($installedHash -ne $expectedHash) {
        throw "A different CTIIO driver is installed ($installedHash); refusing to overwrite it."
    }

    if ($service.Status -ne 'Running') {
        Start-Service -Name 'CTIIO'
    }
    Write-Host 'CTIIO is already installed and running.'
    exit 0
}

if (Test-Path -LiteralPath $destination -PathType Leaf) {
    $installedHash = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash
    if ($installedHash -ne $expectedHash) {
        throw "A different file exists at $destination ($installedHash); refusing to overwrite it."
    }
} else {
    Copy-Item -LiteralPath $source -Destination $destination
}

$scOutput = & "$env:WINDIR\System32\sc.exe" create CTIIO type= kernel start= demand `
    binPath= $destination DisplayName= CTIIO 2>&1
if ($LASTEXITCODE -ne 0) {
    throw "Could not create CTIIO service: $scOutput"
}

Start-Service -Name 'CTIIO'
Write-Host 'CTIIO installed and running.'
