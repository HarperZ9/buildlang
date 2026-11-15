@echo off
setlocal enabledelayedexpansion
REM Build all .bld programs to native binaries via buildc + MSVC.
REM Run from cmd.exe inside this programs/ directory: build_all.bat
REM
REM Prerequisites:
REM   - buildc.exe on PATH, or set BUILDC to its full path before running
REM     (build it with: cargo build --release, in the buildlang compiler crate)
REM   - Visual Studio Build Tools 2022 (or newer) with the C++ toolset

call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1

REM Resolve the compiler: honor an explicit BUILDC, else find buildc on PATH.
if not defined BUILDC (
    for /f "delims=" %%p in ('where buildc 2^>nul') do (
        if not defined BUILDC set "BUILDC=%%p"
    )
)
if not defined BUILDC (
    echo ERROR: buildc not found. Put buildc.exe on PATH or set BUILDC to its path.
    exit /b 1
)
echo Using compiler: %BUILDC%

set PASS=0
set FAIL=0
set SKIP=0

echo === Building all BuildLang programs ===
echo.

for %%f in (*.bld) do (
    set "NAME=%%~nf"
    REM Skip test files
    echo %%~nf | findstr /B "test_" >nul && (
        set /a SKIP+=1
        echo   SKIP %%~nf [test file]
    ) || (
        echo   Compiling %%~nf.bld...
        "%BUILDC%" %%f >nul 2>&1
        if exist "%%~nf.c" (
            cl /O2 /nologo /Fe:q%%~nf.exe %%~nf.c >nul 2>&1
            if exist "q%%~nf.exe" (
                set /a PASS+=1
                echo   OK  q%%~nf.exe
            ) else (
                set /a FAIL+=1
                echo   FAIL %%~nf [C compile error]
            )
        ) else (
            set /a FAIL+=1
            echo   FAIL %%~nf [buildc error]
        )
    )
)

echo.
echo === Results ===
echo   PASS: %PASS%
echo   FAIL: %FAIL%
echo   SKIP: %SKIP%
