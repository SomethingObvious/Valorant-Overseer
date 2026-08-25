@echo off
title Valorant Overseer - Setup
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\install.ps1" %*
set "VS_EXIT=%ERRORLEVEL%"
echo.
pause
exit /b %VS_EXIT%
