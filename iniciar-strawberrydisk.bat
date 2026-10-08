@echo off
setlocal
cd /d "%~dp0"

rem Keep compiler response files in a writable project directory for this launch.
if not exist "target\build-temp" mkdir "target\build-temp"
if not exist "target\build-temp" (
  echo Could not create the temporary build directory.
  pause
  exit /b 1
)
set "TEMP=%CD%\target\build-temp"
set "TMP=%TEMP%"

rem The source checkout can have a Low integrity label. Windows then starts
rem binaries built inside it at Low integrity, blocking the taskbar and tray.
rem Build the development executable in the user's normal application data.
if "%LOCALAPPDATA%"=="" (
  echo LOCALAPPDATA is required to start StrawberryDisk from source.
  pause
  exit /b 1
)
set "CARGO_TARGET_DIR=%LOCALAPPDATA%\StrawberryDisk\dev-target"
if not exist "%CARGO_TARGET_DIR%" mkdir "%CARGO_TARGET_DIR%"
if not exist "%CARGO_TARGET_DIR%" (
  echo Could not create the development build directory.
  pause
  exit /b 1
)

rem Codex's restricted shell can pass a local deny proxy to child processes.
rem Remove only that known value; preserve any proxy configured by the user.
if /I "%HTTP_PROXY%"=="http://127.0.0.1:9" set "HTTP_PROXY="
if /I "%HTTPS_PROXY%"=="http://127.0.0.1:9" set "HTTPS_PROXY="
if /I "%ALL_PROXY%"=="http://127.0.0.1:9" set "ALL_PROXY="
if /I "%GIT_HTTP_PROXY%"=="http://127.0.0.1:9" set "GIT_HTTP_PROXY="
if /I "%GIT_HTTPS_PROXY%"=="http://127.0.0.1:9" set "GIT_HTTPS_PROXY="

if exist "%CARGO_TARGET_DIR%\debug\strawberrydisk.exe" (
  "%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -Command "$app = (Resolve-Path -LiteralPath (Join-Path $env:CARGO_TARGET_DIR 'debug\strawberrydisk.exe')).Path; $running = Get-Process strawberrydisk -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $app }; if (-not $running) { exit 1 }; $client = [Net.Sockets.TcpClient]::new(); try { $client.Connect('127.0.0.1', 1420); exit 0 } catch { exit 1 } finally { $client.Dispose() }" >nul 2>&1
  if not errorlevel 1 (
    start "" "%CARGO_TARGET_DIR%\debug\strawberrydisk.exe"
    exit /b 0
  )
)

"%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -Command "$client = [Net.Sockets.TcpClient]::new(); try { $client.Connect('127.0.0.1', 1420); exit 0 } catch { exit 1 } finally { $client.Dispose() }" >nul 2>&1
if not errorlevel 1 (
  echo Port 1420 is already in use. Close the program using it and try again.
  pause
  exit /b 1
)

if exist "%ProgramFiles%\nodejs\node.exe" set "PATH=%ProgramFiles%\nodejs;%PATH%"
if exist "%USERPROFILE%\.cargo\bin\cargo.exe" set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

where pnpm.cmd >nul 2>&1
if errorlevel 1 (
  if exist "%USERPROFILE%\.cache\codex-runtimes\codex-primary-runtime\dependencies\bin\fallback\pnpm.cmd" (
    set "PATH=%USERPROFILE%\.cache\codex-runtimes\codex-primary-runtime\dependencies\bin\fallback;%PATH%"
  ) else (
    where corepack.cmd >nul 2>&1
    if not errorlevel 1 set "PATH=%~dp0scripts;%PATH%"
  )
)
where pnpm.cmd >nul 2>&1
if errorlevel 1 (
  echo pnpm or Corepack is required. Install Node.js and enable Corepack, then try again.
  pause
  exit /b 1
)

where cargo >nul 2>&1
if errorlevel 1 (
  echo Rust and Cargo are required to start StrawberryDisk from source.
  pause
  exit /b 1
)

if not exist "node_modules" (
  call pnpm.cmd install --frozen-lockfile
  if errorlevel 1 (
    echo Dependency installation failed.
    pause
    exit /b 1
  )
)

call pnpm.cmd tauri:dev
if errorlevel 1 (
  echo StrawberryDisk failed to start.
  pause
  exit /b 1
)
