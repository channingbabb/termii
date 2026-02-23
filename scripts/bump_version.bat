@echo off

setlocal EnableDelayedExpansion

:: locate repo root and cargo.toml
for /f "tokens=*" %%i in ('git rev-parse --show-toplevel 2^>nul') do set REPO_ROOT=%%i
if "!REPO_ROOT!"=="" (
    echo Error: Not inside a git repository.
    exit /b 1
)
:: convert forward slashes to backslashes
set REPO_ROOT=!REPO_ROOT:/=\!
set CARGO_TOML=!REPO_ROOT!\Cargo.toml

:: read the current version
for /f "usebackq tokens=*" %%v in (
    `powershell -NoProfile -Command "(Select-String -Path '!CARGO_TOML!' -Pattern '^version = ""(.+)""').Matches[0].Groups[1].Value"`
) do set CURRENT=%%v

if "!CURRENT!"=="" (
    echo Error: Could not read version from Cargo.toml.
    exit /b 1
)

if "%~1"=="" goto :usage

:: calc the new version
for /f "tokens=1,2,3 delims=." %%a in ("!CURRENT!") do (
    set MAJOR=%%a
    set MINOR=%%b
    set PATCH=%%c
)

if /i "%~1"=="patch" (
    set /a PATCH=!PATCH!+1
    set NEW_VERSION=!MAJOR!.!MINOR!.!PATCH!
) else if /i "%~1"=="minor" (
    set /a MINOR=!MINOR!+1
    set NEW_VERSION=!MAJOR!.!MINOR!.0
) else if /i "%~1"=="major" (
    set /a MAJOR=!MAJOR!+1
    set NEW_VERSION=!MAJOR!.0.0
) else (
    :: validate explicit x.y.z format
    powershell -NoProfile -Command "if ('%~1' -notmatch '^\d+\.\d+\.\d+$') { exit 1 }" >nul 2>&1
    if errorlevel 1 (
        echo Error: '%~1' is not a valid version. Use x.y.z or patch/minor/major.
        exit /b 1
    )
    set NEW_VERSION=%~1
)

echo Bumping version: !CURRENT! -^> !NEW_VERSION!

:: cases for when there are errors
git -C "!REPO_ROOT!" diff --quiet >nul 2>&1
if errorlevel 1 (
    echo Error: Working tree has uncommitted changes. Commit or stash them first.
    exit /b 1
)
git -C "!REPO_ROOT!" diff --cached --quiet >nul 2>&1
if errorlevel 1 (
    echo Error: Working tree has staged changes. Commit or stash them first.
    exit /b 1
)

git -C "!REPO_ROOT!" tag | findstr /x "v!NEW_VERSION!" >nul 2>&1
if not errorlevel 1 (
    echo Error: Tag v!NEW_VERSION! already exists.
    exit /b 1
)

:: update the version in cargo.toml
powershell -NoProfile -Command ^
    "(Get-Content '!CARGO_TOML!') -replace '^version = ""!CURRENT!""', 'version = ""!NEW_VERSION!""' | Set-Content '!CARGO_TOML!' -Encoding UTF8"

:: workflow to bump version
for /f "tokens=*" %%b in ('git -C "!REPO_ROOT!" rev-parse --abbrev-ref HEAD') do set BRANCH=%%b

git -C "!REPO_ROOT!" add "!CARGO_TOML!"
git -C "!REPO_ROOT!" commit -m "chore: bump version to !NEW_VERSION!"
git -C "!REPO_ROOT!" tag v!NEW_VERSION!
git -C "!REPO_ROOT!" push origin !BRANCH!
git -C "!REPO_ROOT!" push origin v!NEW_VERSION!

echo.
echo Done! v!NEW_VERSION! is tagged and pushed.
echo GitHub Actions will now build and publish the installers.
exit /b 0

:usage
echo Usage: bump_version.bat ^<patch^|minor^|major^|x.y.z^>
echo.
echo   patch    Increment patch  (e.g. 0.1.0 -^> 0.1.1)
echo   minor    Increment minor  (e.g. 0.1.0 -^> 0.2.0)
echo   major    Increment major  (e.g. 0.1.0 -^> 1.0.0)
echo   x.y.z    Set explicit version
echo.
echo Current version: !CURRENT!
exit /b 1
