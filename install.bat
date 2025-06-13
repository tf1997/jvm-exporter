@echo off
SETLOCAL
SET "EXE_NAME=ferris-watch.exe"
SET "PARAMS=--install"

:: Check for administrator privileges
NET SESSION >nul 2>&1
IF %ERRORLEVEL% EQU 0 (
    GOTO RUN_AS_ADMIN
) ELSE (
    GOTO ELEVATE
)

:ELEVATE
echo Requesting administrator privileges...
echo Set UAC = CreateObject^("Shell.Application"^) > "%temp%\elevate.vbs"
echo UAC.ShellExecute "%~f0", "%PARAMS%", "", "runas", 1 >> "%temp%\elevate.vbs"
"%temp%\elevate.vbs"
del "%temp%\elevate.vbs"
EXIT /B

:RUN_AS_ADMIN
echo Running with administrator privileges...
"%~dp0%EXE_NAME%" %PARAMS%
EXIT /B
