@echo off
title Valorant Overseer - Setup
rem The wizard asks which front ends to install and then runs the script
rem below, so there is one installer rather than two. With no arguments and
rem the wizard present, it gets the job; with arguments, or on a copy built
rem before the wizard existed, the script runs directly as it always did.
if "%~1"=="" if exist "%~dp0overseer-setup.exe" (
    start "" "%~dp0overseer-setup.exe"
    exit /b 0
)
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\install.ps1" %*
set "VS_EXIT=%ERRORLEVEL%"
echo.
pause
exit /b %VS_EXIT%
