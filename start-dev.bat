@echo off
setlocal enabledelayedexpansion

set "ROOT=%~dp0"
if "%ROOT:~-1%"=="\" set "ROOT=%ROOT:~0,-1%"
set "DEV_PORT=1420"

set "PATH=%ROOT%\target\debug;%PATH%"
set "CONDUCTOR_ROOT=%ROOT%"

echo.
echo  Personal Conductor - Development Mode
echo ========================================
echo.

:: --- [0/3] Kill leftover processes ---
echo [0/3] Cleaning up previous session...

for /f %%p in ('powershell -NoProfile -ExecutionPolicy Bypass -Command "(Get-NetTCPConnection -State Listen -LocalPort %DEV_PORT% -ErrorAction SilentlyContinue | Select-Object -ExpandProperty OwningProcess -Unique)"') do (
    if not "%%p"=="" (
        echo   Killing PID %%p (port %DEV_PORT%)
        taskkill /PID %%p /T /F >nul 2>&1
    )
)
taskkill /IM conductor-desktop.exe /T /F >nul 2>&1
timeout /t 1 /nobreak >nul
powershell -NoProfile -ExecutionPolicy Bypass -Command "if (Get-NetTCPConnection -State Listen -LocalPort %DEV_PORT% -ErrorAction SilentlyContinue) { exit 1 }"
if %errorlevel% neq 0 (
    echo   [X] Port %DEV_PORT% is still in use
    pause
    exit /b 1
)
echo   [OK] Environment clean
echo.

:: --- [1/3] Check prerequisites ---
echo [1/3] Checking prerequisites...

node --version >nul 2>&1
if %errorlevel% neq 0 (
    echo   [X] Node.js not found. Install from https://nodejs.org/
    pause
    exit /b 1
)
for /f "tokens=*" %%v in ('node --version') do echo   [OK] Node.js %%v

cargo --version >nul 2>&1
if %errorlevel% neq 0 (
    echo   [X] Rust/Cargo not found. Install from https://rustup.rs/
    pause
    exit /b 1
)
for /f "tokens=*" %%v in ('cargo --version') do echo   [OK] %%v

:: --- [2/3] Install dependencies ---
echo.
echo [2/3] Installing dependencies...
cd /d "%ROOT%\apps\desktop"
if %errorlevel% neq 0 (
    echo   [X] Could not cd to apps\desktop
    pause
    exit /b 1
)
if not exist "node_modules" (
    call npm install
    if %errorlevel% neq 0 (
        echo   [X] npm install failed
        pause
        exit /b 1
    )
    echo   [OK] Dependencies installed
) else (
    echo   [OK] Dependencies already installed
)

:: --- [3/3] Start Tauri dev ---
echo.
echo [3/3] Starting Tauri dev...
echo.
call npx tauri dev
set "TAURI_EXIT=%errorlevel%"
if %TAURI_EXIT% neq 0 (
    echo.
    echo   [X] Tauri dev exited with code %TAURI_EXIT%
    pause
    exit /b %TAURI_EXIT%
)

echo.
echo Development session ended.
pause
