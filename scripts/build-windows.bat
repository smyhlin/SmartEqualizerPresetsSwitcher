@echo off
setlocal EnableExtensions EnableDelayedExpansion

set ROOT_DIR=%~dp0..
pushd "%ROOT_DIR%" >nul || exit /b 1

set RUN_BOOTSTRAP=1
set RUN_CHECK=1
set CLEAN=0
set TARGET=x86_64-pc-windows-msvc

:parse_args
if "%~1"=="" goto args_done
if /I "%~1"=="--skip-bootstrap" set RUN_BOOTSTRAP=0& shift & goto parse_args
if /I "%~1"=="--skip-check" set RUN_CHECK=0& shift & goto parse_args
if /I "%~1"=="--no-check" set RUN_CHECK=0& shift & goto parse_args
if /I "%~1"=="--clean" set CLEAN=1& shift & goto parse_args
if /I "%~1"=="--nsis" shift & goto parse_args
if /I "%~1"=="--help" goto help
if /I "%~1"=="-h" goto help
echo Unknown argument: %~1 1>&2
exit /b 2

:help
echo Build Windows x64 NSIS installer for SmartEQPresetSwitcher.
echo.
echo Usage:
echo   scripts\build-windows.bat [--skip-bootstrap] [--skip-check] [--clean]
echo.
echo Output:
echo   src-tauri\target\%TARGET%\release\bundle\nsis\*.exe
echo.
echo Prerequisites:
echo   Run scripts\bootstrap-windows.bat first, or use bootstrap implicitly.
echo   Requires MSVC build tools (cl.exe) from Visual Studio 2022 Build Tools
echo   with "Desktop development with C++" workload.
exit /b 0

:args_done
if /I not "%PROCESSOR_ARCHITECTURE%"=="AMD64" (
  if /I not "%PROCESSOR_ARCHITEW6432%"=="AMD64" (
    echo ERROR: Only Windows x64 builds are supported. Current architecture: %PROCESSOR_ARCHITECTURE% 1>&2
    exit /b 1
  )
)

if "%CLEAN%"=="1" (
  echo ==^> Cleaning build outputs
  if exist build rmdir /s /q build
  if exist .svelte-kit rmdir /s /q .svelte-kit
  if exist src-tauri\target\%TARGET%\release\bundle rmdir /s /q src-tauri\target\%TARGET%\release\bundle
  if exist src-tauri\target\release\bundle rmdir /s /q src-tauri\target\release\bundle
)

if "%RUN_BOOTSTRAP%"=="1" (
  echo ==^> Running Windows bootstrap
  if "%RUN_CHECK%"=="1" (
    call scripts\bootstrap-windows.bat || exit /b 1
  ) else (
    call scripts\bootstrap-windows.bat --skip-check || exit /b 1
  )
) else (
  echo ==^> Skipping bootstrap
)

if "%RUN_CHECK%"=="1" if "%RUN_BOOTSTRAP%"=="0" (
  echo ==^> Running project check
  call npm run check || exit /b 1
)

echo ==^> Verifying build tools before building
where node >nul 2>nul || (echo ERROR: node not found. Run bootstrap first. 1>&2 & exit /b 1)
where cargo >nul 2>nul || (echo ERROR: cargo not found. Run bootstrap first. 1>&2 & exit /b 1)
where rustc >nul 2>nul || (echo ERROR: rustc not found. Run bootstrap first. 1>&2 & exit /b 1)

where cl.exe >nul 2>nul
if errorlevel 1 (
  echo ERROR: cl.exe (MSVC C++ compiler) not found on PATH. 1>&2
  echo Tauri requires the MSVC toolchain. Install Visual Studio 2022 Build Tools 1>&2
  echo with "Desktop development with C++" workload, then use a "Developer Command Prompt". 1>&2
  exit /b 1
)

echo ==^> Checking Rust MSVC target
rustup target list --installed 2>nul | findstr "%TARGET%" >nul
if errorlevel 1 (
  echo ==^> Installing Rust MSVC target: %TARGET%
  rustup target add %TARGET%
)

where makensis.exe >nul 2>nul || echo WARN: makensis.exe not found on PATH. Tauri usually bundles NSIS, but if the build fails ensure NSIS is available.

echo ==^> Building Windows NSIS installer
call npm run tauri -- build --bundles nsis || exit /b 1

echo ==^> Build artifacts:
for /R src-tauri\target\%TARGET%\release\bundle\nsis %%F in (*.exe) do echo %%F
if not exist src-tauri\target\%TARGET%\release\bundle\nsis\* (
  for /R src-tauri\target\release\bundle\nsis %%F in (*.exe) do echo %%F
)

popd >nul
exit /b 0
