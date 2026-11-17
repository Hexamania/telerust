<#
.SYNOPSIS
    Redacts sensitive information from telerust logs/errors

.DESCRIPTION
    Reads from stdin or file, outputs sanitized text to stdout.
    Replaces: api_id, api_hash, phone numbers, login codes, session paths
#>

param(
    [string]$InputFile,
    [switch]$Clipboard
)

$patterns = @(
    # api_id=12345 or TELERUST__TELEGRAM__API_ID=12345
    @{ Pattern = '(?i)(api[_-]?id\s*[=:]\s*)\d+'; Replacement = '${1}[REDACTED]' }

    # api_hash=abc123...
    @{ Pattern = '(?i)(api[_-]?hash\s*[=:]\s*)[\w-]+'; Replacement = '${1}[REDACTED]' }

    # Phone numbers: +1234567890, +1 234 567 890
    @{ Pattern = '\+\d[\d\s-]{8,}\d'; Replacement = '+X[REDACTED]' }

    # Login codes (5-6 digits)
    @{ Pattern = '\b\d{5,6}\b(?=\s*(?:code|login))'; Replacement = '[CODE]' }

    # Full session file path with username
    @{ Pattern = 'C:\\Users\\[\w\\\\.]+\\AppData\\\\Roaming\\\\telerust'; Replacement = '%APPDATA%\\telerust' }
    @{ Pattern = '/home/[\\w\\.-]+/.local/share/telerust'; Replacement = '~/.local/share/telerust' }

    # Environment variable exports with secrets
    @{ Pattern = '(?i)(\\$env:TELERUST__TELEGRAM__API_(?:ID|HASH)\s*=\s*")[^"]+'; Replacement = '${1}[REDACTED]' }
)

function Redact-Text {
    param([string]$text)
    foreach ($p in $patterns) {
        $text = $text -replace $p.Pattern, $p.Replacement
    }
    return $text
}

# Read input
if ($Clipboard) {
    $input = Get-Clipboard
} elseif ($InputFile) {
    $input = Get-Content $InputFile -Raw
} else {
    $input = $input = [Console]::In.ReadToEnd()
}

# Process and output
$output = Redact-Text $input
Write-Output $output