param(
    [Parameter(Mandatory)] [string] $MsiPath,
    [string] $CertPath = $env:CONDUCTOR_CERT_PATH,
    [SecureString] $CertPassword,
    [string] $TimestampUrl = "http://timestamp.digicert.com"
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path -LiteralPath $MsiPath)) {
    throw "MSI not found: $MsiPath"
}
if (-not $CertPath -or -not (Test-Path -LiteralPath $CertPath)) {
    throw "Certificate not found. Set -CertPath or CONDUCTOR_CERT_PATH."
}
if (-not $CertPassword) {
    if ($env:CONDUCTOR_CERT_PASSWORD) {
        $CertPassword = ConvertTo-SecureString $env:CONDUCTOR_CERT_PASSWORD -AsPlainText -Force
    } else {
        $CertPassword = Read-Host "Certificate password" -AsSecureString
    }
}

function Find-SignTool {
    if ($env:SIGNTOOL_PATH -and (Test-Path -LiteralPath $env:SIGNTOOL_PATH)) {
        return $env:SIGNTOOL_PATH
    }

    $command = Get-Command signtool.exe -ErrorAction SilentlyContinue
    if ($command) {
        return $command.Source
    }

    $kitRoot = "C:\Program Files (x86)\Windows Kits\10\bin"
    if (Test-Path -LiteralPath $kitRoot) {
        $candidate = Get-ChildItem -Path $kitRoot -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
            Where-Object { $_.FullName -match "\\x64\\signtool\.exe$" } |
            Sort-Object FullName -Descending |
            Select-Object -First 1
        if ($candidate) {
            return $candidate.FullName
        }
    }

    throw "signtool.exe was not found. Install Windows SDK or set SIGNTOOL_PATH."
}

$signtool = Find-SignTool
$bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($CertPassword)
try {
    $plainPassword = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($bstr)
    & $signtool sign /f $CertPath /p $plainPassword /fd SHA256 /tr $TimestampUrl /td SHA256 /d "Personal Conductor Installer" /du "https://conductor.local/" $MsiPath
    if ($LASTEXITCODE -ne 0) {
        throw "signtool sign failed"
    }
} finally {
    [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr)
}

& $signtool verify /pa $MsiPath
if ($LASTEXITCODE -ne 0) {
    throw "signtool verification failed"
}

Write-Host "Signed MSI: $MsiPath" -ForegroundColor Green
