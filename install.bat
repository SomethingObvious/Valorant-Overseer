@echo off
title Valorant Overseer Setup
rem With no arguments, hand over to the setup wizard if it is here. It asks
rem which front ends to install and then runs install.ps1 itself. With
rem arguments, or with no wizard built, install.ps1 runs directly.
if "%~1"=="" if exist "%~dp0overseer-setup.exe" (
    start "" "%~dp0overseer-setup.exe"
    exit /b 0
)
if "%~1"=="" if exist "%~dp0crates\target\release\overseer-setup.exe" (
    start "" "%~dp0crates\target\release\overseer-setup.exe"
    exit /b 0
)
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\install.ps1" %*
set "SETUP_EXIT=%ERRORLEVEL%"
echo.
pause
exit /b %SETUP_EXIT%
