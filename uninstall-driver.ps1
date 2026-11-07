#Requires -Version 5.1
#Requires -RunAsAdministrator

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$name = "CTIIO"
$destination = Join-Path $env:WINDIR 'System32\drivers\CtiIO64.sys'

$service = Get-Service -Name $name -ErrorAction SilentlyContinue
if ($null -ne $service) {
    if ($service.Status -eq 'Running') {
        Stop-Service -Name $name
    }
    $scOutput = & "$env:WINDIR\System32\sc.exe" delete $name
    if ($LASTEXITCODE -ne 0) {
        throw "Could not delete $name service: $scOutput"
    }
}

Remove-Item -LiteralPath $destination

Write-Host "$name driver uninstalled and deleted."
