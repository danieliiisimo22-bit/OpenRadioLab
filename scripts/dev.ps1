# SPDX-License-Identifier: GPL-3.0-only
param(
    [ValidateSet('run', 'analyze', 'build', 'check')]
    [string]$Action = 'run',
    [string]$DatPath,
    [switch]$Release
)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
Get-Command cargo -ErrorAction Stop | Out-Null

function Invoke-Cargo {
    param([string[]]$Arguments)
    & cargo @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "cargo $($Arguments -join ' ') failed with exit code $LASTEXITCODE"
    }
}

Push-Location -LiteralPath $projectRoot
try {
    switch ($Action) {
        'check' {
            Invoke-Cargo -Arguments @('fmt', '--all', '--check')
            Invoke-Cargo -Arguments @('clippy', '--locked', '--workspace', '--all-targets', '--', '-D', 'warnings')
            Invoke-Cargo -Arguments @('test', '--locked', '--workspace')
        }
        'build' {
            $cargoArguments = @('build', '--locked', '--workspace')
            if ($Release) { $cargoArguments += '--release' }
            Invoke-Cargo -Arguments $cargoArguments
        }
        'run' {
            $cargoArguments = @('run', '--locked', '-p', 'radio-app')
            if ($DatPath) { $cargoArguments += @('--', $DatPath) }
            Invoke-Cargo -Arguments $cargoArguments
        }
        'analyze' {
            if (-not $DatPath) { throw 'Use -DatPath to select an input file.' }
            Invoke-Cargo -Arguments @('run', '--locked', '-p', 'dat-analyzer', '--', $DatPath)
        }
    }
} finally {
    Pop-Location
}

