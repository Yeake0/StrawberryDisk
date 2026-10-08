# Only compiled task recipes can call these functions. Keep bounded failure text;
# successful steps retain counts and state, without flooding logs with command output.
$windowsDirectory = '__WINDOWS__'
$client = $null
$writer = $null
$records = [System.Collections.Generic.List[object]]::new()
$serviceSteps = [System.Collections.Generic.HashSet[int]]::new()
$restartRequired = $false
try {
    $client = [System.Net.Sockets.TcpClient]::new()
    $connection = $client.ConnectAsync([System.Net.IPAddress]::Loopback, __PORT__)
    if (-not $connection.Wait(1000)) { throw [System.TimeoutException]::new() }
    $writer = [System.IO.StreamWriter]::new($client.GetStream(), [System.Text.UTF8Encoding]::new($false))
    $writer.AutoFlush = $true
    $writer.WriteLine('__TOKEN__')
} catch {
    $writer = $null
}
function Send-StrawberryLine([string]$line) {
    if ($null -eq $script:writer) { return }
    try { $script:writer.WriteLine($line) } catch { $script:writer = $null }
}
function Send-StrawberryDiagnostic($record) {
    try { Send-StrawberryLine ($record | ConvertTo-Json -Compress) } catch {}
}
function Get-StrawberryDiagnosticText([string]$text) {
    if ($text.Length -le 512) { return $text }
    # Native tools commonly explain failures at the end, after banners and progress output.
    $start = $text.Length - 512
    if ([char]::IsLowSurrogate($text[$start])) { $start++ }
    return '[truncated]...' + $text.Substring($start)
}
function Set-StrawberryFailure($record, $failure) {
    $record.result = 'Failed'
    $record.outputDetail = Get-StrawberryDiagnosticText ($record.outputDetail + $failure.Exception.Message)
    $record.errorCategory = [int]$failure.CategoryInfo.Category
    $exception = $failure.Exception
    $record.hresult = [int]$exception.HResult
    for ($depth = 0; $null -ne $exception -and $depth -lt 8; $depth++) {
        $record.rootHresult = [int]$exception.HResult
        $unsigned = [BitConverter]::ToUInt32([BitConverter]::GetBytes([int]$exception.HResult), 0)
        # Compare the facility as a positive value: PowerShell 5 parses 0x80070000 as signed.
        if (($unsigned -shr 16) -eq 0x8007) { $record.nativeError = [int]($unsigned -band 0xffff) }
        if ($exception -is [System.ComponentModel.Win32Exception]) { $record.nativeError = [int]$exception.NativeErrorCode }
        $exception = $exception.InnerException
    }
}
function Update-StrawberryServiceState($record) {
    if (-not $script:serviceSteps.Contains([int]$record.index)) { return }
    try {
        $service = Get-Service -Name $record.component -ErrorAction Stop
        $record.after = $service.Status.ToString()
        $record.stateQueryFailed = $false
    } catch {
        $record.after = $null
        $record.stateQueryFailed = $true
    }
}
function Invoke-StrawberryStep([string]$component, [string]$action, [scriptblock]$body, [string]$stage = 'Execute') {
    if ($script:records.Count -ge 32) { throw [System.InvalidOperationException]::new('Maintenance step limit exceeded') }
    $record = [ordered]@{
        index = $script:records.Count + 1; component = $component; action = $action; stage = $stage; result = 'Started'
        before = $null; after = $null; startup = $null; stateQueryFailed = $false
        nativeError = $null; hresult = $null; rootHresult = $null; errorCategory = $null
        exitCode = $null; outputBytes = 0; outputDetail = $null; elapsedMs = 0
    }
    $script:records.Add($record)
    Send-StrawberryDiagnostic $record
    $timer = [System.Diagnostics.Stopwatch]::StartNew()
    try {
        & $body $record
        $record.result = 'Succeeded'
        $record.outputDetail = $null
    } catch {
        # A native command already recorded its exit code and output text. Do not
        # replace that evidence with the synthetic exception used to stop the recipe.
        if ($record.result -ne 'Failed') { Set-StrawberryFailure $record $_ }
        throw
    } finally {
        $record.elapsedMs = $timer.ElapsedMilliseconds
        Update-StrawberryServiceState $record
        Send-StrawberryDiagnostic $record
    }
}
function Invoke-StrawberryService([string]$name, [string]$serviceAction = 'RestartOrStart') {
    [void]$script:serviceSteps.Add($script:records.Count + 1)
    Invoke-StrawberryStep $name 'Query' {
        param($record)
        $service = Get-Service -Name $name -ErrorAction Stop
        $record.before = $service.Status.ToString()
        if ($null -ne $service.StartType) { $record.startup = $service.StartType.ToString() }
        $selected = $serviceAction
        if ($selected -eq 'RestartOrStart') {
            if ($service.Status -eq 'Running') { $selected = 'Restart' } else { $selected = 'Start' }
        }
        $record.action = $selected
        Send-StrawberryDiagnostic $record
        switch ($selected) {
            'Restart' { Restart-Service -Name $name -Force -ErrorAction Stop }
            'Start' { Start-Service -Name $name -ErrorAction Stop }
            'Stop' { Stop-Service -Name $name -Force -ErrorAction Stop }
        }
    }
}
function Invoke-StrawberryNative([string]$component, [string]$executable, [string[]]$arguments, [string]$stage = 'Execute', [string]$progressPhase = '') {
    Invoke-StrawberryStep $component 'Run' {
        param($record)
        if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) { throw [System.ComponentModel.Win32Exception]::new(2) }
        $lastPercent = -1
        if ($progressPhase) { Send-StrawberryLine "$progressPhase|" }
        & $executable @arguments 2>&1 | ForEach-Object {
            $line = $_.ToString()
            $bytes = [System.Text.Encoding]::UTF8.GetBytes($line + "`n")
            $record.outputBytes += $bytes.Length
            $record.outputDetail = Get-StrawberryDiagnosticText ($record.outputDetail + $line + "`n")
            if ($progressPhase -and $line -match '([0-9]{1,3})(?:[\.,][0-9]+)?\s*%') {
                $percent = [Math]::Min(100, [int]$matches[1])
                if ($percent -ne $lastPercent) { Send-StrawberryLine "$progressPhase|$percent"; $lastPercent = $percent }
            }
        }
        $record.exitCode = $LASTEXITCODE
        if ($LASTEXITCODE -eq 3010) { $script:restartRequired = $true }
        elseif ($LASTEXITCODE -ne 0) {
            $record.result = 'Failed'
            throw [System.InvalidOperationException]::new('Native maintenance command failed')
        }
    } $stage
}
$failed = $false
try {
__BODY__
} catch {
    $failed = $true
} finally {
    # Preserve the result of each attempted action and report its latest observable state.
    # Failure stops the recipe; no remaining repair steps are run just for diagnostics.
    foreach ($record in $records) { Update-StrawberryServiceState $record; Send-StrawberryDiagnostic $record }
    try { if ($null -ne $writer) { $writer.Dispose() } } catch {}
    try { if ($null -ne $client) { $client.Dispose() } } catch {}
}
if ($failed) { exit 1 }
if ($restartRequired) { exit 3010 }
exit 0
