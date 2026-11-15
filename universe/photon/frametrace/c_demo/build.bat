@echo off
cargo build --locked || exit /b 1
for /f "usebackq tokens=*" %%i in (`"%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe" -latest -property installationPath`) do set "VSPATH=%%i"
if "%VSPATH%"=="" ( echo could not locate Visual Studio via vswhere & exit /b 1 )
call "%VSPATH%\VC\Auxiliary\Build\vcvars64.bat" >nul
cl /nologo /MD /I include c_demo/demo.c target/debug/photon_frametrace.lib kernel32.lib ntdll.lib userenv.lib ws2_32.lib dbghelp.lib /Fe:c_demo/demo.exe /Fo:c_demo/demo.obj
