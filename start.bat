@echo off
title Valorant Overseer
if not exist "%~dp0.overseer\installed.json" goto :notinstalled
if not exist "%~dp0.venv\Scripts\python.exe" goto :notinstalled
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\start.ps1" & exit /b

:notinstalled
echo.
echo   Valorant Overseer isn't set up on this PC yet.
echo   Run install.bat first (one-time setup), then use start.bat.
echo.
pause
exit /b 1
