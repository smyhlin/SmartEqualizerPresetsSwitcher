@echo off
setlocal EnableExtensions EnableDelayedExpansion

set APP_NAME=SmartEQPresetSwitcher
set ROOT_DIR=%~dp0..
pushd "%ROOT_DIR%" >nul || exit /b 1

set SKIP_NPM=0
set SKIP_CHECK=0
set NO_INSTALL=0

:parse_args
if "%~1"=="" goto args_done
if /I "%~1"=="--skip-npm" set SKIP_NPM=1& shift & goto parse_args
if /I "%~1"=="--skip-check" set SKIP_CHECK=1& shift & goto parse_args
if /I "%~1"=="--no-install" set NO_INSTALL=1& shift & goto parse_args
if /I "%~1"=="--help" goto help
if /I "%~1"=="-h" goto help
echo Unknown argument: %~1 1>&2
exit /b 2

:help
echo Bootstrap Windows dependencies for %APP_NAME%.
echo.
echo Usage:
echo   scripts\bootstrap-windows.bat [--no-install] [--skip-npm] [--skip-check]
echo.
echo Options:
echo   --no-install   Do not install tools via winget, only verify.
echo   --skip-npm     Do not run npm ci / npm install.
echo   --skip-check   Do not run npm run check after npm install.
echo.
echo Prerequisites (install manually if winget fails):
echo   - Visual Studio 2022 Build Tools or VS 2022 with "Desktop development with C++"
echo   - Rustup (installed below)
echo   - Node.js LTS (installed below)
exit /b 0

:args_done
if /I not "%PROCESSOR_ARCHITECTURE%"=="AMD64" (
  if /I not "%PROCESSOR_ARCHITEW6432%"=="AMD64" (
    echo ERROR: Only Windows x64 is supported. Current architecture: %PROCESSOR_ARCHITECTURE% 1>&2
    exit /b 1
  )
)

echo ==^> Checking required tools
where node >nul 2>nul || set NEED_NODE=1
where npm >nul 2>nul || set NEED_NODE=1
where cargo >nul 2>nul || set NEED_RUST=1
where rustc >nul 2>nul || set NEED_RUST=1

if "%NO_INSTALL%"=="0" (
  where winget >nul 2>nul
  if errorlevel 1 (
    echo WARN: winget not found. Install Node.js LTS, Rustup, and VS Build Tools manually if missing.
  ) else (
    if defined NEED_NODE (
      echo ==^> Installing Node.js LTS with winget
      winget install --id OpenJS.NodeJS.LTS --exact --accept-source-agreements --accept-package-agreements
    )
    if defined NEED_RUST (
      echo ==^> Installing Rustup with winget
      winget install --id Rustlang.Rustup --exact --accept-source-agreements --accept-package-agreements
    )
    echo ==^> Ensuring Microsoft Edge WebView2 Runtime is installed
    winget install --id Microsoft.EdgeWebView2Runtime --exact --accept-source-agreements --accept-package-agreements

    echo ==^> Ensuring Visual Studio 2022 Build Tools (with C++) is installed
    winget install --id Microsoft.VisualStudio.2022.BuildTools --exact --accept-source-agreements --accept-package-agreements 2>nul
    if errorlevel 1 (
      echo WARN: Could not install VS Build Tools via winget. Will try the C++ workload installer directly.
    )
  )
) else (
  echo ==^> Skipping winget installation.
)

echo ==^> Verifying tools
where node >nul 2>nul || (echo ERROR: node was not found. 1>&2 & exit /b 1)
where npm >nul 2>nul || (echo ERROR: npm was not found. 1>&2 & exit /b 1)
where cargo >nul 2>nul || (echo ERROR: cargo was not found. 1>&2 & exit /b 1)
where rustc >nul 2>nul || (echo ERROR: rustc was not found. 1>&2 & exit /b 1)

echo.
echo ==^> Ensuring Visual Studio C++ workload is installed
if "%NO_INSTALL%"=="0" (
  where cl.exe >nul 2>nul
  if errorlevel 1 (
    echo ==^> VS C++ tools not found on PATH. Attempting to install C++ workload.
    where vs_Installer.exe >nul 2>nul || where "VSInstaller.exe" >nul 2>nul || set "VSWHERE="
    if exist "%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vs_installer.exe" (
      set VSI="%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vs_installer.exe"
    ) else if exist "%ProgramFiles%\Microsoft Visual Studio\Installer\vs_installer.exe" (
      set VSI="%ProgramFiles%\Microsoft Visual Studio\Installer\vs_installer.exe"
    ) else (
      echo WARN: Visual Studio Installer not found. Download VS Build Tools:
      echo   https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022
      echo Then install manually with "Desktop development with C++" workload.
    )
    if defined VSI (
      echo Installing C++ workload (this may take a while)...
      %VSI% modify --quiet --wait --norestart --installPath "%ProgramFiles(x86)%\Microsoft Visual Studio\2022\BuildTools" --add Microsoft.VisualStudio.Workload.VCTools;includeRecommended 2>nul
      if errorlevel 1 (
        echo WARN: Could not install VS C++ workload automatically.
        echo Run this manually from an admin prompt:
        echo   "%VSI%" ^
        echo     modify --quiet --wait --norestart ^
        echo     --installPath "%ProgramFiles(x86)%\Microsoft Visual Studio\2022\BuildTools" ^
        echo     --add Microsoft.VisualStudio.Workload.VCTools;includeRecommended
        echo.
        echo After installation, open a "Developer Command Prompt for VS 2022".
      )
    )
  )
)

echo.
echo ==^> Checking MSVC build tools are available
where cl.exe >nul 2>nul
if errorlevel 1 (
  echo WARN: cl.exe (MSVC C++ compiler) still not found on PATH.
  echo.
  echo Tauri requires the MSVC toolchain. Open a "Developer Command Prompt for VS 2022"
  echo from the Start Menu, then rerun this script.
  echo.
) else (
  echo cl.exe found at:
  where cl.exe
)

echo ==^> Checking Rust Windows MSVC target
rustup target list --installed 2>nul | findstr "x86_64-pc-windows-msvc" >nul
if errorlevel 1 (
  echo ==^> Installing Rust MSVC target: x86_64-pc-windows-msvc
  rustup target add x86_64-pc-windows-msvc
)

echo.
node --version
npm --version
rustc --version
cargo --version

if "%SKIP_NPM%"=="0" (
  if exist package-lock.json (
    echo ==^> Installing frontend dependencies with npm ci
    call npm ci || exit /b 1
  ) else (
    echo ==^> package-lock.json not found. Installing frontend dependencies with npm install
    call npm install || exit /b 1
  )
) else (
  echo ==^> Skipping npm dependency installation.
)

if "%SKIP_CHECK%"=="0" (
  echo ==^> Running project check
  call npm run check || exit /b 1
)

echo ==^> Windows bootstrap complete.
popd >nul
exit /b 0
