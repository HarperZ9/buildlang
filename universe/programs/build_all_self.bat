@echo off
REM Build all programs using the SELF-HOSTED compiler (qcodegen)
REM This proves BuildLang compiles itself.
REM Run from cmd.exe inside this programs/ directory: build_all_self.bat

call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1

set PASS=0
set FAIL_CG=0
set FAIL_CC=0
set SKIP=0

echo === Self-Hosted Build: all programs via qcodegen ===
echo.

for %%f in (*.bld) do (
    set "NAME=%%~nf"
    echo %%~nf | findstr /B "test_" >nul && (
        set /a SKIP+=1
    ) || (
        qcodegen.exe %%f > %%~nf_self.c 2>nul
        if exist "%%~nf_self.c" (
            for %%A in ("%%~nf_self.c") do if %%~zA GTR 100 (
                cl /O2 /nologo /Fe:q%%~nf_self.exe %%~nf_self.c >nul 2>&1
                if exist "q%%~nf_self.exe" (
                    set /a PASS+=1
                    echo   OK  q%%~nf_self.exe
                ) else (
                    set /a FAIL_CC+=1
                    echo   FAIL %%~nf [C compile error]
                )
            ) else (
                set /a FAIL_CG+=1
                echo   FAIL %%~nf [codegen produced empty/small output]
            )
        ) else (
            set /a FAIL_CG+=1
            echo   FAIL %%~nf [codegen error]
        )
    )
)

echo.
echo === Self-Hosted Results ===
echo   PASS (compile + link): %PASS%
echo   FAIL (codegen):        %FAIL_CG%
echo   FAIL (C compile):      %FAIL_CC%
echo   SKIP:                  %SKIP%
