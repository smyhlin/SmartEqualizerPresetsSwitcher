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
    echo WARN: winget not found. Install Node.js LTS, Rustup, and Microsoft Edge WebView2 Runtime manually if missing.
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
  )
) else (
  echo ==^> Skipping winget installation.
)

echo ==^> Verifying tools
where node >nul 2>nul || (echo ERROR: node was not found. Reopen terminal after install or install Node.js LTS manually. 1>&2 & exit /b 1)
where npm >nul 2>nul || (echo ERROR: npm was not found. Reopen terminal after install or install Node.js LTS manually. 1>&2 & exit /b 1)
where cargo >nul 2>nul || (echo ERROR: cargo was not found. Reopen terminal after install or install Rustup manually. 1>&2 & exit /b 1)
where rustc >nul 2>nul || (echo ERROR: rustc was not found. Reopen terminal after install or install Rustup manually. 1>&2 & exit /b 1)

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
