@echo off
setlocal

set "ROOT=%~dp0"
if "%ROOT:~-1%"=="\" set "ROOT=%ROOT:~0,-1%"

set "CONDUCTOR_ROOT=%ROOT%"
set "PATH=%ROOT%\bin;%PATH%"

if not exist "%ROOT%\state" mkdir "%ROOT%\state"
if not exist "%ROOT%\state\summaries" mkdir "%ROOT%\state\summaries"
if not exist "%ROOT%\state\config.json" if exist "%ROOT%\state-template\config.json" copy /Y "%ROOT%\state-template\config.json" "%ROOT%\state\config.json" >nul
if not exist "%ROOT%\state\conductor.sqlite" if exist "%ROOT%\state-template\conductor.sqlite" copy /Y "%ROOT%\state-template\conductor.sqlite" "%ROOT%\state\conductor.sqlite" >nul

start "" "%ROOT%\bin\conductor-desktop.exe"
endlocal
