param(
  [Parameter(Mandatory = $true)]
  [string]$Path
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path -LiteralPath $Path)) {
  throw "File not found: $Path"
}

$signtool = (Get-Command signtool.exe -ErrorAction SilentlyContinue | Select-Object -First 1).Source
if (-not $signtool) {
  $kits = "${env:ProgramFiles(x86)}\Windows Kits\10\bin"
  if (Test-Path $kits) {
    $signtool = Get-ChildItem $kits -Recurse -Filter signtool.exe |
      Where-Object { $_.FullName -match '\\x64\\' } |
      Sort-Object FullName -Descending |
      Select-Object -First 1 -ExpandProperty FullName
  }
}
if (-not $signtool) {
  throw "signtool.exe not found. Install Windows SDK (Signing Tools)."
}

$timestamp = if ($env:CERS_TIMESTAMP_URL) { $env:CERS_TIMESTAMP_URL } else { "http://timestamp.digicert.com" }

$arguments = @(
  "sign",
  "/fd", "SHA256",
  "/td", "SHA256",
  "/tr", $timestamp,
  "/d", "CERS Academic"
)

if ($env:CERS_SIGN_PFX) {
  if (-not $env:CERS_SIGN_PFX_PASSWORD) {
    throw "CERS_SIGN_PFX is set but CERS_SIGN_PFX_PASSWORD is empty."
  }
  $arguments += @("/f", $env:CERS_SIGN_PFX, "/p", $env:CERS_SIGN_PFX_PASSWORD)
} elseif ($env:CERS_SIGN_CERT_SHA1) {
  $arguments += @("/sha1", $env:CERS_SIGN_CERT_SHA1)
} elseif ($env:CERS_SIGN_SUBJECT) {
  $arguments += @("/n", $env:CERS_SIGN_SUBJECT)
} else {
  throw "No certificate configured. Set CERS_SIGN_CERT_SHA1 (store certificate), CERS_SIGN_PFX + CERS_SIGN_PFX_PASSWORD, or CERS_SIGN_SUBJECT."
}

$arguments += $Path

& $signtool @arguments
if ($LASTEXITCODE -ne 0) {
  throw "signtool sign failed with exit code $LASTEXITCODE"
}

& $signtool verify /pa /v $Path
if ($LASTEXITCODE -ne 0) {
  throw "signtool verify failed with exit code $LASTEXITCODE"
}

Write-Output "Signed: $Path"
