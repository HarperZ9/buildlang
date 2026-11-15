@echo off
REM Build a .c file to .exe using MSVC
REM Usage: build.bat input.c output.exe

if "%~1"=="" (
    echo Usage: build.bat input.c [output.exe]
    exit /b 1
)

set "INPUT=%~1"
if "%~2"=="" (
    set "OUTPUT=%~dpn1.exe"
) else (
    set "OUTPUT=%~2"
)

call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1
cl /O2 /nologo /Fe:"%OUTPUT%" "%INPUT%" /link /NOLOGO >nul 2>&1

if exist "%OUTPUT%" (
    echo Built: %OUTPUT%
) else (
    echo FAILED: %INPUT%
    exit /b 1
)
