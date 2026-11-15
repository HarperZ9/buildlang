@echo off
cargo build --locked || exit /b 1
for /f "usebackq tokens=*" %%i in (`"%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe" -latest -property installationPath`) do set "VSPATH=%%i"
if "%VSPATH%"=="" ( echo could not locate Visual Studio via vswhere & exit /b 1 )
call "%VSPATH%\VC\Auxiliary\Build\vcvars64.bat" >nul
cl /nologo /LD /EHsc /MD /I include hook/frametrace_hook.cpp target/debug/photon_frametrace.lib d3d11.lib kernel32.lib ntdll.lib userenv.lib ws2_32.lib dbghelp.lib /Fe:hook/frametrace_hook.dll /Fo:hook/frametrace_hook.obj
