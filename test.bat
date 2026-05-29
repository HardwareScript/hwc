@echo off
REM Strict clippy test script with optional full checks
REM Usage: 
REM   test.bat [cargo flags]           - Fast: clippy only
REM   test.bat --full [cargo flags]    - Full: fmt + clippy + tests
REM Examples:
REM   test.bat -p hwc-parser
REM   test.bat --workspace
REM   test.bat --full -p hwc-parser

REM Change to hwc directory if not already there
cd /d "%~dp0"

REM Check if --full flag is present
if "%1"=="--full" (
    echo Running full quality checks...
    echo.
    
    echo [1/4] Checking code formatting...
    cargo fmt --all -- --check
    if errorlevel 1 (
        echo.
        echo Formatting issues detected. Auto-fixing...
        cargo fmt --all
        if errorlevel 1 (
            echo.
            echo ERROR: Auto-format failed. Manual intervention required.
            exit /b 1
        )
        echo ✓ Auto-format successful. Re-running format check...
        cargo fmt --all -- --check
        if errorlevel 1 (
            echo.
            echo ERROR: Format check still failing after auto-fix.
            exit /b 1
        )
    )
    echo ✓ Formatting OK
    echo.
    
    echo [2/4] Running strict clippy...
    cargo clippy %2 %3 %4 %5 %6 %7 %8 %9 --all-targets -- -D warnings
    if errorlevel 1 (
        echo.
        echo Clippy check failed. Attempting auto-fix...
        cargo clippy %2 %3 %4 %5 %6 %7 %8 %9 --all-targets --fix --allow-dirty --allow-staged -- -D warnings
        if errorlevel 1 (
            echo.
            echo ERROR: Auto-fix failed. Manual intervention required.
            exit /b 1
        )
        echo ✓ Auto-fix successful. Re-running clippy check...
        cargo clippy %2 %3 %4 %5 %6 %7 %8 %9 --all-targets -- -D warnings
        if errorlevel 1 (
            echo.
            echo ERROR: Clippy check still failing after auto-fix.
            exit /b 1
        )
    )
    echo ✓ Clippy OK
    echo.
    
    echo [3/4] Running tests...
    cargo test %2 %3 %4 %5 %6 %7 %8 %9
    if errorlevel 1 (
        echo.
        echo ERROR: Tests failed.
        exit /b 1
    )
    echo ✓ Tests OK
    echo.
    echo All checks passed!
) else (
    REM Fast mode: clippy only
    cargo clippy %* --all-targets -- -D warnings
)
