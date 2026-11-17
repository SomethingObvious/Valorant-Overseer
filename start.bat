@echo off
title Valorant Overseer
set "here=%~dp0"
if not exist "%here%.overseer\installed.json" goto :setup
if not exist "%here%.venv\Scripts\python.exe" goto :setup
powershell -NoProfile -ExecutionPolicy Bypass -File "%here%scripts\start.ps1" %*
exit /b

:setup
echo.
echo   Valorant Overseer isn't set up on this PC yet.
echo   Run install.bat once to set it up, then start it with start.bat.
echo.
pause
exit /b 1
